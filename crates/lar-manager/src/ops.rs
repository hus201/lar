use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use lar_package::load_manifest;
use lar_resolver::{resolve_manifest, verify_lockfile_ready, write_lockfile, Lockfile};
use lar_runtime::{build, ComposeMode};
use lar_store::{Store, StoredPackage};

use crate::record::{InstallPackage, InstallRecord, INSTALL_FORMAT};
use crate::Error;
use crate::Result;

/// Result of a successful [`install`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallOutcome {
    pub record: InstallRecord,
    /// True when an existing install of the same id was replaced (`--force`).
    pub replaced: bool,
}

/// Where to take the application root from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallSource {
    /// Path to a `.lar` archive (added to the store if needed).
    Archive(PathBuf),
    /// Package already in the store (`version` optional → sole match).
    Store { id: String, version: Option<String> },
}

impl InstallSource {
    /// Parse CLI argument: `.lar` path, `id`, or `id@version`.
    pub fn parse(input: &str) -> Result<Self> {
        let path = Path::new(input);
        if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("lar"))
        {
            return Ok(Self::Archive(path.to_path_buf()));
        }

        if let Some((id, version)) = input.rsplit_once('@') {
            if id.is_empty() || version.is_empty() || id.contains('/') || id.contains('\\') {
                return Err(Error::InvalidSource(input.into()));
            }
            return Ok(Self::Store {
                id: id.into(),
                version: Some(version.into()),
            });
        }

        if input.is_empty() || input.contains('/') || input.contains('\\') {
            return Err(Error::InvalidSource(input.into()));
        }

        Ok(Self::Store {
            id: input.into(),
            version: None,
        })
    }
}

/// Install (or replace with `force`) an application.
pub fn install(
    store: &Store,
    source: &InstallSource,
    compose: ComposeMode,
    force: bool,
) -> Result<InstallOutcome> {
    cleanup_tmp_installs(store);

    let root = ensure_root(store, source)?;
    let install_dir = store.paths().installs.join(&root.id);

    let previous = if install_dir.join("install.toml").is_file() {
        let prev = load(store, &root.id)?;
        if !force {
            return Err(Error::AlreadyInstalled(root.id.clone()));
        }
        Some(prev)
    } else {
        None
    };
    let replaced = previous.is_some();

    let manifest = load_manifest(&root.path.join("package.toml"))?;
    let lock = resolve_manifest(&manifest, store)?;
    verify_lockfile_ready(&lock, store)?;

    let content_hash = root.content_hash.clone();
    let packages = packages_from_lock(&lock)?;

    fs::create_dir_all(&store.paths().installs).map_err(|source| Error::Io {
        path: store.paths().installs.clone(),
        source,
    })?;

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = store.paths().installs.join(format!(".tmp-install-{nanos}"));
    if tmp.exists() {
        let _ = fs::remove_dir_all(&tmp);
    }
    fs::create_dir_all(&tmp).map_err(|source| Error::Io {
        path: tmp.clone(),
        source,
    })?;

    let lock_path = tmp.join("lar.lock");
    write_lockfile(&lock_path, &lock)?;
    let built = build(&lock_path, store, compose)?;

    let record = InstallRecord {
        format: INSTALL_FORMAT,
        id: root.id.clone(),
        version: root.version.clone(),
        content_hash,
        runtime_id: built.runtime_id.clone(),
        compose,
        packages,
    };
    record.validate().map_err(Error::InvalidRecord)?;

    let text = toml::to_string_pretty(&record)
        .map_err(|err| Error::Other(format!("serialize install.toml: {err}")))?;
    let meta_path = tmp.join("install.toml");
    fs::write(&meta_path, text).map_err(|source| Error::Io {
        path: meta_path.clone(),
        source,
    })?;
    // Lockfile is only needed for build; keep the install dir lean.
    let _ = fs::remove_file(&lock_path);

    if install_dir.exists() {
        fs::remove_dir_all(&install_dir).map_err(|source| Error::Io {
            path: install_dir.clone(),
            source,
        })?;
    }
    fs::rename(&tmp, &install_dir).map_err(|source| Error::Io {
        path: install_dir.clone(),
        source,
    })?;

    if let Some(prev) = previous {
        if prev.runtime_id != record.runtime_id {
            remove_runtime_dir(store, &prev.runtime_id);
        }
    }

    Ok(InstallOutcome { record, replaced })
}

