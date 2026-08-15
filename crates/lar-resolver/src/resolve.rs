use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use lar_package::{load_manifest, resolve_manifest_path, PackageManifest};
use lar_store::Store;

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
    /// id -> version chosen for the graph
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
        for (dep_id, dep_ver) in &root.dependencies {
            self.visit_dep(dep_id, dep_ver)?;
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

    fn visit_dep(&mut self, id: &str, version: &str) -> Result<()> {
        let key = (id.to_string(), version.to_string());

        if let Some(existing) = self.resolved.get(id) {
            if existing != version {
                return Err(Error::Conflict {
                    id: id.to_string(),
                    required: version.to_string(),
                    resolved: existing.clone(),
                });
            }
            // Already fully resolved, or currently on the DFS stack (cycle).
            if self.visiting.contains(&key) {
                return Err(Error::Cycle {
                    id: id.to_string(),
                    version: version.to_string(),
                });
            }
            return Ok(());
        }

        if !self.visiting.insert(key.clone()) {
            return Err(Error::Cycle {
                id: id.to_string(),
                version: version.to_string(),
            });
        }

        let stored = self.store.get(id, version)?.ok_or_else(|| Error::Missing {
            id: id.to_string(),
            version: version.to_string(),
        })?;

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

        self.resolved.insert(id.to_string(), version.to_string());
        for (child_id, child_ver) in &dep_manifest.dependencies {
            self.visit_dep(child_id, child_ver)?;
        }

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
        Ok(())
    }
}
