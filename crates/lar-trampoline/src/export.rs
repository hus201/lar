//! Export metadata load / argv0 resolve.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Error;
use crate::Result;

/// On-disk metadata for a PATH export (`share/lar/exports/{cmd}.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportMeta {
    pub format: u32,
    pub app_id: String,
    pub runtime: PathBuf,
    pub binary: PathBuf,
}

/// Current export metadata format.
pub const EXPORT_FORMAT: u32 = 1;

/// Load export metadata for `cmd` under `prefix`.
pub fn load_export_meta(prefix: &Path, cmd: &str) -> Result<Option<ExportMeta>> {
    let path = prefix
        .join("share")
        .join("lar")
        .join("exports")
        .join(format!("{cmd}.toml"));
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    let meta: ExportMeta = toml::from_str(&text).map_err(|err| {
        Error::Other(format!(
            "invalid export metadata {}: {err}",
            path.display()
        ))
    })?;
    if meta.format != EXPORT_FORMAT {
        return Err(Error::Other(format!(
            "unsupported export format {} at {}",
            meta.format,
            path.display()
        )));
    }
    Ok(Some(meta))
}

/// Resolve export metadata by walking argv0 (and symlink targets) for `…/bin/{cmd}`.
pub fn resolve_export_from_argv0(argv0: &Path) -> Result<Option<(String, ExportMeta)>> {
    let mut current = absolute_argv0(argv0)?;
    for _ in 0..16 {
        if let Some(cmd) = current.file_name().and_then(|s| s.to_str()) {
            if let Some(bin_dir) = current.parent() {
                if bin_dir.file_name().and_then(|s| s.to_str()) == Some("bin") {
                    if let Some(prefix) = bin_dir.parent() {
                        if let Some(meta) = load_export_meta(prefix, cmd)? {
                            return Ok(Some((cmd.to_string(), meta)));
                        }
                    }
                }
            }
        }

        let meta = match fs::symlink_metadata(&current) {
            Ok(m) => m,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => break,
            Err(source) => {
                return Err(Error::Io {
                    path: current,
                    source,
                });
            }
        };
        if !meta.file_type().is_symlink() {
            break;
        }
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
    }
    Ok(None)
}

fn absolute_argv0(argv0: &Path) -> Result<PathBuf> {
    if argv0.is_absolute() {
        return Ok(argv0.to_path_buf());
    }
    let cwd = std::env::current_dir().map_err(|source| Error::Io {
        path: PathBuf::from("."),
        source,
    })?;
    Ok(cwd.join(argv0))
}
