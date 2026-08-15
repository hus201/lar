use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use lar_package::load_manifest;
use lar_resolver::{resolve_manifest, verify_lockfile_ready, write_lockfile, Lockfile};
use lar_runtime::{build, ComposeMode};
use lar_store::{Store, StoredPackage};
use semver::Version;

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

/// Result of [`update`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    /// No newer version in apps sources.
    UpToDate(InstallRecord),
    /// Replaced active with a newer apps-source version.
    Updated {
        from: InstallRecord,
        to: InstallRecord,
    },
}

/// Result of [`rollback`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackOutcome {
    /// New active install (was previous).
    pub record: InstallRecord,
    /// Displaced active (now previous).
    pub previous: InstallRecord,
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

    let displaced = if install_dir.join("install.toml").is_file() {
        let prev = load(store, &root.id)?;
        if !force {
            return Err(Error::AlreadyInstalled(root.id.clone()));
        }
        Some(prev)
    } else {
        None
    };
    let replaced = displaced.is_some();
    let older_previous = if replaced {
        load_previous(store, &root.id)?
    } else {
        None
    };

    let record = compose_install(store, &root, compose)?;
    activate_install(store, &record, displaced.as_ref())?;

    if let Some(older) = older_previous {
        let keep_active = older.runtime_id == record.runtime_id;
        let keep_prev = displaced
            .as_ref()
            .is_some_and(|d| d.runtime_id == older.runtime_id);
        if !keep_active && !keep_prev {
            remove_runtime_dir(store, &older.runtime_id);
        }
    }

    Ok(InstallOutcome { record, replaced })
}

/// Update an installed app to the newest newer semver from apps sources.
pub fn update(store: &Store, app_id: &str) -> Result<UpdateOutcome> {
    cleanup_tmp_installs(store);
    let current = load(store, app_id)?;
    let current_ver = parse_semver(&current.version)?;

    let candidates = apps_versions_for(store, app_id)?;
    let mut best: Option<(Version, String)> = None;
    for ver_str in candidates {
        let Ok(ver) = Version::parse(&ver_str) else {
            continue;
        };
        if ver <= current_ver {
            continue;
        }
        match &best {
            None => best = Some((ver, ver_str)),
            Some((prev, _)) if ver > *prev => best = Some((ver, ver_str)),
            _ => {}
        }
    }

    let Some((_, newer)) = best else {
        return Ok(UpdateOutcome::UpToDate(current));
    };

    let older_previous = load_previous(store, app_id)?;
    let source = InstallSource::Store {
        id: app_id.to_string(),
        version: Some(newer),
    };
    let root = ensure_root(store, &source)?;
    let record = compose_install(store, &root, current.compose)?;
    activate_install(store, &record, Some(&current))?;

    if let Some(older) = older_previous {
        let keep_active = older.runtime_id == record.runtime_id;
        let keep_prev = older.runtime_id == current.runtime_id;
        if !keep_active && !keep_prev {
            remove_runtime_dir(store, &older.runtime_id);
        }
    }

    Ok(UpdateOutcome::Updated {
        from: current,
        to: record,
    })
}

/// Swap active install with `previous.toml`.
pub fn rollback(store: &Store, app_id: &str) -> Result<RollbackOutcome> {
    cleanup_tmp_installs(store);
    let active = load(store, app_id)?;
    let previous = load_previous(store, app_id)?.ok_or_else(|| Error::NoPrevious(app_id.into()))?;

    let install_dir = store.paths().installs.join(app_id);
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

    write_record(&tmp.join("install.toml"), &previous)?;
    write_record(&tmp.join("previous.toml"), &active)?;

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

    Ok(RollbackOutcome {
        record: previous,
        previous: active,
    })
}

/// Remove an install record and its composed runtimes. Store packages are kept.
pub fn uninstall(store: &Store, app_id: &str) -> Result<InstallRecord> {
    cleanup_tmp_installs(store);
    let record = load(store, app_id)?;
    let previous = load_previous(store, app_id)?;

    remove_runtime_dir(store, &record.runtime_id);
    if let Some(prev) = &previous {
        if prev.runtime_id != record.runtime_id {
            remove_runtime_dir(store, &prev.runtime_id);
        }
    }

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
    read_record(&path, app_id)
}

/// Load `previous.toml` if present.
pub fn load_previous(store: &Store, app_id: &str) -> Result<Option<InstallRecord>> {
    let path = store.paths().installs.join(app_id).join("previous.toml");
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(read_record(&path, app_id)?))
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

