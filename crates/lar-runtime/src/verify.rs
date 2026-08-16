//! Verify that a composed runtime's `files/` tree still matches the store.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use lar_package::verify_package_dir;
use lar_store::Store;

use crate::catalog::{inspect, ListedRuntime};
use crate::compose::ComposeMode;
use crate::meta::RuntimeMeta;
use crate::Error;
use crate::Result;

/// Successful filesystem verification of a composed runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyReport {
    pub runtime_id: String,
    pub path: PathBuf,
    pub compose: ComposeMode,
    pub packages_checked: usize,
    pub files_checked: usize,
}

/// Verify a runtime by id or path (`runtime.toml` / directory).
pub fn verify(store: &Store, runtime: &Path) -> Result<VerifyReport> {
    let listed = inspect(store, runtime)?;
    verify_listed(store, &listed)
}

/// Verify an already-loaded runtime directory + metadata.
pub fn verify_listed(store: &Store, listed: &ListedRuntime) -> Result<VerifyReport> {
    verify_runtime_dir(store, &listed.path, &listed.meta)
}

/// Verify `{runtime_dir}/files` against store packages in `meta`.
pub fn verify_runtime_dir(
    store: &Store,
    runtime_dir: &Path,
    meta: &RuntimeMeta,
) -> Result<VerifyReport> {
    if meta.packages.is_empty() {
        return Err(Error::VerifyFailed(format!(
            "runtime {} has no packages",
            meta.runtime_id
        )));
    }

    let files_root = runtime_dir.join("files");
    if !files_root.is_dir() {
        return Err(Error::VerifyFailed(format!(
            "runtime {} missing files/ directory",
            meta.runtime_id
        )));
    }

    let mut expected = BTreeSet::new();
    let mut files_checked = 0usize;
    let mut packages: Vec<_> = meta.packages.iter().collect();
    packages.sort_by(|a, b| (&a.id, &a.version).cmp(&(&b.id, &b.version)));

    for pkg in packages {
        let Some(stored) = store.get(&pkg.id, &pkg.version)? else {
            return Err(Error::VerifyFailed(format!(
                "package {} {} missing from store",
                pkg.id, pkg.version
            )));
        };
        if stored.content_hash != pkg.content_hash {
            return Err(Error::VerifyFailed(format!(
                "package {} {} content_hash mismatch: runtime has {}, store has {}",
                pkg.id, pkg.version, pkg.content_hash, stored.content_hash
            )));
        }
        // Ensure the store tree itself is intact before comparing the runtime.
        verify_package_dir(&stored.path).map_err(|err| {
            Error::VerifyFailed(format!(
                "store package {} {} failed integrity check: {err}",
                pkg.id, pkg.version
            ))
        })?;

        let pkg_files = stored.path.join("files");
        if !pkg_files.exists() {
            continue;
        }
        let label = format!("{} {}", pkg.id, pkg.version);
        for rel in collect_file_rels(&pkg_files)? {
            let store_file = pkg_files.join(&rel);
            let runtime_file = files_root.join(&rel);
            check_placed_file(meta.compose, &store_file, &runtime_file, &label, &rel)?;
            expected.insert(rel);
            files_checked += 1;
        }
    }

    for rel in collect_file_rels(&files_root)? {
        if !expected.contains(&rel) {
            return Err(Error::VerifyFailed(format!(
                "unexpected file in runtime files/: {rel}"
            )));
        }
    }

    Ok(VerifyReport {
        runtime_id: meta.runtime_id.clone(),
        path: runtime_dir.to_path_buf(),
        compose: meta.compose,
        packages_checked: meta.packages.len(),
        files_checked,
    })
}

