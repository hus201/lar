use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::{SystemTime, UNIX_EPOCH};

use lar_package::load_manifest;
use lar_resolver::{load_lockfile, verify_lockfile_ready, LockedPackage, Lockfile};
use lar_store::Store;

use crate::catalog::cleanup_tmp_runtimes;
use crate::compose::ComposeMode;
use crate::meta::{RuntimeMeta, RuntimePackage, RuntimeRoot, RUNTIME_FORMAT};
use crate::Error;
use crate::Result;

/// A composed (or reused) runtime directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltRuntime {
    pub runtime_id: String,
    pub path: PathBuf,
    pub reused: bool,
    pub meta: RuntimeMeta,
}

/// Resolve a path that may be a `lar.lock` file or a directory containing it.
pub fn resolve_lockfile_path(path: &Path) -> Result<PathBuf> {
    if path.is_dir() {
        let candidate = path.join("lar.lock");
        if candidate.is_file() {
            return Ok(candidate);
        }
        return Err(Error::LockfileNotFound(candidate));
    }
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    Err(Error::LockfileNotFound(path.to_path_buf()))
}

/// Content-addressed runtime id for a lockfile + compose mode.
pub fn runtime_id(lock: &Lockfile, compose: ComposeMode) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"lar-runtime-v1\0");
    hasher.update(compose.as_str().as_bytes());
    hasher.update(b"\0");
    hasher.update(lock.root.id.as_bytes());
    hasher.update(b"\0");
    hasher.update(lock.root.version.as_bytes());
    hasher.update(b"\0");
    let mut packages = lock.packages.clone();
    packages.sort_by(|a, b| (&a.id, &a.version).cmp(&(&b.id, &b.version)));
    for pkg in &packages {
        hasher.update(pkg.id.as_bytes());
        hasher.update(b"\0");
        hasher.update(pkg.version.as_bytes());
        hasher.update(b"\0");
        let hash = pkg.content_hash.as_deref().unwrap_or("");
        hasher.update(hash.as_bytes());
        hasher.update(b"\0");
    }
    hasher.finalize().to_hex().to_string()
}

/// Build or reuse a runtime for `lock_path` against `store`.
pub fn build(lock_path: &Path, store: &Store, compose: ComposeMode) -> Result<BuiltRuntime> {
    cleanup_tmp_runtimes(store);

    let lock_path = resolve_lockfile_path(lock_path)?;
    let lock = load_lockfile(&lock_path)?;
    verify_lockfile_ready(&lock, store)?;

    let id = runtime_id(&lock, compose);
    let final_path = store.paths().runtimes.join(&id);
    let meta = meta_from_lock(&lock, &id, compose)?;

    if final_path.is_dir() {
        let existing = final_path.join("runtime.toml");
        if existing.is_file() {
            let text = fs::read_to_string(&existing).map_err(|source| Error::Io {
                path: existing.clone(),
                source,
            })?;
            if let Ok(existing_meta) = toml::from_str::<RuntimeMeta>(&text) {
                if existing_meta == meta {
                    return Ok(BuiltRuntime {
                        runtime_id: id,
                        path: final_path,
                        reused: true,
                        meta: existing_meta,
                    });
                }
            }
        }
        // Stale or mismatched — remove and rebuild.
        fs::remove_dir_all(&final_path).map_err(|source| Error::Io {
            path: final_path.clone(),
            source,
        })?;
    }

    fs::create_dir_all(&store.paths().runtimes).map_err(|source| Error::Io {
        path: store.paths().runtimes.clone(),
        source,
    })?;

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = store.paths().runtimes.join(format!(".tmp-runtime-{nanos}"));
    if tmp.exists() {
        let _ = fs::remove_dir_all(&tmp);
    }
    fs::create_dir_all(tmp.join("files")).map_err(|source| Error::Io {
        path: tmp.join("files"),
        source,
    })?;

    if let Err(err) = compose_files(&lock, store, &tmp.join("files"), compose) {
        let _ = fs::remove_dir_all(&tmp);
        return Err(err);
    }

    let meta_path = tmp.join("runtime.toml");
    let text = toml::to_string_pretty(&meta)
        .map_err(|err| Error::Other(format!("serialize runtime.toml: {err}")))?;
    if let Err(source) = fs::write(&meta_path, text) {
        let _ = fs::remove_dir_all(&tmp);
        return Err(Error::Io {
            path: meta_path,
            source,
        });
    }

    if let Err(source) = fs::rename(&tmp, &final_path) {
        let _ = fs::remove_dir_all(&tmp);
        return Err(Error::Io {
            path: final_path,
            source,
        });
    }

    Ok(BuiltRuntime {
        runtime_id: id,
        path: final_path,
        reused: false,
        meta,
    })
}

