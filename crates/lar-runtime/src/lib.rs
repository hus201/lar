//! Disposable runtime composition and launch from `lar.lock`.

mod build;
mod catalog;
mod compose;
mod error;
mod meta;

pub use build::{
    build, resolve_lockfile_path, run, run_runtime_entry, runtime_id, runtime_launch_env,
    BuiltRuntime, RuntimeLaunchEnv,
};
pub use catalog::{gc, inspect, list, GcReport, ListedRuntime};
pub use compose::ComposeMode;
pub use error::Error;
pub use meta::{RuntimeMeta, RuntimePackage, RuntimeRoot, RUNTIME_FORMAT};

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use lar_package::{init_package, load_manifest, pack, InitOptions};
    use lar_resolver::{resolve, verify_lockfile_ready, write_lockfile};
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
    ) {
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
    }

    fn resolve_app(store: &Store, dir: &Path) -> std::path::PathBuf {
        resolve_root(store, dir, "org.example.app", "0.1.0")
    }

    fn resolve_root(store: &Store, dir: &Path, id: &str, version: &str) -> std::path::PathBuf {
        // Root is already in store; write a local package.toml matching store for resolve.
        let stored = store.get(id, version).unwrap().unwrap();
        let manifest = load_manifest(&stored.path.join("package.toml")).unwrap();
        let root_dir = dir.join(format!("root-{id}"));
        fs::create_dir_all(&root_dir).unwrap();
        let manifest_path = root_dir.join("package.toml");
        fs::write(&manifest_path, toml::to_string_pretty(&manifest).unwrap()).unwrap();
        let lock = resolve(&manifest_path, store).unwrap();
        let lock_path = root_dir.join("lar.lock");
        write_lockfile(&lock_path, &lock).unwrap();
        lock_path
    }

    #[test]
    fn build_reuses_same_runtime_id() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(
            &store,
            dir.path(),
            "org.example.lib",
            "1.0.0",
            &[],
            &[("lib/helper.txt", "lib")],
            None,
        );
        add_pkg(
            &store,
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[("org.example.lib", "1.0.0")],
            &[("bin/app", "#!/bin/sh\necho ok\n")],
            Some("bin/app"),
        );
        let lock_path = resolve_app(&store, dir.path());
        let first = build(&lock_path, &store, ComposeMode::Symlink).unwrap();
        assert!(!first.reused);
        assert!(first.path.join("files/bin/app").exists());
        assert!(first.path.join("files/lib/helper.txt").exists());
        let link = fs::read_link(first.path.join("files/bin/app")).unwrap();
        assert!(
            !link.is_absolute(),
            "runtime symlinks must be relative, got {}",
            link.display()
        );
        assert!(first
            .path
            .join("files/bin/app")
            .canonicalize()
            .unwrap()
            .is_file());

        let second = build(&lock_path, &store, ComposeMode::Symlink).unwrap();
        assert!(second.reused);
        assert_eq!(first.runtime_id, second.runtime_id);
        assert_eq!(first.path, second.path);

        let listed = list(&store).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].runtime_id, first.runtime_id);
        assert_eq!(listed[0].meta.root.id, "org.example.app");

        let by_id = inspect(&store, Path::new(&first.runtime_id)).unwrap();
        assert_eq!(by_id.path, first.path);
        let by_path = inspect(&store, &first.path).unwrap();
        assert_eq!(by_path.meta.runtime_id, first.runtime_id);
    }

    #[test]
    fn cleans_leftover_tmp_runtime_dirs() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(
            &store,
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[],
            &[("bin/app", "#!/bin/sh\necho ok\n")],
            Some("bin/app"),
        );
        let lock_path = resolve_app(&store, dir.path());

        fs::create_dir_all(store.paths().runtimes.join(".tmp-runtime-stale/files")).unwrap();
        assert!(store.paths().runtimes.join(".tmp-runtime-stale").exists());

        build(&lock_path, &store, ComposeMode::Symlink).unwrap();
        assert!(!store.paths().runtimes.join(".tmp-runtime-stale").exists());

        fs::create_dir_all(store.paths().runtimes.join(".tmp-runtime-stale2")).unwrap();
        list(&store).unwrap();
        assert!(!store.paths().runtimes.join(".tmp-runtime-stale2").exists());
    }

    #[test]
    fn compose_modes_produce_distinct_runtimes() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(
            &store,
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[],
            &[("bin/app", "#!/bin/sh\necho ok\n")],
            Some("bin/app"),
        );
        let lock_path = resolve_app(&store, dir.path());
        let sym = build(&lock_path, &store, ComposeMode::Symlink).unwrap();
        let hard = build(&lock_path, &store, ComposeMode::Hardlink).unwrap();
        let copied = build(&lock_path, &store, ComposeMode::Copy).unwrap();
        assert_ne!(sym.runtime_id, hard.runtime_id);
        assert_ne!(sym.runtime_id, copied.runtime_id);
        assert_eq!(sym.meta.compose, ComposeMode::Symlink);
        assert_eq!(hard.meta.compose, ComposeMode::Hardlink);
        assert_eq!(copied.meta.compose, ComposeMode::Copy);
        assert!(!fs::read_link(sym.path.join("files/bin/app"))
            .unwrap()
            .is_absolute());
        assert!(hard.path.join("files/bin/app").is_file());
        assert!(fs::symlink_metadata(hard.path.join("files/bin/app"))
            .unwrap()
            .file_type()
            .is_file());
        assert!(!fs::symlink_metadata(hard.path.join("files/bin/app"))
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(copied.path.join("files/bin/app").is_file());
    }

    #[test]
    fn gc_removes_broken_keeps_healthy() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(
            &store,
            dir.path(),
            "org.example.keep",
            "0.1.0",
            &[],
            &[("bin/keep", "#!/bin/sh\necho keep\n")],
            Some("bin/keep"),
        );
        add_pkg(
            &store,
            dir.path(),
            "org.example.drop",
            "0.1.0",
            &[],
            &[("bin/drop", "#!/bin/sh\necho drop\n")],
            Some("bin/drop"),
        );
        let keep = build(
            &resolve_root(&store, dir.path(), "org.example.keep", "0.1.0"),
            &store,
            ComposeMode::Symlink,
        )
        .unwrap();
        let dropped = build(
            &resolve_root(&store, dir.path(), "org.example.drop", "0.1.0"),
            &store,
            ComposeMode::Symlink,
        )
        .unwrap();
        assert_eq!(list(&store).unwrap().len(), 2);

        store.remove("org.example.drop", "0.1.0", false).unwrap();

        let report = gc(&store, false).unwrap();
        assert_eq!(report.removed.len(), 1);
        assert_eq!(report.removed[0].runtime_id, dropped.runtime_id);
        assert!(report.orphans.is_empty());
        assert_eq!(report.kept, 1);
        let listed = list(&store).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].runtime_id, keep.runtime_id);
        assert!(keep.path.is_dir());
        assert!(!dropped.path.exists());
    }

    #[test]
    fn gc_removes_hash_mismatch() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(
            &store,
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[],
            &[("bin/app", "#!/bin/sh\necho ok\n")],
            Some("bin/app"),
        );
        let built = build(
            &resolve_app(&store, dir.path()),
            &store,
            ComposeMode::Symlink,
        )
        .unwrap();

        let meta_path = built.path.join("runtime.toml");
        let text = fs::read_to_string(&meta_path).unwrap();
        let mut meta: RuntimeMeta = toml::from_str(&text).unwrap();
        meta.packages[0].content_hash = "blake3:deadbeef".into();
        fs::write(&meta_path, toml::to_string_pretty(&meta).unwrap()).unwrap();

        let report = gc(&store, false).unwrap();
        assert_eq!(report.removed.len(), 1);
        assert_eq!(report.removed[0].runtime_id, built.runtime_id);
        assert_eq!(report.kept, 0);
        assert!(!built.path.exists());
    }

    #[test]
    fn gc_all_removes_healthy_runtimes() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(
            &store,
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[],
            &[("bin/app", "#!/bin/sh\necho ok\n")],
            Some("bin/app"),
        );
        let lock_path = resolve_app(&store, dir.path());
        build(&lock_path, &store, ComposeMode::Symlink).unwrap();
        assert_eq!(list(&store).unwrap().len(), 1);

        let report = gc(&store, true).unwrap();
        assert_eq!(report.removed.len(), 1);
        assert_eq!(report.kept, 0);
        assert!(list(&store).unwrap().is_empty());
    }

    #[test]
    fn gc_removes_orphan_dirs() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        let orphan = store.paths().runtimes.join("not-a-runtime");
        fs::create_dir_all(&orphan).unwrap();
        fs::write(orphan.join("junk.txt"), b"x").unwrap();

        let report = gc(&store, false).unwrap();
        assert!(report.removed.is_empty());
        assert_eq!(report.orphans, vec![orphan.clone()]);
        assert_eq!(report.total_removed(), 1);
        assert!(!orphan.exists());
    }

    #[test]
    fn library_search_paths_include_multiarch_and_usr() {
        let dir = tempdir().unwrap();
        let files = dir.path().join("files");
        fs::create_dir_all(files.join("lib/x86_64-linux-gnu")).unwrap();
        fs::create_dir_all(files.join("usr/lib64")).unwrap();
        fs::create_dir_all(files.join("lib32")).unwrap();
        fs::write(files.join("lib/x86_64-linux-gnu/libfoo.so"), b"x").unwrap();

        let paths = lar_trampoline::library_search_paths(&files);
        let rendered: Vec<_> = paths
            .iter()
            .map(|p| {
                p.strip_prefix(&files)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert!(rendered.iter().any(|p| p == "lib"));
        assert!(rendered.iter().any(|p| p == "lib/x86_64-linux-gnu"));
        assert!(rendered.iter().any(|p| p == "lib32"));
        assert!(rendered.iter().any(|p| p == "usr/lib64"));
    }

    #[test]
    fn path_conflict_errors() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(
            &store,
            dir.path(),
            "org.example.left",
            "1.0.0",
            &[],
            &[("shared.txt", "left")],
            None,
        );
        add_pkg(
            &store,
            dir.path(),
            "org.example.right",
            "1.0.0",
            &[],
            &[("shared.txt", "right")],
            None,
        );
        add_pkg(
            &store,
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[
                ("org.example.left", "1.0.0"),
                ("org.example.right", "1.0.0"),
            ],
            &[("bin/app", "#!/bin/sh\necho app\n")],
            Some("bin/app"),
        );
        let lock_path = resolve_app(&store, dir.path());
        let err = build(&lock_path, &store, ComposeMode::Symlink).unwrap_err();
        assert!(matches!(err, Error::PathConflict { .. }), "{err}");
    }

    #[test]
    fn ready_requires_root_in_store() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(
            &store,
            dir.path(),
            "org.example.lib",
            "1.0.0",
            &[],
            &[("lib.txt", "x")],
            None,
        );
        let root = dir.path().join("root");
        init_package(
            &root,
            &InitOptions {
                id: "org.example.app".into(),
                name: "App".into(),
                version: "0.1.0".into(),
                force: false,
            },
        )
        .unwrap();
        let mut manifest = load_manifest(&root.join("package.toml")).unwrap();
        manifest
            .dependencies
            .insert("org.example.lib".into(), "1.0.0".into());
        fs::write(
            root.join("package.toml"),
            toml::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let lock = resolve(&root.join("package.toml"), &store).unwrap();
        assert!(lock
            .packages
            .iter()
            .find(|p| p.id == "org.example.app")
            .unwrap()
            .content_hash
            .is_none());
        let err = verify_lockfile_ready(&lock, &store).unwrap_err();
        assert!(
            matches!(err, lar_resolver::Error::InvalidLockfile(_)),
            "{err}"
        );
    }

    #[test]
    fn run_executes_entry() {
        let dir = tempdir().unwrap();
        let store = Store::open(Paths::from_prefix(dir.path().join("prefix"), false));
        add_pkg(
            &store,
            dir.path(),
            "org.example.app",
            "0.1.0",
            &[],
            &[("bin/app", "#!/bin/sh\nexit 42\n")],
            Some("bin/app"),
        );
        let lock_path = resolve_app(&store, dir.path());
        let status = run(&lock_path, &store, ComposeMode::Symlink, &[]).unwrap();
        assert_eq!(status.code(), Some(42));
    }
}
