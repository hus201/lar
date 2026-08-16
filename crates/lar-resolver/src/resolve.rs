use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use lar_package::{load_manifest, resolve_manifest_path, PackageManifest};
use lar_store::Store;
use semver::{Version, VersionReq};

use crate::lockfile::{LockRoot, LockedPackage, Lockfile, LOCKFILE_FORMAT};
use crate::Error;
use crate::Result;

/// Resolve dependencies for a root `package.toml` against `store`.
pub fn resolve(manifest_path: &Path, store: &Store) -> Result<Lockfile> {
    let manifest_path = resolve_manifest_path(manifest_path)?;
    let root_manifest = load_manifest(&manifest_path)?;
    resolve_manifest(&root_manifest, store)
}

/// Resolve from an already-loaded root manifest.
///
/// Picks the highest matching version for each package id, and backtracks to
/// older candidates when a later requirement makes the choice unsatisfiable.
/// Still enforces one version per id.
pub fn resolve_manifest(root: &PackageManifest, store: &Store) -> Result<Lockfile> {
    let mut ctx = ResolveCtx {
        store,
        resolved: BTreeMap::new(),
        packages: BTreeMap::new(),
        visiting: BTreeSet::new(),
    };

    ctx.solve_root(root)?;

    let mut locked_packages: Vec<LockedPackage> = ctx.packages.into_values().collect();
    locked_packages.sort_by(|a, b| (&a.id, &a.version).cmp(&(&b.id, &b.version)));

    let lock = Lockfile {
        format: LOCKFILE_FORMAT,
        root: LockRoot {
            id: root.package.id.clone(),
            version: root.package.version.clone(),
        },
        packages: locked_packages,
    };
    lock.validate()?;
    Ok(lock)
}

struct ResolveCtx<'a> {
    store: &'a Store,
    /// id -> exact version chosen for the graph
    resolved: BTreeMap<String, String>,
    packages: BTreeMap<(String, String), LockedPackage>,
    visiting: BTreeSet<(String, String)>,
}

#[derive(Clone)]
struct Checkpoint {
    resolved: BTreeMap<String, String>,
    packages: BTreeMap<(String, String), LockedPackage>,
    visiting: BTreeSet<(String, String)>,
}