fn check_placed_file(
    compose: ComposeMode,
    store_file: &Path,
    runtime_file: &Path,
    package_label: &str,
    rel: &str,
) -> Result<()> {
    let meta = fs::symlink_metadata(runtime_file).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            Error::VerifyFailed(format!(
                "missing runtime file `{rel}` (from {package_label})"
            ))
        } else {
            Error::Io {
                path: runtime_file.to_path_buf(),
                source,
            }
        }
    })?;

    match compose {
        ComposeMode::Symlink => {
            if !meta.file_type().is_symlink() {
                return Err(Error::VerifyFailed(format!(
                    "`{rel}` should be a symlink (compose=symlink)"
                )));
            }
            let actual = fs::canonicalize(runtime_file).map_err(|source| {
                Error::VerifyFailed(format!("dangling or unreadable symlink `{rel}`: {source}"))
            })?;
            let expected = fs::canonicalize(store_file).map_err(|source| Error::Io {
                path: store_file.to_path_buf(),
                source,
            })?;
            if actual != expected {
                return Err(Error::VerifyFailed(format!(
                    "`{rel}` symlink target mismatch: got {}, expected {} ({package_label})",
                    actual.display(),
                    expected.display()
                )));
            }
        }
        ComposeMode::Hardlink => {
            if meta.file_type().is_symlink() {
                return Err(Error::VerifyFailed(format!(
                    "`{rel}` should be a hardlink, found symlink (compose=hardlink)"
                )));
            }
            if !meta.is_file() {
                return Err(Error::VerifyFailed(format!(
                    "`{rel}` should be a hardlinked file (compose=hardlink)"
                )));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let store_meta = fs::metadata(store_file).map_err(|source| Error::Io {
                    path: store_file.to_path_buf(),
                    source,
                })?;
                if meta.dev() != store_meta.dev() || meta.ino() != store_meta.ino() {
                    return Err(Error::VerifyFailed(format!(
                        "`{rel}` is not a hardlink to the store file ({package_label})"
                    )));
                }
            }
            #[cfg(not(unix))]
            {
                let a = fs::read(runtime_file).map_err(|source| Error::Io {
                    path: runtime_file.to_path_buf(),
                    source,
                })?;
                let b = fs::read(store_file).map_err(|source| Error::Io {
                    path: store_file.to_path_buf(),
                    source,
                })?;
                if a != b {
                    return Err(Error::VerifyFailed(format!(
                        "`{rel}` content does not match store ({package_label})"
                    )));
                }
            }
        }
        ComposeMode::Copy => {
            if meta.file_type().is_symlink() {
                return Err(Error::VerifyFailed(format!(
                    "`{rel}` should be a regular file, found symlink (compose=copy)"
                )));
            }
            if !meta.is_file() {
                return Err(Error::VerifyFailed(format!(
                    "`{rel}` should be a regular file (compose=copy)"
                )));
            }
            let a = fs::read(runtime_file).map_err(|source| Error::Io {
                path: runtime_file.to_path_buf(),
                source,
            })?;
            let b = fs::read(store_file).map_err(|source| Error::Io {
                path: store_file.to_path_buf(),
                source,
            })?;
            if a != b {
                return Err(Error::VerifyFailed(format!(
                    "`{rel}` content does not match store ({package_label})"
                )));
            }
        }
    }
    Ok(())
}

fn collect_file_rels(root: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    collect_file_rels_into(root, root, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect_file_rels_into(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
    let entries = fs::read_dir(dir).map_err(|source| Error::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| Error::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        if file_type.is_dir() {
            collect_file_rels_into(root, &path, out)?;
            continue;
        }
        // Count symlinks and regular files (payload entries).
        if !(file_type.is_file() || file_type.is_symlink()) {
            return Err(Error::VerifyFailed(format!(
                "unsupported file type in runtime tree: {}",
                path.display()
            )));
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|_| Error::Other(format!("path outside root: {}", path.display())))?;
        let rel_str = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if rel_str.is_empty() || rel_str.contains('\0') {
            return Err(Error::Other(format!("invalid relative path: {rel_str}")));
        }
        out.push(rel_str);
    }
    Ok(())
}
