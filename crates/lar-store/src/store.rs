use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use lar_package::{extract, load_manifest};

use crate::error::Error;
use crate::paths::Paths;
use crate::Result;

/// A package recorded in the SxS store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPackage {
    pub id: String,
    pub version: String,
    pub name: String,
    pub content_hash: String,
    pub path: PathBuf,
}

/// Immutable side-by-side package store rooted at a LAR prefix.
#[derive(Debug, Clone)]
pub struct Store {
    paths: Paths,
}

impl Store {
    /// Open a store for an already-resolved prefix.
    pub fn open(paths: Paths) -> Self {
        Self { paths }
    }

    /// Open using [`crate::prefix`].
    pub fn open_default(system: bool) -> Self {
        let prefix = crate::prefix(system);
        Self::open(Paths::from_prefix(prefix, system))
    }

    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    pub fn package_dir(&self, id: &str, version: &str) -> PathBuf {
        self.paths.packages.join(id).join(version)
    }

    /// Look up a stored package by id and version.
    pub fn get(&self, id: &str, version: &str) -> Result<Option<StoredPackage>> {
        let path = self.package_dir(id, version);
        if !path.is_dir() {
            return Ok(None);
        }
        Ok(Some(read_stored_package(&path)?))
    }

    /// List all packages in the store (sorted by id, then version).
    pub fn list(&self) -> Result<Vec<StoredPackage>> {
        self.cleanup_tmp_adds();

        let mut packages = Vec::new();
        if !self.paths.packages.is_dir() {
            return Ok(packages);
        }

        let id_entries = fs::read_dir(&self.paths.packages).map_err(|source| Error::Io {
            path: self.paths.packages.clone(),
            source,
        })?;
        for id_entry in id_entries {
            let id_entry = id_entry.map_err(|source| Error::Io {
                path: self.paths.packages.clone(),
                source,
            })?;
            let id_path = id_entry.path();
            if !id_path.is_dir() {
                continue;
            }
            let name = id_entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') {
                continue;
            }
            let version_entries = fs::read_dir(&id_path).map_err(|source| Error::Io {
                path: id_path.clone(),
                source,
            })?;
            for version_entry in version_entries {
                let version_entry = version_entry.map_err(|source| Error::Io {
                    path: id_path.clone(),
                    source,
                })?;
                let version_path = version_entry.path();
                let name = version_entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with('.') {
                    continue;
                }
                if !version_path.is_dir() {
                    continue;
                }
                if !version_path.join("package.toml").is_file() {
                    continue;
                }
                packages.push(read_stored_package(&version_path)?);
            }
        }