impl<'a> ResolveCtx<'a> {
    fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            resolved: self.resolved.clone(),
            packages: self.packages.clone(),
            visiting: self.visiting.clone(),
        }
    }

    fn restore(&mut self, cp: Checkpoint) {
        self.resolved = cp.resolved;
        self.packages = cp.packages;
        self.visiting = cp.visiting;
    }

    fn solve_root(&mut self, root: &PackageManifest) -> Result<()> {
        let id = root.package.id.clone();
        let version = root.package.version.clone();
        let key = (id.clone(), version.clone());

        if !self.visiting.insert(key.clone()) {
            return Err(Error::Cycle { id, version });
        }

        self.resolved.insert(id.clone(), version.clone());

        let deps: Vec<(String, String)> = root
            .dependencies
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        self.select_deps(&deps)?;

        self.packages.insert(
            key.clone(),
            LockedPackage {
                id,
                version,
                content_hash: root.package.content_hash.clone(),
                dependencies: root.dependencies.clone(),
            },
        );
        self.visiting.remove(&key);
        Ok(())
    }

    /// Satisfy `deps` left-to-right. Failures in later deps backtrack into earlier choices.
    fn select_deps(&mut self, deps: &[(String, String)]) -> Result<()> {
        if deps.is_empty() {
            return Ok(());
        }
        self.select_one(&deps[0].0, &deps[0].1, &deps[1..])
    }

    fn select_one(&mut self, id: &str, req_str: &str, rest: &[(String, String)]) -> Result<()> {
        let req = VersionReq::parse(req_str).map_err(|err| {
            Error::Other(format!(
                "invalid version requirement `{req_str}` for {id}: {err}"
            ))
        })?;

        if let Some(existing) = self.resolved.get(id).cloned() {
            let existing_ver = Version::parse(&existing).map_err(|err| {
                Error::Other(format!(
                    "resolved version `{existing}` for {id} is not semver: {err}"
                ))
            })?;
            if !req.matches(&existing_ver) {
                return Err(Error::Conflict {
                    id: id.to_string(),
                    required: req_str.to_string(),
                    resolved: existing,
                });
            }
            let key = (id.to_string(), existing);
            if self.visiting.contains(&key) {
                return Err(Error::Cycle {
                    id: id.to_string(),
                    version: key.1,
                });
            }
            return self.select_deps(rest);
        }

        let candidates = matching_versions(self.store, id, &req)?;
        if candidates.is_empty() {
            return Err(Error::Unsatisfiable {
                id: id.to_string(),
                req: req_str.to_string(),
            });
        }

        let mut last_err = None;
        for version in &candidates {
            let cp = self.checkpoint();
            match self.expand(id, version, rest) {
                Ok(()) => return Ok(()),
                Err(err) if is_search_failure(&err) => {
                    self.restore(cp);
                    last_err = Some(err);
                }
                Err(err) => {
                    self.restore(cp);
                    return Err(err);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| Error::Unsatisfiable {
            id: id.to_string(),
            req: req_str.to_string(),
        }))
    }

    fn expand(&mut self, id: &str, version: &str, rest: &[(String, String)]) -> Result<()> {
        let key = (id.to_string(), version.to_string());

        if !self.visiting.insert(key.clone()) {
            return Err(Error::Cycle {
                id: id.to_string(),
                version: version.to_string(),
            });
        }

        let stored = match self.store.get(id, version)? {
            Some(existing) => {
                lar_repo::emit_store_hit_warnings(
                    self.store,
                    id,
                    version,
                    Some(&existing.content_hash),
                    &mut std::io::stderr(),
                )?;
                existing
            }
            None => lar_repo::fetch_into_store(
                self.store,
                id,
                version,
                &mut std::io::stderr(),
            )
            .map_err(|err| match err {
                lar_repo::Error::PackageNotFound { id, version } => Error::Missing { id, version },
                other => other.into(),
            })?,
        };

        let dep_manifest = load_manifest(&stored.path.join("package.toml"))?;
        if dep_manifest.package.id != id || dep_manifest.package.version != version {
            self.visiting.remove(&key);
            return Err(Error::Other(format!(
                "stored package at {} has id/version {} {}, expected {} {}",
                stored.path.display(),
                dep_manifest.package.id,
                dep_manifest.package.version,
                id,
                version
            )));
        }

        self.resolved.insert(id.to_string(), version.to_string());

        let child_deps: Vec<(String, String)> = dep_manifest
            .dependencies
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        self.select_deps(&child_deps)?;

        self.packages.insert(
            key.clone(),
            LockedPackage {
                id: id.to_string(),
                version: version.to_string(),
                content_hash: Some(stored.content_hash),
                dependencies: dep_manifest.dependencies,
            },
        );
        self.visiting.remove(&key);

        self.select_deps(rest)
    }
}

fn is_search_failure(err: &Error) -> bool {
    matches!(
        err,
        Error::Conflict { .. }
            | Error::Unsatisfiable { .. }
            | Error::Cycle { .. }
            | Error::Missing { .. }
    )
}

/// Matching versions of `id`, highest semver first.
fn matching_versions(store: &Store, id: &str, req: &VersionReq) -> Result<Vec<String>> {
    let candidates = lar_repo::list_dep_versions(store, id)?;
    let mut matched: Vec<(Version, String)> = Vec::new();
    for ver_str in candidates {
        let Ok(ver) = Version::parse(&ver_str) else {
            continue;
        };
        if req.matches(&ver) {
            matched.push((ver, ver_str));
        }
    }
    matched.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(matched.into_iter().map(|(_, s)| s).collect())
}
