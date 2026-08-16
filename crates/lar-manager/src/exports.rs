//! PATH command exports: symlinks to `libexec/lar-exec` + metadata for the trampoline.

use std::env;
use std::fmt;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

use lar_package::load_manifest;
use lar_store::Store;
use lar_trampoline::{ExportMeta, EXPORT_FORMAT};

use crate::launch_cmd::ensure_libexec_lar_exec;
use crate::platform;
use crate::record::InstallRecord;
use crate::Error;
use crate::Result;

/// Result of publishing PATH exports.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExportPublish {
    /// True when `[entry]` exports were written.
    pub published: bool,
    /// Non-LAR commands earlier on `PATH` that shadow LAR exports.
    pub shadows: Vec<PathShadow>,
}

/// A host/`PATH` binary that would be chosen instead of a LAR export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathShadow {
    pub command: String,
    /// LAR export path that should win (usually the session bin link).
    pub export: PathBuf,
    /// Earlier runnable path found on `PATH`.
    pub shadowed_by: PathBuf,
}

impl fmt::Display for PathShadow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let export_dir = self
            .export
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| self.export.display().to_string());
        write!(
            f,
            "PATH: `{}` shadows LAR export `{}` at `{}` (put `{export_dir}` earlier on PATH)",
            self.shadowed_by.display(),
            self.command,
            self.export.display()
        )
    }
}

/// Publish (or refresh) PATH exports for an installed app with `[entry]`.
///
/// Writes export metadata and symlinks `{prefix}/bin/{cmd}` → `libexec/lar-exec`
/// (session bin links to the prefix bin entry). `lar-exec` trampolines when invoked
/// under that name and `exec`s the entry ELF.
pub fn publish(store: &Store, record: &InstallRecord) -> Result<ExportPublish> {
    let stored = store
        .get(&record.id, &record.version)?
        .ok_or_else(|| Error::NotInStore {
            id: record.id.clone(),
            version: record.version.clone(),
        })?;
    let manifest = load_manifest(&stored.path.join("package.toml"))?;
    let Some(entry) = &manifest.entry else {
        remove(store, &record.id)?;
        return Ok(ExportPublish::default());
    };

    let runtime_path = store.paths().runtimes.join(&record.runtime_id);
    if !runtime_path.is_dir() {
        return Err(Error::RuntimeMissing {
            id: record.id.clone(),
            runtime_id: record.runtime_id.clone(),
        });
    }

    let mut names: Vec<(String, String)> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for rel in &entry.binaries {
        let cmd = command_name(rel)?;
        if !seen.insert(cmd.clone()) {
            return Err(Error::Other(format!(
                "duplicate entry basename `{cmd}` in package {}",
                record.id
            )));
        }
        names.push((cmd, rel.clone()));
    }

    remove(store, &record.id)?;
    let lar_link = ensure_libexec_lar_exec(store)?;
    let (platform_requires, platform_optional) = {
        let need = platform::need_for_record(store, record)?;
        platform::need_to_export_lists(&need)
    };

    let prefix_bin = store.paths().share_bin();
    let session_bin = store.paths().bin.clone();
    let exports_dir = store.paths().share_exports();
    fs::create_dir_all(&exports_dir).map_err(|source| Error::Io {
        path: exports_dir.clone(),
        source,
    })?;
    fs::create_dir_all(&prefix_bin).map_err(|source| Error::Io {
        path: prefix_bin.clone(),
        source,
    })?;
    fs::create_dir_all(&session_bin).map_err(|source| Error::Io {
        path: session_bin.clone(),
        source,
    })?;

    for (cmd, _) in &names {
        for path in [prefix_bin.join(cmd), session_bin.join(cmd)] {
            if path_exists(&path) && !is_lar_export(store, &path)? {
                return Err(Error::ExportCollision {
                    path: path.display().to_string(),
                });
            }
        }
    }

    for (cmd, binary_rel) in &names {
        let exe = runtime_path.join("files").join(binary_rel);
        if !exe.is_file() {
            return Err(Error::Other(format!(
                "runtime entry `{binary_rel}` missing at {}",
                exe.display()
            )));
        }

        let meta = ExportMeta {
            format: EXPORT_FORMAT,
            app_id: record.id.clone(),
            runtime: runtime_path.clone(),
            binary: exe,
            platform_requires: platform_requires.clone(),
            platform_optional: platform_optional.clone(),
        };
        write_meta(&exports_dir.join(format!("{cmd}.toml")), &meta)?;

        let prefix_link = prefix_bin.join(cmd);
        replace_symlink(&prefix_link, &lar_link)?;
        replace_symlink(&session_bin.join(cmd), &prefix_link)?;
    }

    let mut shadows = Vec::new();
    for (cmd, _) in &names {
        if let Some(shadow) = detect_path_shadow(store, cmd)? {
            shadows.push(shadow);
        }
    }

    Ok(ExportPublish {
        published: true,
        shadows,
    })
}

