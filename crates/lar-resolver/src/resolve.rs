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
pub fn resolve_manifest(root: &PackageManifest, store: &Store) -> Result<Lockfile> {
    let mut ctx = ResolveCtx {
        store,
        resolved: BTreeMap::new(),
        packages: BTreeMap::new(),
        visiting: BTreeSet::new(),
    };

    ctx.visit_root(root)?;

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

impl<'a> ResolveCtx<'a> {
    fn visit_root(&mut self, root: &PackageManifest) -> Result<()> {
        let id = root.package.id.clone();
        let version = root.package.version.clone();
        let key = (id.clone(), version.clone());

        if !self.visiting.insert(key.clone()) {
            return Err(Error::Cycle { id, version });
        }

        self.resolved.insert(id.clone(), version.clone());
        for (dep_id, dep_req) in &root.dependencies {
            self.visit_dep(dep_id, dep_req)?;
        }

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

    fn visit_dep(&mut self, id: &str, req_str: &str) -> Result<()> {
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
            return Ok(());
        }

        let version = select_version(self.store, id, &req, req_str)?;
        let key = (id.to_string(), version.clone());

        if !self.visiting.insert(key.clone()) {
            return Err(Error::Cycle {
                id: id.to_string(),
                version: version.clone(),
            });
        }

        let stored = match self.store.get(id, &version)? {
            Some(existing) => {
                lar_repo::emit_store_hit_warnings(
                    self.store,
                    id,
                    &version,
                    Some(&existing.content_hash),
                    &mut std::io::stderr(),
                )?;
                existing
            }
            None => lar_repo::fetch_into_store(
                self.store,
                id,
                &version,
                lar_repo::LookupMode::Deps,
                &mut std::io::stderr(),
            )
            .map_err(|err| match err {
                lar_repo::Error::PackageNotFound { id, version } => Error::Missing { id, version },
                other => other.into(),
            })?,
        };

        let dep_manifest = load_manifest(&stored.path.join("package.toml"))?;
        if dep_manifest.package.id != id || dep_manifest.package.version != version {
            return Err(Error::Other(format!(
                "stored package at {} has id/version {} {}, expected {} {}",
                stored.path.display(),
                dep_manifest.package.id,
                dep_manifest.package.version,
                id,
                version
            )));
        }

        self.resolved.insert(id.to_string(), version.clone());
        for (child_id, child_req) in &dep_manifest.dependencies {
            self.visit_dep(child_id, child_req)?;
        }

        self.packages.insert(
            key.clone(),
            LockedPackage {
                id: id.to_string(),
                version,
                content_hash: Some(stored.content_hash),
                dependencies: dep_manifest.dependencies,
            },
        );
        self.visiting.remove(&key);
        Ok(())
    }
}

fn select_version(store: &Store, id: &str, req: &VersionReq, req_str: &str) -> Result<String> {
    let candidates = lar_repo::list_dep_versions(store, id)?;
    let mut best: Option<(Version, String)> = None;
    for ver_str in candidates {
        let Ok(ver) = Version::parse(&ver_str) else {
            continue;
        };
        if !req.matches(&ver) {
            continue;
        }
        match &best {
            None => best = Some((ver, ver_str)),
            Some((prev, _)) if ver > *prev => best = Some((ver, ver_str)),
            _ => {}
        }
    }
    match best {
        Some((_, version)) => Ok(version),
        None => Err(Error::Unsatisfiable {
            id: id.to_string(),
            req: req_str.to_string(),
        }),
    }
}
