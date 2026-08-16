//! Freedesktop `.desktop` publish/remove for installed apps.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use lar_package::load_manifest;
use lar_store::Store;

use crate::exports;
use crate::launch_cmd::{ensure_libexec_lar, shell_quote};
use crate::record::InstallRecord;
use crate::Error;
use crate::Result;

/// Publish (or refresh) desktop entries for an installed app with `[entry]`.
///
/// `Exec` points at the prefix PATH shim so menus use the same direct-exec path.
pub fn publish(store: &Store, record: &InstallRecord) -> Result<bool> {
    let stored = store
        .get(&record.id, &record.version)?
        .ok_or_else(|| Error::NotInStore {
            id: record.id.clone(),
            version: record.version.clone(),
        })?;
    let manifest = load_manifest(&stored.path.join("package.toml"))?;
    let Some(entry) = &manifest.entry else {
        remove(store, &record.id)?;
        return Ok(false);
    };

    let name = manifest
        .desktop
        .as_ref()
        .and_then(|d| d.name.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or(manifest.package.name.as_str());

    let icon = match manifest
        .desktop
        .as_ref()
        .and_then(|d| d.icon.as_ref())
        .filter(|s| !s.is_empty())
    {
        Some(rel) => {
            let path = stored.path.join("files").join(rel);
            if !path.is_file() {
                return Err(Error::Other(format!(
                    "desktop icon `{rel}` not found at {}",
                    path.display()
                )));
            }
            Some(path)
        }
        None => None,
    };

    let categories = manifest
        .desktop
        .as_ref()
        .and_then(|d| d.categories.clone())
        .unwrap_or_default();

    let default_rel = entry
        .default
        .as_ref()
        .or_else(|| entry.binaries.first())
        .ok_or_else(|| Error::NoEntry(record.id.clone()))?;
    let shim = exports::prefix_shim_path(store, default_rel)?;
    if !shim.is_file() {
        return Err(Error::Other(format!(
            "PATH shim missing at {} (publish exports before desktop)",
            shim.display()
        )));
    }

    ensure_libexec_lar(store)?;
    let shim_str = shim.display().to_string();
    let exec = shell_quote(&shim_str);

    let body = render_desktop(name, &exec, &shim_str, icon.as_deref(), &categories);
    let prefix_path = prefix_desktop_path(store, &record.id);
    let xdg_path = xdg_desktop_path(store, &record.id);

    write_desktop_file(&prefix_path, &body)?;
    write_desktop_file(&xdg_path, &body)?;
    maybe_update_desktop_database(xdg_path.parent());

    Ok(true)
}

/// Remove published desktop entries for `app_id` (best-effort for missing files).
pub fn remove(store: &Store, app_id: &str) -> Result<()> {
    let prefix_path = prefix_desktop_path(store, app_id);
    let xdg_path = xdg_desktop_path(store, app_id);
    for path in [&prefix_path, &xdg_path] {
        if path.is_file() {
            fs::remove_file(path).map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
        }
    }
    maybe_update_desktop_database(xdg_path.parent());
    Ok(())
}

fn prefix_desktop_path(store: &Store, app_id: &str) -> PathBuf {
    store
        .paths()
        .share_applications()
        .join(format!("{app_id}.desktop"))
}

fn xdg_desktop_path(store: &Store, app_id: &str) -> PathBuf {
    store
        .paths()
        .applications
        .join(format!("lar-{app_id}.desktop"))
}

fn render_desktop(
    name: &str,
    exec: &str,
    try_exec: &str,
    icon: Option<&Path>,
    categories: &[String],
) -> String {
    let mut out = String::from("[Desktop Entry]\n");
    out.push_str("Type=Application\n");
    out.push_str("Version=1.5\n");
    out.push_str(&format!("Name={}\n", desktop_escape_value(name)));
    out.push_str(&format!("Exec={exec}\n"));
    out.push_str(&format!("TryExec={}\n", desktop_escape_value(try_exec)));
    if let Some(icon) = icon {
        out.push_str(&format!(
            "Icon={}\n",
            desktop_escape_value(&icon.display().to_string())
        ));
    }
    if !categories.is_empty() {
        let mut cats = categories.join(";");
        if !cats.ends_with(';') {
            cats.push(';');
        }
        out.push_str(&format!("Categories={}\n", desktop_escape_value(&cats)));
    }
    out.push_str("StartupNotify=true\n");
    out
}

fn write_desktop_file(path: &Path, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    fs::write(path, body).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn maybe_update_desktop_database(dir: Option<&Path>) {
    let Some(dir) = dir else {
        return;
    };
    if !dir.is_dir() {
        return;
    }
    let _ = Command::new("update-desktop-database")
        .arg(dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

fn desktop_escape_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
