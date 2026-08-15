//! Application lifecycle: install records under `{prefix}/installs/`.

mod error;
mod ops;
mod record;

pub use error::Error;
pub use ops::{install, list, load, uninstall, InstallOutcome, InstallSource};
pub use record::{InstallPackage, InstallRecord, INSTALL_FORMAT};

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use lar_package::{init_package, load_manifest, pack, InitOptions};
    use lar_runtime::ComposeMode;
    use lar_store::{Paths, Store};
    use tempfile::tempdir;

    use super::*;

    fn add_pkg(
        store: &Store,
        dir: &Path,
        id: &str,
        version: &str,
        deps: &[(&str, &str)],
        files: &[(&str, &str)],
        entry: Option<&str>,
    ) -> std::path::PathBuf {
        let pkg = dir.join(format!("{id}-{version}"));
        init_package(
            &pkg,
            &InitOptions {
                id: id.into(),
                name: id.into(),
                version: version.into(),
                force: false,
            },
        )
        .unwrap();
        for (rel, contents) in files {
            let path = pkg.join("files").join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, contents).unwrap();
            if rel.starts_with("bin/") {
                let mut perms = fs::metadata(&path).unwrap().permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&path, perms).unwrap();
            }
        }
        let mut manifest = load_manifest(&pkg.join("package.toml")).unwrap();
        for (dep_id, dep_ver) in deps {
            manifest
                .dependencies
                .insert((*dep_id).into(), (*dep_ver).into());
        }
        if let Some(bin) = entry {
            manifest.entry = Some(lar_package::Entry {
                default: Some(bin.into()),
                binaries: vec![bin.into()],
            });
        }
        fs::write(
            pkg.join("package.toml"),
            toml::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let archive = dir.join(format!("{id}-{version}.lar"));
        pack(&pkg, &archive).unwrap();
        store.add(&archive).unwrap();
        archive
    }

    #[test]
    fn install_from_lar_and_uninstall() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(
            &store,
            dir.path(),
            "org.example.lib",
            "1.0.0",
            &[],
            &[("lib.txt", "lib")],
            None,
        );

        let pkg = dir.path().join("app-src");
        init_package(
            &pkg,
            &InitOptions {
                id: "org.example.app".into(),
                name: "App".into(),
                version: "0.1.0".into(),
                force: false,
            },
        )
        .unwrap();
        let bin = pkg.join("files/bin");
        fs::create_dir_all(&bin).unwrap();
        let script = bin.join("app");
        fs::write(&script, "#!/bin/sh\necho ok\n").unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();
        let mut manifest = load_manifest(&pkg.join("package.toml")).unwrap();
        manifest
            .dependencies
            .insert("org.example.lib".into(), "1.0.0".into());
        manifest.entry = Some(lar_package::Entry {
            default: Some("bin/app".into()),
            binaries: vec!["bin/app".into()],
        });
        fs::write(
            pkg.join("package.toml"),
            toml::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let app_lar = dir.path().join("app.lar");
        pack(&pkg, &app_lar).unwrap();

        let source = InstallSource::Archive(app_lar);
        let outcome = install(&store, &source, ComposeMode::Symlink, false).unwrap();
        assert!(!outcome.replaced);
        let rec = outcome.record;
        assert_eq!(rec.id, "org.example.app");
        assert!(store
            .paths()
            .installs
            .join("org.example.app/install.toml")
            .is_file());
        assert!(store.paths().runtimes.join(&rec.runtime_id).is_dir());
        assert_eq!(list(&store).unwrap().len(), 1);

        let err = store.remove("org.example.lib", "1.0.0", true).unwrap_err();
        assert!(matches!(err, lar_store::Error::InUse { .. }), "{err}");
        let msg = err.to_string();
        assert!(msg.contains("install:org.example.app"), "{msg}");

        let err = install(&store, &source, ComposeMode::Symlink, false).unwrap_err();
        assert!(matches!(err, Error::AlreadyInstalled(_)), "{err}");

        let un = uninstall(&store, "org.example.app").unwrap();
        assert_eq!(un.id, "org.example.app");
        assert!(!store.paths().installs.join("org.example.app").exists());
        assert!(!store.paths().runtimes.join(&rec.runtime_id).exists());
        assert!(store.get("org.example.app", "0.1.0").unwrap().is_some());
        assert!(store.get("org.example.lib", "1.0.0").unwrap().is_some());

        store.remove("org.example.app", "0.1.0", false).unwrap();
        store.remove("org.example.lib", "1.0.0", false).unwrap();
    }

    #[test]
    fn install_from_store_id_and_force_replace() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(
            &store,
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[],
            &[("bin/app", "#!/bin/sh\necho v1\n")],
            Some("bin/app"),
        );

        let source = InstallSource::parse("org.example.app").unwrap();
        let first = install(&store, &source, ComposeMode::Symlink, false).unwrap();
        assert!(!first.replaced);

        let second = install(&store, &source, ComposeMode::Symlink, true).unwrap();
        assert!(second.replaced);
        assert_eq!(first.record.runtime_id, second.record.runtime_id);
        assert_eq!(list(&store).unwrap().len(), 1);

        let third = install(&store, &source, ComposeMode::Copy, true).unwrap();
        assert!(third.replaced);
        assert_ne!(first.record.runtime_id, third.record.runtime_id);
        assert!(!store
            .paths()
            .runtimes
            .join(&first.record.runtime_id)
            .exists());
        assert!(store
            .paths()
            .runtimes
            .join(&third.record.runtime_id)
            .is_dir());
    }

    #[test]
    fn install_archive_rejects_hash_mismatch_on_already_exists() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        let first = add_pkg(
            &store,
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[],
            &[("bin/app", "#!/bin/sh\necho v1\n")],
            Some("bin/app"),
        );

        // Same id/version, different payload → different content_hash.
        let pkg = dir.path().join("app-v2");
        init_package(
            &pkg,
            &InitOptions {
                id: "org.example.app".into(),
                name: "App".into(),
                version: "0.1.0".into(),
                force: false,
            },
        )
        .unwrap();
        let bin = pkg.join("files/bin");
        fs::create_dir_all(&bin).unwrap();
        let script = bin.join("app");
        fs::write(&script, "#!/bin/sh\necho v2\n").unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();
        let mut manifest = load_manifest(&pkg.join("package.toml")).unwrap();
        manifest.entry = Some(lar_package::Entry {
            default: Some("bin/app".into()),
            binaries: vec!["bin/app".into()],
        });
        fs::write(
            pkg.join("package.toml"),
            toml::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let second = dir.path().join("app-different.lar");
        pack(&pkg, &second).unwrap();

        let stored_hash = store
            .get("org.example.app", "0.1.0")
            .unwrap()
            .unwrap()
            .content_hash;
        let archived = lar_package::inspect(&second).unwrap();
        assert_ne!(archived.index.content_hash, stored_hash);

        let err = install(
            &store,
            &InstallSource::Archive(second),
            ComposeMode::Symlink,
            false,
        )
        .unwrap_err();
        assert!(matches!(err, Error::HashMismatch { .. }), "{err}");

        // Identical archive still reuses the store copy.
        let ok = install(
            &store,
            &InstallSource::Archive(first),
            ComposeMode::Symlink,
            false,
        )
        .unwrap();
        assert!(!ok.replaced);
        assert_eq!(ok.record.id, "org.example.app");
    }

    #[test]
    fn parse_install_source() {
        assert!(matches!(
            InstallSource::parse("app.lar").unwrap(),
            InstallSource::Archive(_)
        ));
        assert_eq!(
            InstallSource::parse("org.example.app").unwrap(),
            InstallSource::Store {
                id: "org.example.app".into(),
                version: None
            }
        );
        assert_eq!(
            InstallSource::parse("org.example.app@1.2.3").unwrap(),
            InstallSource::Store {
                id: "org.example.app".into(),
                version: Some("1.2.3".into())
            }
        );
    }
}
