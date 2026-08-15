//! Minimal install-record scanner for store remove referrers.
//!
//! Kept inside `lar-store` (no dependency on `lar-manager`) so remove can
//! refuse packages pinned by `{prefix}/installs/*/install.toml`.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::error::Error;
use crate::paths::Paths;
use crate::Result;

const INSTALL_FORMAT: u32 = 1;

#[derive(Debug, Deserialize)]
struct InstallPinFile {
    #[serde(default)]
    format: u32,
    id: String,
    #[serde(default)]
    packages: Vec<InstallPinPackage>,
}

#[derive(Debug, Deserialize)]
struct InstallPinPackage {
    id: String,
    version: String,
}

/// Application ids whose install records pin `(package_id, version)`.
pub fn install_referrers(paths: &Paths, package_id: &str, version: &str) -> Result<Vec<String>> {
    cleanup_tmp_installs(paths);

    let root = &paths.installs;
    let mut apps = Vec::new();
    if !root.is_dir() {
        return Ok(apps);
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
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let meta_path = dir.join("install.toml");
        if !meta_path.is_file() {
            continue;
        }
        match load_pin_file(&meta_path) {
            Ok(pin) => {
                if pin
                    .packages
                    .iter()
                    .any(|p| p.id == package_id && p.version == version)
                {
                    apps.push(pin.id);
                }
            }
            Err(_) => continue,
        }
    }

    apps.sort();
    apps.dedup();
    Ok(apps)
}

fn load_pin_file(path: &Path) -> Result<InstallPinFile> {
    let text = fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let pin: InstallPinFile = toml::from_str(&text)
        .map_err(|err| Error::Other(format!("invalid install.toml: {err}")))?;
    if pin.format != 0 && pin.format != INSTALL_FORMAT {
        return Err(Error::Other(format!(
            "unsupported install format {} in {}",
            pin.format,
            path.display()
        )));
    }
    if pin.id.is_empty() {
        return Err(Error::Other(format!(
            "install.toml missing id: {}",
            path.display()
        )));
    }
    Ok(pin)
}

fn cleanup_tmp_installs(paths: &Paths) {
    let root = &paths.installs;
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(".tmp-install-") {
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