/// Remove an install record and its composed runtime. Store packages are kept.
pub fn uninstall(store: &Store, app_id: &str) -> Result<InstallRecord> {
    cleanup_tmp_installs(store);
    let record = load(store, app_id)?;
    remove_runtime_dir(store, &record.runtime_id);

    let dir = store.paths().installs.join(app_id);
    fs::remove_dir_all(&dir).map_err(|source| Error::Io {
        path: dir.clone(),
        source,
    })?;
    Ok(record)
}

/// Load one install record by application id.
pub fn load(store: &Store, app_id: &str) -> Result<InstallRecord> {
    let path = store.paths().installs.join(app_id).join("install.toml");
    if !path.is_file() {
        return Err(Error::NotInstalled(app_id.to_string()));
    }
    let text = fs::read_to_string(&path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    let record: InstallRecord =
        toml::from_str(&text).map_err(|err| Error::InvalidRecord(err.to_string()))?;
    record.validate().map_err(Error::InvalidRecord)?;
    if record.id != app_id {
        return Err(Error::InvalidRecord(format!(
            "install id `{}` does not match directory `{app_id}`",
            record.id
        )));
    }
    Ok(record)
}

/// List installed applications (sorted by id).
pub fn list(store: &Store) -> Result<Vec<InstallRecord>> {
    cleanup_tmp_installs(store);

    let root = &store.paths().installs;
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
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        if !dir.join("install.toml").is_file() {
            continue;
        }
        match load(store, &name) {
            Ok(rec) => out.push(rec),
            Err(_) => continue,
        }
    }

    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

fn ensure_root(store: &Store, source: &InstallSource) -> Result<StoredPackage> {
    match source {
        InstallSource::Archive(path) => match store.add(path) {
            Ok(stored) => Ok(stored),
            Err(lar_store::Error::AlreadyExists { id, version }) => {
                let archived = lar_package::inspect(path)?;
                let stored = store.get(&id, &version)?.ok_or_else(|| Error::NotInStore {
                    id: id.clone(),
                    version: version.clone(),
                })?;
                if archived.index.content_hash != stored.content_hash {
                    return Err(Error::HashMismatch {
                        id,
                        version,
                        archive: archived.index.content_hash,
                        store: stored.content_hash,
                    });
                }
                Ok(stored)
            }
            Err(err) => Err(err.into()),
        },
        InstallSource::Store { id, version } => lookup_store_root(store, id, version.as_deref()),
    }
}

fn lookup_store_root(store: &Store, id: &str, version: Option<&str>) -> Result<StoredPackage> {
    if let Some(version) = version {
        return store.get(id, version)?.ok_or_else(|| Error::NotInStore {
            id: id.to_string(),
            version: version.to_string(),
        });
    }

    let matches: Vec<_> = store.list()?.into_iter().filter(|p| p.id == id).collect();
    match matches.len() {
        0 => Err(Error::NotInStore {
            id: id.to_string(),
            version: "*".into(),
        }),
        1 => Ok(matches.into_iter().next().unwrap()),
        _ => {
            let versions = matches
                .iter()
                .map(|p| p.version.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            Err(Error::AmbiguousVersion {
                id: id.to_string(),
                versions,
            })
        }
    }
}

fn packages_from_lock(lock: &Lockfile) -> Result<Vec<InstallPackage>> {
    let mut packages = Vec::with_capacity(lock.packages.len());
    for pkg in &lock.packages {
        let hash = pkg.content_hash.clone().ok_or_else(|| {
            Error::InvalidRecord(format!(
                "package {} {} missing content_hash after resolve",
                pkg.id, pkg.version
            ))
        })?;
        packages.push(InstallPackage {
            id: pkg.id.clone(),
            version: pkg.version.clone(),
            content_hash: hash,
        });
    }
    packages.sort_by(|a, b| (&a.id, &a.version).cmp(&(&b.id, &b.version)));
    Ok(packages)
}

fn remove_runtime_dir(store: &Store, runtime_id: &str) {
    if runtime_id.is_empty() {
        return;
    }
    let path = store.paths().runtimes.join(runtime_id);
    if path.is_dir() {
        let _ = fs::remove_dir_all(&path);
    }
}

fn cleanup_tmp_installs(store: &Store) {
    let root = &store.paths().installs;
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
