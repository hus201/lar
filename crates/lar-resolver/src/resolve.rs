use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io;
use std::path::Path;

use lar_package::{load_manifest, resolve_manifest_path, PackageManifest};
use lar_repo::ResolvePackage;
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
/// Searches with highest-matching versions first and backtracks on conflict.
/// Candidate packages are inspected without adding failed tries to the store;
/// only the winning set is materialized.
pub fn resolve_manifest(root: &PackageManifest, store: &Store) -> Result<Lockfile> {
    let mut ctx = ResolveCtx {
        store,
        resolved: BTreeMap::new(),
        packages: BTreeMap::new(),
        visiting: BTreeSet::new(),
        trail: Vec::new(),
        cache: HashMap::new(),
    };

    ctx.solve_root(root)?;
    ctx.materialize()?;

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

struct Frame {
    id: String,
    version: String,
}

struct ResolveCtx<'a> {
    store: &'a Store,
    resolved: BTreeMap<String, String>,
    packages: BTreeMap<(String, String), LockedPackage>,
    visiting: BTreeSet<(String, String)>,
    /// Assignment order for O(depth) undo instead of cloning maps.
    trail: Vec<Frame>,
    cache: HashMap<(String, String), ResolvePackage>,
}

impl<'a> ResolveCtx<'a> {
    fn trail_len(&self) -> usize {
        self.trail.len()
    }

    fn undo_to(&mut self, len: usize) {
        while self.trail.len() > len {
            let frame = self.trail.pop().expect("trail non-empty");
            self.resolved.remove(&frame.id);
            self.packages
                .remove(&(frame.id.clone(), frame.version.clone()));
            self.visiting
                .remove(&(frame.id.clone(), frame.version.clone()));
        }
    }

    fn push_assignment(&mut self, id: &str, version: &str) {
        self.resolved.insert(id.to_string(), version.to_string());
        self.trail.push(Frame {
            id: id.to_string(),
            version: version.to_string(),
        });
    }

    fn solve_root(&mut self, root: &PackageManifest) -> Result<()> {
        let id = root.package.id.clone();
        let version = root.package.version.clone();
        let key = (id.clone(), version.clone());

        if !self.visiting.insert(key.clone()) {
            return Err(Error::Cycle { id, version });
        }
        self.push_assignment(&id, &version);

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

        let mark = self.trail_len();
        let mut failures: Vec<(String, Error)> = Vec::new();
        for version in &candidates {
            match self.expand(id, version, rest) {
                Ok(()) => return Ok(()),
                Err(err) if is_candidate_failure(&err) => {
                    self.undo_to(mark);
                    failures.push((version.clone(), err));
                }
                Err(err) => {
                    self.undo_to(mark);
                    return Err(err);
                }
            }
        }

        Err(summarize_failures(id, req_str, failures))
    }

    fn expand(&mut self, id: &str, version: &str, rest: &[(String, String)]) -> Result<()> {
        let key = (id.to_string(), version.to_string());

        if !self.visiting.insert(key.clone()) {
            return Err(Error::Cycle {
                id: id.to_string(),
                version: version.to_string(),
            });
        }

        let pkg = match self.load_cached(id, version) {
            Ok(pkg) => pkg.clone(),
            Err(err) => {
                self.visiting.remove(&key);
                return Err(err);
            }
        };
        self.push_assignment(id, version);

        let child_deps: Vec<(String, String)> = pkg
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
                content_hash: Some(pkg.content_hash),
                dependencies: pkg.dependencies,
            },
        );
        self.visiting.remove(&key);

        self.select_deps(rest)
    }

    fn load_cached(&mut self, id: &str, version: &str) -> Result<&ResolvePackage> {
        let key = (id.to_string(), version.to_string());
        if !self.cache.contains_key(&key) {
            // Discard advisory noise while probing candidates; materialize warns.
            let mut sink = io::sink();
            let pkg = lar_repo::load_package_for_resolve(self.store, id, version, &mut sink)
                .map_err(|err| match err {
                    lar_repo::Error::PackageNotFound { id, version } => {
                        Error::Missing { id, version }
                    }
                    other => other.into(),
                })?;
            self.cache.insert(key.clone(), pkg);
        }
        Ok(self.cache.get(&key).expect("cache insert"))
    }

    /// Fetch winning packages into the store (failed search candidates were never added).
    fn materialize(&mut self) -> Result<()> {
        let mut warn_out = io::stderr();
        let pins: Vec<(String, String, Option<String>)> = self
            .packages
            .values()
            .map(|p| (p.id.clone(), p.version.clone(), p.content_hash.clone()))
            .collect();

        for (id, version, expected_hash) in pins {
            let Some(expected_hash) = expected_hash else {
                continue;
            };
            let stored = if let Some(existing) = self.store.get(&id, &version)? {
                lar_repo::emit_store_hit_warnings(
                    self.store,
                    &id,
                    &version,
                    Some(&existing.content_hash),
                    &mut warn_out,
                )?;
                existing
            } else {
                lar_repo::fetch_into_store(self.store, &id, &version, &mut warn_out).map_err(
                    |err| match err {
                        lar_repo::Error::PackageNotFound { id, version } => {
                            Error::Missing { id, version }
                        }
                        other => other.into(),
                    },
                )?
            };
            if stored.content_hash != expected_hash {
                return Err(Error::HashMismatch {
                    id,
                    version,
                    locked: expected_hash,
                    store: stored.content_hash,
                });
            }
        }
        Ok(())
    }
}

/// Failures that mean "try the next candidate," not abort the whole resolve.
fn is_candidate_failure(err: &Error) -> bool {
    match err {
        Error::Conflict { .. }
        | Error::Unsatisfiable { .. }
        | Error::Unresolvable(_)
        | Error::Missing { .. } => true,
        // Yanked / not found while probing a listed candidate → try another version.
        Error::Repo(lar_repo::Error::PackageNotFound { .. })
        | Error::Repo(lar_repo::Error::Yanked { .. }) => true,
        // Cycles are structural for this path; do not burn other candidates hoping they help.
        Error::Cycle { .. } => false,
        _ => false,
    }
}

fn summarize_failures(id: &str, req: &str, failures: Vec<(String, Error)>) -> Error {
    if failures.is_empty() {
        return Error::Unsatisfiable {
            id: id.to_string(),
            req: req.to_string(),
        };
    }
    if failures.len() == 1 {
        return failures.into_iter().next().unwrap().1;
    }

    let mut lines = vec![format!(
        "could not resolve {id} requiring `{req}` (tried {}):",
        failures
            .iter()
            .map(|(v, _)| v.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    )];
    for (version, err) in &failures {
        lines.push(format!("  - {version}: {err}"));
    }
    Error::Unresolvable(lines.join("\n"))
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