/// Build/reuse a runtime and execute the root entry binary.
pub fn run(
    lock_path: &Path,
    store: &Store,
    compose: ComposeMode,
    args: &[String],
) -> Result<ExitStatus> {
    let built = build(lock_path, store, compose)?;
    run_runtime_entry(
        store,
        &built.path,
        &built.meta.root.id,
        &built.meta.root.version,
        None,
        args,
    )
}

/// Execute the entry binary of a composed runtime for the given root package.
///
/// When `binary` is `None`, uses the entry default (or sole) binary.
pub fn run_runtime_entry(
    store: &Store,
    runtime_path: &Path,
    root_id: &str,
    root_version: &str,
    binary: Option<&str>,
    args: &[String],
) -> Result<ExitStatus> {
    let root = store
        .get(root_id, root_version)?
        .ok_or_else(|| lar_store::Error::NotFound {
            id: root_id.to_string(),
            version: root_version.to_string(),
        })?;
    let manifest = load_manifest(&root.path.join("package.toml"))?;
    let entry = manifest.entry.as_ref().ok_or_else(|| Error::NoEntry {
        id: root_id.to_string(),
        version: root_version.to_string(),
    })?;
    let binary = if let Some(rel) = binary {
        if !entry.binaries.iter().any(|b| b == rel) {
            return Err(Error::EntryMissing(rel.to_string()));
        }
        rel
    } else {
        entry
            .default
            .as_deref()
            .or_else(|| entry.binaries.first().map(String::as_str))
            .ok_or_else(|| Error::NoEntry {
                id: root_id.to_string(),
                version: root_version.to_string(),
            })?
    };

    let exe = runtime_path.join("files").join(binary);
    if !exe.exists() {
        return Err(Error::EntryMissing(binary.to_string()));
    }

    let mut cmd = Command::new(&exe);
    cmd.args(args);
    apply_runtime_env(&mut cmd, runtime_path);
    cmd.status()
        .map_err(|source| Error::Io { path: exe, source })
}

fn apply_runtime_env(cmd: &mut Command, runtime_path: &Path) {
    let env = runtime_launch_env(runtime_path);
    if !env.path_prepend.is_empty() {
        let current = std::env::var_os("PATH").unwrap_or_default();
        let mut new_path = env.path_prepend.clone();
        if !current.is_empty() {
            new_path.push(":");
            new_path.push(current);
        }
        cmd.env("PATH", new_path);
    }

    if !env.ld_library_path_prepend.is_empty() {
        let current = std::env::var_os("LD_LIBRARY_PATH").unwrap_or_default();
        let mut new_path = env.ld_library_path_prepend.clone();
        if !current.is_empty() {
            new_path.push(":");
            new_path.push(current);
        }
        cmd.env("LD_LIBRARY_PATH", new_path);
    }

    cmd.env("LAR_RUNTIME", &env.lar_runtime);
}

pub use lar_trampoline::{runtime_launch_env, RuntimeLaunchEnv};

fn meta_from_lock(lock: &Lockfile, runtime_id: &str, compose: ComposeMode) -> Result<RuntimeMeta> {
    let mut packages = Vec::new();
    for pkg in &lock.packages {
        let hash = pkg.content_hash.clone().ok_or_else(|| {
            Error::Other(format!(
                "package {} {} missing content_hash for runtime",
                pkg.id, pkg.version
            ))
        })?;
        packages.push(RuntimePackage {
            id: pkg.id.clone(),
            version: pkg.version.clone(),
            content_hash: hash,
            dependencies: pkg.dependencies.clone(),
        });
    }
    packages.sort_by(|a, b| (&a.id, &a.version).cmp(&(&b.id, &b.version)));
    Ok(RuntimeMeta {
        format: RUNTIME_FORMAT,
        runtime_id: runtime_id.to_string(),
        compose,
        root: RuntimeRoot {
            id: lock.root.id.clone(),
            version: lock.root.version.clone(),
        },
        packages,
    })
}

