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
    let output = lar().args(["resolve"]).output().expect("failed to run lar");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("lar resolve: not implemented yet"),
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