        packages.sort_by(|a, b| (&a.id, &a.version).cmp(&(&b.id, &b.version)));
        Ok(packages)
    }

    /// Verify and add a `.lar` archive to the store.
    pub fn add(&self, archive: &Path) -> Result<StoredPackage> {
        fs::create_dir_all(&self.paths.packages).map_err(|source| Error::Io {
            path: self.paths.packages.clone(),
            source,
        })?;
        self.cleanup_tmp_adds();

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp_path = self.paths.packages.join(format!(".tmp-add-{nanos}"));
        if tmp_path.exists() {
            let _ = fs::remove_dir_all(&tmp_path);
        }

        if let Err(err) = extract(archive, &tmp_path) {
            let _ = fs::remove_dir_all(&tmp_path);
            return Err(err.into());
        }

        let staged = match read_stored_package(&tmp_path) {
            Ok(pkg) => pkg,
            Err(err) => {
                let _ = fs::remove_dir_all(&tmp_path);
                return Err(err);
            }
        };

        let final_path = self.package_dir(&staged.id, &staged.version);
        if final_path.exists() {
            let _ = fs::remove_dir_all(&tmp_path);
            return Err(Error::AlreadyExists {
                id: staged.id,
                version: staged.version,
            });
        }

        let id_dir = self.paths.packages.join(&staged.id);
        if let Err(source) = fs::create_dir_all(&id_dir) {
            let _ = fs::remove_dir_all(&tmp_path);
            return Err(Error::Io {
                path: id_dir,
                source,
            });
        }

        if let Err(source) = fs::rename(&tmp_path, &final_path) {
            let _ = fs::remove_dir_all(&tmp_path);
            return Err(Error::Io {
                path: final_path,
                source,
            });
        }

        read_stored_package(&final_path)
    }

    /// Remove a package version from the store.
    ///
    /// Default (**refuse**): fails if any other stored package pins this exact
    /// `id`/`version` in `[dependencies]`.
    ///
    /// With `force` (**cascade**): recursively removes dependents first, then
    /// this package.
    pub fn remove(&self, id: &str, version: &str, force: bool) -> Result<Vec<StoredPackage>> {
        self.cleanup_tmp_adds();

        if force {
            let mut visiting = std::collections::BTreeSet::new();
            let mut removed = Vec::new();
            self.cascade_remove(id, version, &mut visiting, &mut removed)?;
            if removed.is_empty() {
                return Err(Error::NotFound {
                    id: id.to_string(),
                    version: version.to_string(),
                });
            }
            return Ok(removed);
        }

        let path = self.package_dir(id, version);
        if !path.is_dir() {
            return Err(Error::NotFound {
                id: id.to_string(),
                version: version.to_string(),
            });
        }

        let referrers = self.referrers(id, version)?;
        if !referrers.is_empty() {
            let dependents = referrers
                .iter()
                .map(|pkg| format!("{} {}", pkg.id, pkg.version))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(Error::InUse {
                id: id.to_string(),
                version: version.to_string(),
                dependents,
            });
        }

        Ok(vec![self.remove_one(id, version)?])
    }

    fn cascade_remove(
        &self,
        id: &str,
        version: &str,
        visiting: &mut std::collections::BTreeSet<(String, String)>,
        removed: &mut Vec<StoredPackage>,
    ) -> Result<()> {
        let key = (id.to_string(), version.to_string());
        if visiting.contains(&key) {
            return Ok(());
        }
        if self.get(id, version)?.is_none() {
            return Ok(());
        }
        visiting.insert(key);

        for referrer in self.referrers(id, version)? {
            self.cascade_remove(&referrer.id, &referrer.version, visiting, removed)?;
        }

        if self.get(id, version)?.is_some() {
            removed.push(self.remove_one(id, version)?);
        }
        Ok(())
    }

    fn remove_one(&self, id: &str, version: &str) -> Result<StoredPackage> {
        let path = self.package_dir(id, version);
        if !path.is_dir() {
            return Err(Error::NotFound {
                id: id.to_string(),
                version: version.to_string(),
            });
        }

        let stored = read_stored_package(&path)?;
        fs::remove_dir_all(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;

        let id_dir = self.paths.packages.join(id);
        if id_dir.is_dir() {
            let is_empty = fs::read_dir(&id_dir)
                .map_err(|source| Error::Io {
                    path: id_dir.clone(),
                    source,
                })?
                .next()
                .is_none();
            if is_empty {
                let _ = fs::remove_dir(&id_dir);
            }
        }

        Ok(stored)
    }

    /// Packages in the store that declare an exact dependency on `id`/`version`.
    pub fn referrers(&self, id: &str, version: &str) -> Result<Vec<StoredPackage>> {
        let mut dependents = Vec::new();
        for pkg in self.list()? {
            if pkg.id == id && pkg.version == version {
                continue;
            }
            let manifest = load_manifest(&pkg.path.join("package.toml"))?;
            if manifest.dependencies.get(id).map(String::as_str) == Some(version) {
                dependents.push(pkg);
            }
        }
        Ok(dependents)
    }

    /// Remove leftover `{packages}/.tmp-add-*` dirs from failed or crashed adds.
    fn cleanup_tmp_adds(&self) {
        let Ok(entries) = fs::read_dir(&self.paths.packages) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with(".tmp-add-") {
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
}

fn read_stored_package(path: &Path) -> Result<StoredPackage> {
    let manifest = load_manifest(&path.join("package.toml"))?;
    let content_hash = manifest.package.content_hash.clone().ok_or_else(|| {
        Error::Other(format!(
            "stored package missing content_hash: {}",
            path.display()
        ))
    })?;
    Ok(StoredPackage {
        id: manifest.package.id,
        version: manifest.package.version,
        name: manifest.package.name,
        content_hash,
        path: path.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lar_package::{init_package, load_manifest, pack, InitOptions};
    use tempfile::tempdir;

    fn make_lar(dir: &Path) -> PathBuf {
        let pkg = dir.join("pkg");
        init_package(
            &pkg,
            &InitOptions {
                id: "org.example.editor".into(),
                name: "Example Editor".into(),
                version: "0.1.0".into(),
                force: false,
            },
        )
        .unwrap();
        fs::write(pkg.join("files/hello.txt"), b"hello").unwrap();
        let archive = dir.join("org.example.editor-0.1.0.lar");
        pack(&pkg, &archive).unwrap();
        archive
    }

    #[test]
    fn add_list_and_get() {
        let dir = tempdir().unwrap();
        let prefix = dir.path().join("prefix");
        let store = Store::open(Paths::from_prefix(prefix, false));
        let archive = make_lar(dir.path());

        let stored = store.add(&archive).unwrap();
        assert_eq!(stored.id, "org.example.editor");
        assert_eq!(stored.version, "0.1.0");
        assert!(stored.path.join("files/hello.txt").is_file());
        assert!(!store.paths.packages.read_dir().unwrap().any(|e| {
            let name = e.unwrap().file_name();
            name.to_string_lossy().starts_with(".tmp-add-")
        }));

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, stored.id);

        let got = store.get("org.example.editor", "0.1.0").unwrap().unwrap();
        assert_eq!(got.content_hash, stored.content_hash);
    }

    #[test]
    fn duplicate_add_fails() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        let archive = make_lar(dir.path());
        store.add(&archive).unwrap();
        let err = store.add(&archive).unwrap_err();
        assert!(matches!(err, Error::AlreadyExists { .. }));
    }

    #[test]
    fn remove_package_version() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        let archive = make_lar(dir.path());
        let stored = store.add(&archive).unwrap();
        let path = stored.path.clone();
        let id_dir = path.parent().unwrap().to_path_buf();

        let removed = store.remove("org.example.editor", "0.1.0", false).unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].id, "org.example.editor");
        assert!(!path.exists());
        assert!(!id_dir.exists());
        assert!(store.list().unwrap().is_empty());

        let err = store
            .remove("org.example.editor", "0.1.0", false)
            .unwrap_err();
        assert!(matches!(err, Error::NotFound { .. }));
    }

    #[test]
    fn remove_refuses_when_required_by_other_package() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));

        let lib_pkg = dir.path().join("lib");
        init_package(
            &lib_pkg,
            &InitOptions {
                id: "org.example.lib".into(),
                name: "Example Lib".into(),
                version: "1.0.0".into(),
                force: false,
            },
        )
        .unwrap();
        fs::write(lib_pkg.join("files/lib.txt"), b"lib").unwrap();
        let lib_lar = dir.path().join("lib.lar");
        pack(&lib_pkg, &lib_lar).unwrap();
        store.add(&lib_lar).unwrap();

        let app_pkg = dir.path().join("app");
        init_package(
            &app_pkg,
            &InitOptions {
                id: "org.example.app".into(),
                name: "Example App".into(),
                version: "0.1.0".into(),
                force: false,
            },
        )
        .unwrap();
        fs::write(app_pkg.join("files/app.txt"), b"app").unwrap();
        let mut manifest = load_manifest(&app_pkg.join("package.toml")).unwrap();
        manifest
            .dependencies
            .insert("org.example.lib".into(), "1.0.0".into());
        fs::write(
            app_pkg.join("package.toml"),
            toml::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let app_lar = dir.path().join("app.lar");
        pack(&app_pkg, &app_lar).unwrap();
        store.add(&app_lar).unwrap();

        let err = store.remove("org.example.lib", "1.0.0", false).unwrap_err();
        match err {
            Error::InUse { dependents, .. } => {
                assert!(dependents.contains("org.example.app 0.1.0"), "{dependents}");
            }
            other => panic!("expected InUse, got {other}"),
        }

        let cascaded = store.remove("org.example.lib", "1.0.0", true).unwrap();
        let ids: Vec<_> = cascaded
            .iter()
            .map(|pkg| format!("{} {}", pkg.id, pkg.version))
            .collect();
        assert_eq!(
            ids,
            vec![
                "org.example.app 0.1.0".to_string(),
                "org.example.lib 1.0.0".to_string()
            ]
        );
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn cleans_leftover_tmp_add_dirs() {
        let dir = tempdir().unwrap();
        let prefix = dir.path().join("prefix");
        let store = Store::open(Paths::from_prefix(prefix, false));
        fs::create_dir_all(&store.paths.packages).unwrap();
        let leftover = store.paths.packages.join(".tmp-add-stale");
        fs::create_dir_all(&leftover).unwrap();
        fs::write(leftover.join("junk"), b"x").unwrap();

        store.list().unwrap();
        assert!(!leftover.exists());
    }
}
