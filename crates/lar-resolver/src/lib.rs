//! Dependency resolution and `lar.lock` generation.

mod error;
mod lockfile;
mod resolve;
mod verify;

pub use error::Error;
pub use lockfile::{
    load_lockfile, lockfile_path_for_manifest, parse_lockfile, write_lockfile, LockRoot,
    LockedPackage, Lockfile, LOCKFILE_FORMAT,
};
pub use resolve::{resolve, resolve_manifest};
pub use verify::{verify_lockfile, verify_lockfile_ready};

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use lar_package::{init_package, load_manifest, pack, InitOptions};
    use lar_store::{Paths, Store};
    use tempfile::tempdir;

    use super::*;

    fn add_pkg(store: &Store, dir: &Path, id: &str, version: &str, deps: &[(&str, &str)]) {
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
        fs::write(pkg.join("files/payload.txt"), format!("{id}-{version}")).unwrap();
        if !deps.is_empty() {
            let mut manifest = load_manifest(&pkg.join("package.toml")).unwrap();
            for (dep_id, dep_ver) in deps {
                manifest
                    .dependencies
                    .insert((*dep_id).into(), (*dep_ver).into());
            }
            fs::write(
                pkg.join("package.toml"),
                toml::to_string_pretty(&manifest).unwrap(),
            )
            .unwrap();
        }
        let archive = dir.join(format!("{id}-{version}.lar"));
        pack(&pkg, &archive).unwrap();
        store.add(&archive).unwrap();
    }

    fn write_root(
        dir: &Path,
        id: &str,
        version: &str,
        deps: &[(&str, &str)],
    ) -> std::path::PathBuf {
        let pkg = dir.join("root");
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
        let mut manifest = load_manifest(&pkg.join("package.toml")).unwrap();
        for (dep_id, dep_ver) in deps {
            manifest
                .dependencies
                .insert((*dep_id).into(), (*dep_ver).into());
        }
        let path = pkg.join("package.toml");
        fs::write(&path, toml::to_string_pretty(&manifest).unwrap()).unwrap();
        path
    }

    #[test]
    fn resolve_root_only() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        let manifest = write_root(dir.path(), "org.example.app", "0.1.0", &[]);
        let lock = resolve(&manifest, &store).unwrap();
        assert_eq!(lock.root.id, "org.example.app");
        assert_eq!(lock.packages.len(), 1);
        assert!(lock.packages[0].content_hash.is_none());
    }

    #[test]
    fn resolve_transitive() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(&store, dir.path(), "org.example.base", "2.0.0", &[]);
        add_pkg(
            &store,
            dir.path(),
            "org.example.lib",
            "1.0.0",
            &[("org.example.base", "2.0.0")],
        );
        let manifest = write_root(
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[("org.example.lib", "1.0.0")],
        );

        let lock = resolve(&manifest, &store).unwrap();
        let ids: Vec<_> = lock
            .packages
            .iter()
            .map(|p| format!("{} {}", p.id, p.version))
            .collect();
        assert_eq!(
            ids,
            vec![
                "org.example.app 0.1.0".to_string(),
                "org.example.base 2.0.0".to_string(),
                "org.example.lib 1.0.0".to_string(),
            ]
        );
        let lib = lock
            .packages
            .iter()
            .find(|p| p.id == "org.example.lib")
            .unwrap();
        assert!(lib.content_hash.as_ref().unwrap().starts_with("blake3:"));
        assert_eq!(
            lib.dependencies.get("org.example.base").map(String::as_str),
            Some("2.0.0")
        );

        let out = dir.path().join("root/lar.lock");
        write_lockfile(&out, &lock).unwrap();
        let loaded = load_lockfile(&out).unwrap();
        assert_eq!(loaded, lock);
        verify_lockfile(&lock, &store).unwrap();
    }

    #[test]
    fn verify_rejects_missing_store_package() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(&store, dir.path(), "org.example.lib", "1.0.0", &[]);
        let manifest = write_root(
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[("org.example.lib", "1.0.0")],
        );
        let lock = resolve(&manifest, &store).unwrap();
        store.remove("org.example.lib", "1.0.0", true).unwrap();
        let err = verify_lockfile(&lock, &store).unwrap_err();
        assert!(matches!(err, Error::Missing { .. }), "{err}");
    }

    #[test]
    fn verify_rejects_hash_mismatch() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(&store, dir.path(), "org.example.lib", "1.0.0", &[]);
        let manifest = write_root(
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[("org.example.lib", "1.0.0")],
        );
        let mut lock = resolve(&manifest, &store).unwrap();
        let lib = lock
            .packages
            .iter_mut()
            .find(|p| p.id == "org.example.lib")
            .unwrap();
        lib.content_hash = Some("blake3:deadbeef".into());
        let err = verify_lockfile(&lock, &store).unwrap_err();
        assert!(matches!(err, Error::HashMismatch { .. }), "{err}");
    }

    #[test]
    fn verify_allows_unpackaged_root() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        let manifest = write_root(dir.path(), "org.example.app", "0.1.0", &[]);
        let lock = resolve(&manifest, &store).unwrap();
        assert!(lock.packages[0].content_hash.is_none());
        verify_lockfile(&lock, &store).unwrap();
    }

    #[test]
    fn missing_dependency() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        let manifest = write_root(
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[("org.example.lib", "1.0.0")],
        );
        let err = resolve(&manifest, &store).unwrap_err();
        assert!(
            matches!(
                err,
                Error::Unsatisfiable { .. } | Error::Missing { .. } | Error::Unresolvable(_)
            ),
            "{err}"
        );
    }

    #[test]
    fn version_conflict() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(&store, dir.path(), "org.example.lib", "1.0.0", &[]);
        add_pkg(&store, dir.path(), "org.example.lib", "2.0.0", &[]);
        add_pkg(
            &store,
            dir.path(),
            "org.example.left",
            "1.0.0",
            &[("org.example.lib", "1.0.0")],
        );
        add_pkg(
            &store,
            dir.path(),
            "org.example.right",
            "1.0.0",
            &[("org.example.lib", "2.0.0")],
        );
        let manifest = write_root(
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[
                ("org.example.left", "1.0.0"),
                ("org.example.right", "1.0.0"),
            ],
        );
        let err = resolve(&manifest, &store).unwrap_err();
        assert!(
            matches!(err, Error::Conflict { .. } | Error::Unresolvable(_)),
            "{err}"
        );
    }

    #[test]
    fn dependency_cycle() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(
            &store,
            dir.path(),
            "org.example.a",
            "1.0.0",
            &[("org.example.b", "1.0.0")],
        );
        add_pkg(
            &store,
            dir.path(),
            "org.example.b",
            "1.0.0",
            &[("org.example.a", "1.0.0")],
        );
        let manifest = write_root(
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[("org.example.a", "1.0.0")],
        );
        let err = resolve(&manifest, &store).unwrap_err();
        assert!(matches!(err, Error::Cycle { .. }), "{err}");
    }

    #[test]
    fn resolve_picks_highest_matching_range() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(&store, dir.path(), "org.example.lib", "1.0.0", &[]);
        add_pkg(&store, dir.path(), "org.example.lib", "1.2.0", &[]);
        add_pkg(&store, dir.path(), "org.example.lib", "2.0.0", &[]);
        let manifest = write_root(
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[("org.example.lib", "^1.0")],
        );
        let lock = resolve(&manifest, &store).unwrap();
        let lib = lock
            .packages
            .iter()
            .find(|p| p.id == "org.example.lib")
            .unwrap();
        assert_eq!(lib.version, "1.2.0");
    }

    #[test]
    fn compatible_ranges_share_chosen_version() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(&store, dir.path(), "org.example.lib", "1.5.0", &[]);
        add_pkg(
            &store,
            dir.path(),
            "org.example.left",
            "1.0.0",
            &[("org.example.lib", "^1.0")],
        );
        add_pkg(
            &store,
            dir.path(),
            "org.example.right",
            "1.0.0",
            &[("org.example.lib", "~1.5.0")],
        );
        let manifest = write_root(
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[
                ("org.example.left", "1.0.0"),
                ("org.example.right", "1.0.0"),
            ],
        );
        let lock = resolve(&manifest, &store).unwrap();
        let lib = lock
            .packages
            .iter()
            .find(|p| p.id == "org.example.lib")
            .unwrap();
        assert_eq!(lib.version, "1.5.0");
    }

    #[test]
    fn incompatible_ranges_conflict() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(&store, dir.path(), "org.example.lib", "1.0.0", &[]);
        add_pkg(&store, dir.path(), "org.example.lib", "2.0.0", &[]);
        add_pkg(
            &store,
            dir.path(),
            "org.example.left",
            "1.0.0",
            &[("org.example.lib", "^1.0")],
        );
        add_pkg(
            &store,
            dir.path(),
            "org.example.right",
            "1.0.0",
            &[("org.example.lib", "^2.0")],
        );
        let manifest = write_root(
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[
                ("org.example.left", "1.0.0"),
                ("org.example.right", "1.0.0"),
            ],
        );
        let err = resolve(&manifest, &store).unwrap_err();
        assert!(
            matches!(err, Error::Conflict { .. } | Error::Unresolvable(_)),
            "{err}"
        );
    }

    #[test]
    fn backtracking_retries_older_version() {
        // Highest A (1.1) pulls C^2; B needs C^1. Solver should fall back to A 1.0.
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(&store, dir.path(), "org.example.c", "1.0.0", &[]);
        add_pkg(&store, dir.path(), "org.example.c", "2.0.0", &[]);
        add_pkg(
            &store,
            dir.path(),
            "org.example.a",
            "1.0.0",
            &[("org.example.c", "^1")],
        );
        add_pkg(
            &store,
            dir.path(),
            "org.example.a",
            "1.1.0",
            &[("org.example.c", "^2")],
        );
        add_pkg(
            &store,
            dir.path(),
            "org.example.b",
            "1.0.0",
            &[("org.example.c", "^1")],
        );
        let manifest = write_root(
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[("org.example.a", "^1"), ("org.example.b", "1.0.0")],
        );

        let lock = resolve(&manifest, &store).unwrap();
        let a = lock
            .packages
            .iter()
            .find(|p| p.id == "org.example.a")
            .unwrap();
        let c = lock
            .packages
            .iter()
            .find(|p| p.id == "org.example.c")
            .unwrap();
        assert_eq!(a.version, "1.0.0");
        assert_eq!(c.version, "1.0.0");
    }

    #[test]
    fn backtracking_multilevel() {
        // A2→B2→C^2 conflicts with D→C^1; fall back through A1→B1→C^1.
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(&store, dir.path(), "org.example.c", "1.0.0", &[]);
        add_pkg(&store, dir.path(), "org.example.c", "2.0.0", &[]);
        add_pkg(
            &store,
            dir.path(),
            "org.example.b",
            "1.0.0",
            &[("org.example.c", "^1")],
        );
        add_pkg(
            &store,
            dir.path(),
            "org.example.b",
            "2.0.0",
            &[("org.example.c", "^2")],
        );
        add_pkg(
            &store,
            dir.path(),
            "org.example.a",
            "1.0.0",
            &[("org.example.b", "^1")],
        );
        add_pkg(
            &store,
            dir.path(),
            "org.example.a",
            "1.5.0",
            &[("org.example.b", "^2")],
        );
        add_pkg(
            &store,
            dir.path(),
            "org.example.d",
            "1.0.0",
            &[("org.example.c", "^1")],
        );
        let manifest = write_root(
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[("org.example.a", "^1"), ("org.example.d", "1.0.0")],
        );

        let lock = resolve(&manifest, &store).unwrap();
        let ver = |id: &str| {
            lock.packages
                .iter()
                .find(|p| p.id == id)
                .unwrap()
                .version
                .clone()
        };
        assert_eq!(ver("org.example.a"), "1.0.0");
        assert_eq!(ver("org.example.b"), "1.0.0");
        assert_eq!(ver("org.example.c"), "1.0.0");
    }

    #[test]
    fn unresolvable_lists_tried_candidates() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(&store, dir.path(), "org.example.lib", "1.0.0", &[]);
        add_pkg(&store, dir.path(), "org.example.lib", "2.0.0", &[]);
        add_pkg(
            &store,
            dir.path(),
            "org.example.a",
            "1.0.0",
            &[("org.example.lib", "2.0.0")],
        );
        add_pkg(
            &store,
            dir.path(),
            "org.example.a",
            "1.1.0",
            &[("org.example.lib", "2.0.0")],
        );
        add_pkg(
            &store,
            dir.path(),
            "org.example.b",
            "1.0.0",
            &[("org.example.lib", "1.0.0")],
        );
        let manifest = write_root(
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[("org.example.a", "^1"), ("org.example.b", "1.0.0")],
        );

        let err = resolve(&manifest, &store).unwrap_err();
        let msg = err.to_string();
        assert!(
            matches!(err, Error::Unresolvable(_)),
            "expected Unresolvable, got {err}"
        );
        assert!(
            msg.contains("org.example.lib") || msg.contains("org.example.a"),
            "{msg}"
        );
        assert!(
            msg.contains("incompatible") || msg.contains("forbidden"),
            "{msg}"
        );
    }

    #[test]
    fn backtrack_does_not_pollute_store_with_rejected_remote() {
        use lar_repo::{add_source, build_index, keygen, trust_add, write_index};

        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        let (public, secret, _) = keygen().unwrap();
        trust_add(&store, &public, "").unwrap();

        // Only C 1.0 is preinstalled; A versions and C 2.0 live only in the repo.
        add_pkg(&store, dir.path(), "org.example.c", "1.0.0", &[]);
        add_pkg(
            &store,
            dir.path(),
            "org.example.b",
            "1.0.0",
            &[("org.example.c", "^1")],
        );

        let repo = dir.path().join("repo");
        fs::create_dir_all(repo.join("packages")).unwrap();

        for (id, version, deps) in [
            ("org.example.c", "2.0.0", vec![]),
            ("org.example.a", "1.0.0", vec![("org.example.c", "^1")]),
            ("org.example.a", "1.1.0", vec![("org.example.c", "^2")]),
        ] {
            let pkg = dir.path().join(format!("{id}-{version}"));
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
            fs::write(pkg.join("files/payload.txt"), version).unwrap();
            if !deps.is_empty() {
                let mut manifest = load_manifest(&pkg.join("package.toml")).unwrap();
                for (dep_id, dep_ver) in deps {
                    manifest.dependencies.insert(dep_id.into(), dep_ver.into());
                }
                fs::write(
                    pkg.join("package.toml"),
                    toml::to_string_pretty(&manifest).unwrap(),
                )
                .unwrap();
            }
            let archive = repo.join(format!("packages/{id}-{version}.lar"));
            pack(&pkg, &archive).unwrap();
        }
        let index = build_index(&repo, &secret).unwrap();
        write_index(&repo, &index).unwrap();
        add_source(&store, "main".into(), repo.display().to_string()).unwrap();

        let manifest = write_root(
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[("org.example.a", "^1"), ("org.example.b", "1.0.0")],
        );

        let lock = resolve(&manifest, &store).unwrap();
        let a = lock
            .packages
            .iter()
            .find(|p| p.id == "org.example.a")
            .unwrap();
        assert_eq!(a.version, "1.0.0");

        // Rejected A 1.1 / C 2.0 were peeked but must not remain in the store.
        assert!(store.get("org.example.a", "1.1.0").unwrap().is_none());
        assert!(store.get("org.example.c", "2.0.0").unwrap().is_none());
        assert!(store.get("org.example.a", "1.0.0").unwrap().is_some());
    }

    #[test]
    fn range_fetches_highest_from_deps_source() {
        use lar_repo::{add_source, build_index, keygen, trust_add, write_index};

        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        let (public, secret, _) = keygen().unwrap();
        trust_add(&store, &public, "").unwrap();

        let repo = dir.path().join("repo");
        fs::create_dir_all(repo.join("packages")).unwrap();
        for version in ["1.0.0", "1.4.0"] {
            let pkg = dir.path().join(format!("lib-{version}"));
            init_package(
                &pkg,
                &InitOptions {
                    id: "org.example.lib".into(),
                    name: "Lib".into(),
                    version: version.into(),
                    force: false,
                },
            )
            .unwrap();
            fs::write(pkg.join("files/payload.txt"), version).unwrap();
            let archive = repo.join(format!("packages/org.example.lib-{version}.lar"));
            pack(&pkg, &archive).unwrap();
        }
        let index = build_index(&repo, &secret).unwrap();
        write_index(&repo, &index).unwrap();
        add_source(&store, "main".into(), repo.display().to_string()).unwrap();

        let manifest = write_root(
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[("org.example.lib", "^1")],
        );
        let lock = resolve(&manifest, &store).unwrap();
        let lib = lock
            .packages
            .iter()
            .find(|p| p.id == "org.example.lib")
            .unwrap();
        assert_eq!(lib.version, "1.4.0");
        assert!(store.get("org.example.lib", "1.4.0").unwrap().is_some());
    }
}
