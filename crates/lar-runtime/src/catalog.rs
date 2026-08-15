use std::fs;
use std::path::{Path, PathBuf};

use lar_store::Store;

use crate::meta::{RuntimeMeta, RUNTIME_FORMAT};
use crate::Error;
use crate::Result;

/// A runtime discovered under `{prefix}/runtimes/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedRuntime {
    pub runtime_id: String,
    pub path: PathBuf,
    pub meta: RuntimeMeta,
}

/// List composed runtimes (sorted by `runtime_id`). Skips `.tmp-runtime-*`.
pub fn list(store: &Store) -> Result<Vec<ListedRuntime>> {
    cleanup_tmp_runtimes(store);

    let root = &store.paths().runtimes;
    let mut out = Vec::new();
    if !root.is_dir() {
        return Ok(out);
    }

    let entries = fs::read_dir(root).map_err(|source| Error::Io {
        path: root.clone(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| Error::Io {
            path: root.clone(),
            source,
        })?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let meta_path = path.join("runtime.toml");
        if !meta_path.is_file() {
            continue;
        }
        match load_runtime_meta(&meta_path) {
            Ok(meta) => {
                out.push(ListedRuntime {
                    runtime_id: meta.runtime_id.clone(),
                    path,
                    meta,
                });
            }
            Err(_) => continue,
        }
    }

    out.sort_by(|a, b| a.runtime_id.cmp(&b.runtime_id));
    Ok(out)
}

/// Inspect a runtime by id or by path to its directory / `runtime.toml`.
pub fn inspect(store: &Store, runtime: &Path) -> Result<ListedRuntime> {
    cleanup_tmp_runtimes(store);
    let path = resolve_runtime_path(store, runtime)?;
    let meta = load_runtime_meta(&path.join("runtime.toml"))?;
    Ok(ListedRuntime {
        runtime_id: meta.runtime_id.clone(),
        path,
        meta,
    })
}

/// Result of [`gc`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GcReport {
    /// Composed runtimes that were deleted (broken store refs, or `--all`).
    pub removed: Vec<ListedRuntime>,
    /// Corrupt / orphan directories under `runtimes/` that were deleted.
    pub orphans: Vec<PathBuf>,
    /// Valid runtimes left in place (only meaningful when not removing all).
    pub kept: usize,
}

impl GcReport {
    /// Total directories deleted (runtimes + orphans).
    pub fn total_removed(&self) -> usize {
        self.removed.len() + self.orphans.len()
    }
}

/// Garbage-collect disposable runtimes under the prefix.
///
/// - Always cleans `.tmp-runtime-*`.
/// - With `all = false` (default): removes runtimes whose locked packages are
///   missing from the store or whose `content_hash` no longer matches, and
///   removes unreadable/orphan runtime directories.
/// - With `all = true`: removes every composed runtime (they can be rebuilt
///   from a lockfile).
pub fn gc(store: &Store, all: bool) -> Result<GcReport> {
    cleanup_tmp_runtimes(store);

    let root = &store.paths().runtimes;
    if !root.is_dir() {
        return Ok(GcReport::default());
    }

    let mut report = GcReport::default();
    let candidates = collect_runtime_dirs(store)?;

    for candidate in candidates {
        let listed = match load_listed(&candidate) {
            Ok(rt) => rt,
            Err(_) => {
                // Orphan / corrupt directory — always collect.
                fs::remove_dir_all(&candidate).map_err(|source| Error::Io {
                    path: candidate.clone(),
                    source,
                })?;
                report.orphans.push(candidate);
                continue;
            }
        };

        let should_remove = all || runtime_is_broken(store, &listed)?;
        if should_remove {
            fs::remove_dir_all(&listed.path).map_err(|source| Error::Io {
                path: listed.path.clone(),
                source,
            })?;
            report.removed.push(listed);
        } else {
            report.kept += 1;
        }
    }

    report
        .removed
        .sort_by(|a, b| a.runtime_id.cmp(&b.runtime_id));
    report.orphans.sort();
    Ok(report)
}

fn collect_runtime_dirs(store: &Store) -> Result<Vec<PathBuf>> {
    let root = &store.paths().runtimes;
    let mut dirs = Vec::new();
    let entries = fs::read_dir(root).map_err(|source| Error::Io {
        path: root.clone(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| Error::Io {
            path: root.clone(),
            source,
        })?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            dirs.push(path);
        }
    }
    dirs.sort();
    Ok(dirs)
}

fn load_listed(path: &Path) -> Result<ListedRuntime> {
    let meta = load_runtime_meta(&path.join("runtime.toml"))?;
    Ok(ListedRuntime {
        runtime_id: meta.runtime_id.clone(),
        path: path.to_path_buf(),
        meta,
    })
}

fn runtime_is_broken(store: &Store, runtime: &ListedRuntime) -> Result<bool> {
    if runtime.meta.packages.is_empty() {
        return Ok(true);
    }
    for pkg in &runtime.meta.packages {
        let Some(stored) = store.get(&pkg.id, &pkg.version)? else {
            return Ok(true);
        };
        if stored.content_hash != pkg.content_hash {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Remove leftover `{runtimes}/.tmp-runtime-*` dirs from failed or crashed builds.
pub(crate) fn cleanup_tmp_runtimes(store: &Store) {
    let root = &store.paths().runtimes;
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(".tmp-runtime-") {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            let _ = fs::remove_dir_all(&path);
        } else {
            let _ = fs::remove_file(&path);
        }
    }
}

fn resolve_runtime_path(store: &Store, runtime: &Path) -> Result<PathBuf> {
    if runtime.is_dir() && runtime.join("runtime.toml").is_file() {
        return Ok(runtime.to_path_buf());
    }
    if runtime.is_file() && runtime.file_name().is_some_and(|n| n == "runtime.toml") {
        return runtime
            .parent()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| Error::Other("invalid runtime.toml path".into()));
    }

    // Treat as runtime_id under the prefix.
    let id = runtime
        .to_str()
        .filter(|s| !s.is_empty() && !s.contains('/'))
        .ok_or_else(|| Error::RuntimeNotFound(runtime.to_path_buf()))?;
    let path = store.paths().runtimes.join(id);
    if path.is_dir() && path.join("runtime.toml").is_file() {
        return Ok(path);
    }
    Err(Error::RuntimeNotFound(runtime.to_path_buf()))
}

fn load_runtime_meta(path: &Path) -> Result<RuntimeMeta> {
    let text = fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let meta: RuntimeMeta =
        toml::from_str(&text).map_err(|err| Error::InvalidRuntime(err.to_string()))?;
    if meta.format != RUNTIME_FORMAT {
        return Err(Error::InvalidRuntime(format!(
            "unsupported runtime format {} (supported: {RUNTIME_FORMAT})",
            meta.format
        )));
    }
    if meta.runtime_id.is_empty() {
        return Err(Error::InvalidRuntime("runtime_id must not be empty".into()));
    }
    Ok(meta)
}
