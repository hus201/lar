use lar_package::load_manifest;
use lar_store::Store;

use crate::lockfile::Lockfile;
use crate::Error;
use crate::Result;

/// Verify that a lockfile matches packages currently in the store.
///
/// - Validates lockfile structure (including required non-root `content_hash`).
/// - Root without `content_hash` is allowed to be absent from the store.
/// - Every package with a `content_hash` must exist in the store at that
///   `(id, version)` with the same hash.
/// - Locked `dependencies` must match the stored package’s `[dependencies]`.
pub fn verify_lockfile(lock: &Lockfile, store: &Store) -> Result<()> {
    verify_lockfile_inner(lock, store, false)
}

/// Like [`verify_lockfile`], but the root package must also have a `content_hash`
/// and be present in the store (required before runtime composition).
pub fn verify_lockfile_ready(lock: &Lockfile, store: &Store) -> Result<()> {
    verify_lockfile_inner(lock, store, true)
}

fn verify_lockfile_inner(
    lock: &Lockfile,
    store: &Store,
    require_root_in_store: bool,
) -> Result<()> {
    lock.validate()?;

    for pkg in &lock.packages {
        let is_root = pkg.id == lock.root.id && pkg.version == lock.root.version;
        let Some(expected_hash) = &pkg.content_hash else {
            if is_root && !require_root_in_store {
                continue;
            }
            return Err(Error::InvalidLockfile(format!(
                "package {} {} missing required content_hash",
                pkg.id, pkg.version
            )));
        };

        let stored = store
            .get(&pkg.id, &pkg.version)?
            .ok_or_else(|| Error::Missing {
                id: pkg.id.clone(),
                version: pkg.version.clone(),
            })?;

        if &stored.content_hash != expected_hash {
            return Err(Error::HashMismatch {
                id: pkg.id.clone(),
                version: pkg.version.clone(),
                locked: expected_hash.clone(),
                store: stored.content_hash,
            });
        }

        let manifest = load_manifest(&stored.path.join("package.toml"))?;
        if manifest.dependencies != pkg.dependencies {
            return Err(Error::DependencyMismatch {
                id: pkg.id.clone(),
                version: pkg.version.clone(),
            });
        }
    }

    Ok(())
}
