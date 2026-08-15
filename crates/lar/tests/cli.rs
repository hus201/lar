use std::fs;
use std::process::Command;

use tempfile::tempdir;

fn lar() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lar"))
}

#[test]
fn help_lists_core_commands() {
    let output = lar().arg("--help").output().expect("failed to run lar");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for cmd in [
        "package",
        "store",
        "resolve",
        "runtime",
        "run",
        "install",
        "update",
        "rollback",
        "uninstall",
        "repo",
        "config",
    ] {
        assert!(stdout.contains(cmd), "missing {cmd} in --help:\n{stdout}");
    }
}

#[test]
fn unimplemented_commands_are_stubbed() {
    let output = lar()
        .args(["runtime", "build", "org.example.app"])
        .output()
        .expect("failed to run lar");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("lar runtime build: not implemented yet"),
        "{stderr}"
    );
}

#[test]
fn store_add_and_list() {
    let dir = tempdir().unwrap();
    let prefix = dir.path().join("prefix");
    let pkg = dir.path().join("pkg");

    let init = lar()
        .args([
            "package",
            "init",
            "--id",
            "org.example.editor",
            "--name",
            "Example Editor",
        ])
        .arg(&pkg)
        .output()
        .unwrap();
    assert!(init.status.success());
    fs::write(pkg.join("files/hello.txt"), b"hello").unwrap();

    let pack = lar().args(["package", "pack"]).arg(&pkg).output().unwrap();
    assert!(
        pack.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&pack.stderr)
    );
    let lar_path = pkg.join("org.example.editor-0.1.0.lar");

    let add = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["store", "add"])
        .arg(&lar_path)
        .output()
        .unwrap();
    assert!(
        add.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&add.stderr)
    );

    let list = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["store", "list"])
        .output()
        .unwrap();
    assert!(list.status.success());
    let stdout = String::from_utf8_lossy(&list.stdout);
    assert!(stdout.contains("org.example.editor"), "{stdout}");
    assert!(stdout.contains("0.1.0"), "{stdout}");
    assert!(stdout.contains("blake3:"), "{stdout}");

    let remove = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["store", "remove", "org.example.editor", "0.1.0"])
        .output()
        .unwrap();
    assert!(
        remove.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&remove.stderr)
    );

    let list_after = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["store", "list"])
        .output()
        .unwrap();
    assert!(list_after.status.success());
    assert!(String::from_utf8_lossy(&list_after.stdout)
        .trim()
        .is_empty());

    let config = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["config", "--json"])
        .output()
        .unwrap();
    assert!(config.status.success());
    let cfg = String::from_utf8_lossy(&config.stdout);
    assert!(cfg.contains("prefix"), "{cfg}");
    assert!(cfg.contains("store"), "{cfg}");
}

#[test]
fn resolve_writes_lockfile() {
    let dir = tempdir().unwrap();
    let prefix = dir.path().join("prefix");

    let lib = dir.path().join("lib");
    let init_lib = lar()
        .args([
            "package",
            "init",
            "--id",
            "org.example.lib",
            "--name",
            "Lib",
            "--version",
            "1.0.0",
        ])
        .arg(&lib)
        .output()
        .unwrap();
    assert!(init_lib.status.success());
    fs::write(lib.join("files/lib.txt"), b"lib").unwrap();
    let pack_lib = lar().args(["package", "pack"]).arg(&lib).output().unwrap();
    assert!(pack_lib.status.success());
    let add_lib = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["store", "add"])
        .arg(lib.join("org.example.lib-1.0.0.lar"))
        .output()
        .unwrap();
    assert!(
        add_lib.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&add_lib.stderr)
    );

    let app = dir.path().join("app");
    let init_app = lar()
        .args([
            "package",
            "init",
            "--id",
            "org.example.app",
            "--name",
            "App",
        ])
        .arg(&app)
        .output()
        .unwrap();
    assert!(init_app.status.success());
    let mut manifest = fs::read_to_string(app.join("package.toml")).unwrap();
    manifest.push_str("\n[dependencies]\n\"org.example.lib\" = \"1.0.0\"\n");
    fs::write(app.join("package.toml"), manifest).unwrap();

    let resolve = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["resolve"])
        .arg(&app)
        .output()
        .unwrap();
    assert!(
        resolve.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&resolve.stderr)
    );
    let stdout = String::from_utf8_lossy(&resolve.stdout);
    assert!(stdout.contains("org.example.app"), "{stdout}");
    assert!(stdout.contains("lar.lock"), "{stdout}");

    let lock = fs::read_to_string(app.join("lar.lock")).unwrap();
    assert!(lock.contains("org.example.app"), "{lock}");
    assert!(lock.contains("org.example.lib"), "{lock}");
    assert!(lock.contains("blake3:"), "{lock}");
}