fn compose_install(
    store: &Store,
    root: &StoredPackage,
    compose: ComposeMode,
) -> Result<InstallRecord> {
    let manifest = load_manifest(&root.path.join("package.toml"))?;
    let lock = resolve_manifest(&manifest, store)?;
    verify_lockfile_ready(&lock, store)?;

    fs::create_dir_all(&store.paths().installs).map_err(|source| Error::Io {
        path: store.paths().installs.clone(),
        source,
    })?;

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = store.paths().installs.join(format!(".tmp-build-{nanos}"));
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
    let _ = fs::remove_dir_all(&tmp);

    let record = InstallRecord {
        format: INSTALL_FORMAT,
        id: root.id.clone(),
        version: root.version.clone(),
        content_hash: root.content_hash.clone(),
        runtime_id: built.runtime_id.clone(),
        compose,
        packages: packages_from_lock(&lock)?,
    };
    record.validate().map_err(Error::InvalidRecord)?;
    Ok(record)
}

/// Atomically write active install (+ optional previous stash). Keeps displaced runtime.
fn activate_install(
    store: &Store,
    record: &InstallRecord,
    displaced: Option<&InstallRecord>,
) -> Result<()> {
    let install_dir = store.paths().installs.join(&record.id);
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

    write_record(&tmp.join("install.toml"), record)?;
    if let Some(prev) = displaced {
        write_record(&tmp.join("previous.toml"), prev)?;
    }

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
    Ok(())
}

fn write_record(path: &Path, record: &InstallRecord) -> Result<()> {
    record.validate().map_err(Error::InvalidRecord)?;
    let text = toml::to_string_pretty(record)
        .map_err(|err| Error::Other(format!("serialize install record: {err}")))?;
    fs::write(path, text).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn read_record(path: &Path, app_id: &str) -> Result<InstallRecord> {
    let text = fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
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

fn parse_semver(version: &str) -> Result<Version> {
    Version::parse(version).map_err(|err| {
        Error::Other(format!(
            "install version `{version}` is not valid semver: {err}"
        ))
    })
}

fn apps_versions_for(store: &Store, id: &str) -> Result<Vec<String>> {
    let sources = lar_repo::load_sources(store)?;
    let mut found = Vec::new();
    for src in lar_repo::ordered_apps_sources(&sources) {
        let Ok(base) = lar_repo::parse_uri(&src.uri) else {
            continue;
        };
        let Ok(index) = lar_repo::read_index(&base) else {
            continue;
        };
        for pkg in &index.packages {
            if pkg.id == id {
                found.push(pkg.version.clone());
            }
        }
    }
    found.sort();
    found.dedup();
    Ok(found)
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
        if let Some(stored) = store.get(id, version)? {
            lar_repo::emit_store_hit_warnings(
                store,
                id,
                version,
                Some(&stored.content_hash),
                &mut std::io::stderr(),
            )?;
            return Ok(stored);
        }
        return lar_repo::fetch_into_store(
            store,
            id,
            version,
            lar_repo::LookupMode::Apps,
            &mut std::io::stderr(),
        )
        .map_err(|err| match err {
            lar_repo::Error::PackageNotFound { id, version } => Error::NotInStore { id, version },
            other => other.into(),
        });
    }

    let matches: Vec<_> = store.list()?.into_iter().filter(|p| p.id == id).collect();
    match matches.len() {
        0 => {
            let sources = lar_repo::load_sources(store)?;
            let mut found: Vec<(String, String)> = Vec::new();
            for src in lar_repo::ordered_apps_sources(&sources) {
                let Ok(base) = lar_repo::parse_uri(&src.uri) else {
                    continue;
                };
                let Ok(index) = lar_repo::read_index(&base) else {
                    continue;
                };
                for pkg in &index.packages {
                    if pkg.id == id {
                        found.push((pkg.version.clone(), src.name.clone()));
                    }
                }
            }
            found.sort();
            found.dedup_by(|a, b| a.0 == b.0);
            match found.len() {
                0 => Err(Error::NotInStore {
                    id: id.to_string(),
                    version: "*".into(),
                }),
                1 => lookup_store_root(store, id, Some(&found[0].0)),
                _ => {
                    let versions = found
                        .iter()
                        .map(|(v, _)| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    Err(Error::AmbiguousVersion {
                        id: id.to_string(),
                        versions,
                    })
                }
            }
        }
        1 => {
            let stored = matches.into_iter().next().unwrap();
            lar_repo::emit_store_hit_warnings(
                store,
                &stored.id,
                &stored.version,
                Some(&stored.content_hash),
                &mut std::io::stderr(),
            )?;
            Ok(stored)
        }
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
        if !(name.starts_with(".tmp-install-") || name.starts_with(".tmp-build-")) {
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