/// Walk `$PATH` and report the first non-LAR hit that precedes the LAR export.
pub fn detect_path_shadow(store: &Store, cmd: &str) -> Result<Option<PathShadow>> {
    let export = store.paths().bin.join(cmd);
    let path_var = env::var_os("PATH").unwrap_or_default();
    let path_var = path_var.to_string_lossy();

    for dir in path_var.split(':') {
        if dir.is_empty() {
            continue;
        }
        let candidate = Path::new(dir).join(cmd);
        if !looks_runnable(&candidate) {
            continue;
        }
        if is_lar_export(store, &candidate)? {
            // LAR export found first — no shadow.
            return Ok(None);
        }
        return Ok(Some(PathShadow {
            command: cmd.to_string(),
            export,
            shadowed_by: candidate,
        }));
    }
    Ok(None)
}

fn looks_runnable(path: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if meta.file_type().is_dir() {
        return false;
    }
    if meta.file_type().is_symlink() {
        return true;
    }
    meta.permissions().mode() & 0o111 != 0
}

/// Remove PATH exports owned by `app_id`.
pub fn remove(store: &Store, app_id: &str) -> Result<()> {
    let exports_dir = store.paths().share_exports();
    let cmds = list_cmds_for_app(&exports_dir, app_id)?;
    for cmd in cmds {
        let meta_path = exports_dir.join(format!("{cmd}.toml"));
        if meta_path.is_file() {
            fs::remove_file(&meta_path).map_err(|source| Error::Io {
                path: meta_path,
                source,
            })?;
        }
        // Remove session link before prefix link so ownership checks aren't
        // confused by a dangling symlink into an already-removed prefix bin.
        for path in [
            store.paths().bin.join(&cmd),
            store.paths().share_bin().join(&cmd),
        ] {
            if path_exists(&path) {
                fs::remove_file(&path).map_err(|source| Error::Io {
                    path: path.clone(),
                    source,
                })?;
            }
        }
    }
    Ok(())
}

/// Absolute path of the prefix-owned export link for an entry binary relative path.
pub fn prefix_shim_path(store: &Store, binary_rel: &str) -> Result<PathBuf> {
    let cmd = command_name(binary_rel)?;
    Ok(store.paths().share_bin().join(cmd))
}

fn list_cmds_for_app(exports_dir: &Path, app_id: &str) -> Result<Vec<String>> {
    let mut cmds = Vec::new();
    if !exports_dir.is_dir() {
        return Ok(cmds);
    }
    let entries = fs::read_dir(exports_dir).map_err(|source| Error::Io {
        path: exports_dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| Error::Io {
            path: exports_dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let Ok(meta) = toml::from_str::<ExportMeta>(&text) else {
            continue;
        };
        if meta.app_id == app_id {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                cmds.push(stem.to_string());
            }
        }
    }
    Ok(cmds)
}

fn write_meta(path: &Path, meta: &ExportMeta) -> Result<()> {
    let body = toml::to_string_pretty(meta)
        .map_err(|err| Error::Other(format!("serialize export metadata: {err}")))?;
    fs::write(path, body).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn replace_symlink(link: &Path, target: &Path) -> Result<()> {
    if path_exists(link) {
        fs::remove_file(link).map_err(|source| Error::Io {
            path: link.to_path_buf(),
            source,
        })?;
    }
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    symlink(target, link).map_err(|source| Error::Io {
        path: link.to_path_buf(),
        source,
    })
}

fn path_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn is_lar_export(store: &Store, path: &Path) -> Result<bool> {
    let lar_link = store.paths().libexec_lar_exec();
    let Ok(lar_canon) = fs::canonicalize(&lar_link) else {
        return Ok(false);
    };

    let mut current = path.to_path_buf();
    for _ in 0..16 {
        let meta = match fs::symlink_metadata(&current) {
            Ok(m) => m,
            Err(_) => return Ok(false),
        };
        if meta.file_type().is_symlink() {
            let target = fs::read_link(&current).map_err(|source| Error::Io {
                path: current.clone(),
                source,
            })?;
            current = if target.is_absolute() {
                target
            } else {
                current
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(target)
            };
            continue;
        }
        if let Ok(canon) = fs::canonicalize(&current) {
            return Ok(canon == lar_canon);
        }
        return Ok(false);
    }
    Ok(false)
}

fn command_name(rel: &str) -> Result<String> {
    let name = Path::new(rel)
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::Other(format!("invalid entry binary path `{rel}`")))?;
    Ok(name.to_string())
}