#[test]
fn resolve_fails_when_dependency_missing() {
    let dir = tempdir().unwrap();
    let prefix = dir.path().join("prefix");
    let app = dir.path().join("app");

    let init = lar()
        .args([
            "package",
            "init",
            "--id",
            "org.example.app",
            "--name",
            "App",
        ])
        .arg(&app)
        .output()
        .unwrap();
    assert!(init.status.success());
    let mut manifest = fs::read_to_string(app.join("package.toml")).unwrap();
    manifest.push_str("\n[dependencies]\n\"org.example.lib\" = \"1.0.0\"\n");
    fs::write(app.join("package.toml"), manifest).unwrap();

    let resolve = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["resolve"])
        .arg(&app)
        .output()
        .unwrap();
    assert!(!resolve.status.success());
    let stderr = String::from_utf8_lossy(&resolve.stderr);
    assert!(
        stderr.contains("not found in store") || stderr.contains("org.example.lib"),
        "{stderr}"
    );
    assert!(!app.join("lar.lock").exists());
}

#[test]
fn resolve_fails_on_version_conflict() {
    let dir = tempdir().unwrap();
    let prefix = dir.path().join("prefix");

    for (id, version) in [
        ("org.example.lib", "1.0.0"),
        ("org.example.lib", "2.0.0"),
        ("org.example.left", "1.0.0"),
        ("org.example.right", "1.0.0"),
    ] {
        let pkg = dir.path().join(format!("{id}-{version}"));
        let init = lar()
            .args([
                "package",
                "init",
                "--id",
                id,
                "--name",
                id,
                "--version",
                version,
            ])
            .arg(&pkg)
            .output()
            .unwrap();
        assert!(init.status.success());
        fs::write(pkg.join("files/payload.txt"), format!("{id}-{version}")).unwrap();
        if id == "org.example.left" {
            let mut manifest = fs::read_to_string(pkg.join("package.toml")).unwrap();
            manifest.push_str("\n[dependencies]\n\"org.example.lib\" = \"1.0.0\"\n");
            fs::write(pkg.join("package.toml"), manifest).unwrap();
        }
        if id == "org.example.right" {
            let mut manifest = fs::read_to_string(pkg.join("package.toml")).unwrap();
            manifest.push_str("\n[dependencies]\n\"org.example.lib\" = \"2.0.0\"\n");
            fs::write(pkg.join("package.toml"), manifest).unwrap();
        }
        let pack = lar().args(["package", "pack"]).arg(&pkg).output().unwrap();
        assert!(
            pack.status.success(),
            "stderr={}",
            String::from_utf8_lossy(&pack.stderr)
        );
        let add = lar()
            .env("LAR_USER_PREFIX", &prefix)
            .args(["store", "add"])
            .arg(pkg.join(format!("{id}-{version}.lar")))
            .output()
            .unwrap();
        assert!(
            add.status.success(),
            "stderr={}",
            String::from_utf8_lossy(&add.stderr)
        );
    }

    let app = dir.path().join("app");
    let init_app = lar()
        .args([
            "package",
            "init",
            "--id",
            "org.example.app",
            "--name",
            "App",
        ])
        .arg(&app)
        .output()
        .unwrap();
    assert!(init_app.status.success());
    let mut manifest = fs::read_to_string(app.join("package.toml")).unwrap();
    manifest.push_str(
        r#"
[dependencies]
"org.example.left" = "1.0.0"
"org.example.right" = "1.0.0"
"#,
    );
    fs::write(app.join("package.toml"), manifest).unwrap();

    let resolve = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["resolve"])
        .arg(&app)
        .output()
        .unwrap();
    assert!(!resolve.status.success());
    let stderr = String::from_utf8_lossy(&resolve.stderr);
    assert!(
        stderr.contains("conflict") || stderr.contains("org.example.lib"),
        "{stderr}"
    );
    assert!(!app.join("lar.lock").exists());
}