fn compose_files(
    lock: &Lockfile,
    store: &Store,
    dest_files: &Path,
    compose: ComposeMode,
) -> Result<()> {
    let mut claimed: BTreeMap<String, String> = BTreeMap::new();
    let mut packages: Vec<&LockedPackage> = lock.packages.iter().collect();
    packages.sort_by(|a, b| (&a.id, &a.version).cmp(&(&b.id, &b.version)));

    for pkg in packages {
        let stored =
            store
                .get(&pkg.id, &pkg.version)?
                .ok_or_else(|| lar_store::Error::NotFound {
                    id: pkg.id.clone(),
                    version: pkg.version.clone(),
                })?;
        let files_root = stored.path.join("files");
        if !files_root.exists() {
            continue;
        }
        let label = format!("{} {}", pkg.id, pkg.version);
        link_tree(
            &files_root,
            &files_root,
            dest_files,
            &label,
            compose,
            &mut claimed,
        )?;
    }
    Ok(())
}

fn link_tree(
    root: &Path,
    dir: &Path,
    dest_files: &Path,
    package_label: &str,
    compose: ComposeMode,
    claimed: &mut BTreeMap<String, String>,
) -> Result<()> {
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
            link_tree(root, &path, dest_files, package_label, compose, claimed)?;
            continue;
        }
        if !file_type.is_file() {
            return Err(Error::Other(format!(
                "unsupported file type in package payload: {}",
                path.display()
            )));
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|_| Error::Other(format!("file outside files/: {}", path.display())))?;
        let rel_str = normalize_rel(rel)?;
        if let Some(first) = claimed.get(&rel_str) {
            return Err(Error::PathConflict {
                path: rel_str,
                first: first.clone(),
                second: package_label.to_string(),
            });
        }
        let dest = dest_files.join(&rel_str);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        place_file(compose, &path, &dest)?;
        claimed.insert(rel_str, package_label.to_string());
    }
    Ok(())
}

fn place_file(compose: ComposeMode, src: &Path, dest: &Path) -> Result<()> {
    match compose {
        ComposeMode::Symlink => {
            let target = relative_symlink_target(parent_of(dest)?, src)?;
            symlink(&target, dest).map_err(|source| Error::Io {
                path: dest.to_path_buf(),
                source,
            })?;
        }
        ComposeMode::Hardlink => {
            fs::hard_link(src, dest).map_err(|source| Error::Hardlink {
                path: dest.to_path_buf(),
                source,
            })?;
        }
        ComposeMode::Copy => {
            fs::copy(src, dest).map_err(|source| Error::Io {
                path: dest.to_path_buf(),
                source,
            })?;
        }
    }
    Ok(())
}

fn parent_of(path: &Path) -> Result<&Path> {
    path.parent()
        .ok_or_else(|| Error::Other(format!("no parent for {}", path.display())))
}

/// Path of `to` relative to directory `from_dir` (where the symlink will live).
fn relative_symlink_target(from_dir: &Path, to: &Path) -> Result<PathBuf> {
    let from_dir = if from_dir.is_absolute() {
        from_dir.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|source| Error::Io {
                path: PathBuf::from("."),
                source,
            })?
            .join(from_dir)
    };
    let to = if to.is_absolute() {
        to.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|source| Error::Io {
                path: PathBuf::from("."),
                source,
            })?
            .join(to)
    };

    let from_comps: Vec<_> = from_dir.components().collect();
    let to_comps: Vec<_> = to.components().collect();
    let mut i = 0;
    while i < from_comps.len() && i < to_comps.len() && from_comps[i] == to_comps[i] {
        i += 1;
    }
    let mut rel = PathBuf::new();
    for _ in i..from_comps.len() {
        rel.push("..");
    }
    for c in &to_comps[i..] {
        rel.push(c.as_os_str());
    }
    if rel.as_os_str().is_empty() {
        rel.push(".");
    }
    Ok(rel)
}

fn normalize_rel(path: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for c in path.components() {
        match c {
            Component::Normal(s) => {
                let s = s.to_string_lossy();
                if s.is_empty() || s == "." {
                    continue;
                }
                parts.push(s.into_owned());
            }
            Component::CurDir => {}
            _ => {
                return Err(Error::Other(format!(
                    "invalid payload path: {}",
                    path.display()
                )));
            }
        }
    }
    if parts.is_empty() {
        return Err(Error::Other("empty payload path".into()));
    }
    Ok(parts.join("/"))
}