#[test]
fn package_init_creates_manifest() {
    let dir = tempdir().unwrap();
    let output = lar()
        .args([
            "package",
            "init",
            "--id",
            "org.example.editor",
            "--name",
            "Example Editor",
            "--version",
            "0.2.0",
        ])
        .arg(dir.path())
        .output()
        .expect("failed to run lar");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest = fs::read_to_string(dir.path().join("package.toml")).unwrap();
    assert!(manifest.contains("org.example.editor"));
    assert!(manifest.contains("0.2.0"));
    assert!(dir.path().join("files").is_dir());
}

#[test]
fn package_init_requires_id() {
    let dir = tempdir().unwrap();
    let output = lar()
        .args(["package", "init"])
        .arg(dir.path())
        .output()
        .expect("failed to run lar");
    assert!(!output.status.success());
}

#[test]
fn package_init_defaults_name_from_directory() {
    let root = tempdir().unwrap();
    let pkg = root.path().join("my-editor");
    fs::create_dir_all(&pkg).unwrap();

    let output = lar()
        .args(["package", "init", "--id", "com.acme.my-editor"])
        .current_dir(&pkg)
        .arg(".")
        .output()
        .expect("failed to run lar");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest = fs::read_to_string(pkg.join("package.toml")).unwrap();
    assert!(
        manifest.contains("com.acme.my-editor"),
        "manifest:\n{manifest}"
    );
    assert!(manifest.contains("my-editor"), "manifest:\n{manifest}");
}

#[test]
fn package_validate_rejects_bad_id() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("files")).unwrap();
    fs::write(
        dir.path().join("package.toml"),
        r#"
[package]
format = 1
id = "not-reverse-dns"
name = "Bad"
version = "0.1.0"
"#,
    )
    .unwrap();

    let output = lar()
        .args(["package", "validate"])
        .arg(dir.path())
        .output()
        .expect("failed to run lar");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid package id") || stderr.contains("not-reverse-dns"),
        "{stderr}"
    );
}

#[test]
fn package_pack_writes_lar() {
    let dir = tempdir().unwrap();
    let init = lar()
        .args([
            "package",
            "init",
            "--id",
            "org.example.editor",
            "--name",
            "Example Editor",
        ])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(init.status.success());

    let bin = dir.path().join("files/bin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(bin.join("editor"), b"hello").unwrap();

    let mut manifest = fs::read_to_string(dir.path().join("package.toml")).unwrap();
    manifest.push_str(
        r#"

[entry]
default = "bin/editor"
binaries = ["bin/editor"]
"#,
    );
    fs::write(dir.path().join("package.toml"), manifest).unwrap();

    let output = lar()
        .args(["package", "pack"])
        .arg(dir.path())
        .output()
        .expect("failed to run lar");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let lar_path = dir.path().join("org.example.editor-0.1.0.lar");
    assert!(lar_path.is_file(), "missing {}", lar_path.display());

    let inspected = lar()
        .args(["package", "inspect"])
        .arg(&lar_path)
        .output()
        .expect("failed to run lar");
    assert!(
        inspected.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    let stdout = String::from_utf8_lossy(&inspected.stdout);
    assert!(stdout.contains("org.example.editor"), "{stdout}");
    assert!(stdout.contains("blake3:"), "{stdout}");
}
