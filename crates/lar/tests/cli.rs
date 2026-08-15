use std::fs;
use std::process::Command;

use tempfile::tempdir;

fn lar() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lar"))
}

#[test]
fn unimplemented_commands_are_stubbed() {
    let output = lar()
        .args(["update", "org.example.app"])
        .output()
        .expect("failed to run lar");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("lar update: not implemented yet"),
        "{stderr}"
    );
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
        "list",
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
fn runtime_build_and_run() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let prefix = dir.path().join("prefix");

    let lib = dir.path().join("lib");
    assert!(lar()
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
        .unwrap()
        .status
        .success());
    fs::write(lib.join("files/lib.txt"), b"lib").unwrap();
    assert!(lar()
        .args(["package", "pack"])
        .arg(&lib)
        .output()
        .unwrap()
        .status
        .success());
    assert!(lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["store", "add"])
        .arg(lib.join("org.example.lib-1.0.0.lar"))
        .output()
        .unwrap()
        .status
        .success());

    let app = dir.path().join("app");
    assert!(lar()
        .args([
            "package",
            "init",
            "--id",
            "org.example.app",
            "--name",
            "App"
        ])
        .arg(&app)
        .output()
        .unwrap()
        .status
        .success());
    let bin = app.join("files/bin");
    fs::create_dir_all(&bin).unwrap();
    let script = bin.join("app");
    fs::write(&script, "#!/bin/sh\necho hello-from-runtime\n").unwrap();
    let mut perms = fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).unwrap();
    let mut manifest = fs::read_to_string(app.join("package.toml")).unwrap();
    manifest.push_str(
        r#"
[dependencies]
"org.example.lib" = "1.0.0"

[entry]
default = "bin/app"
binaries = ["bin/app"]
"#,
    );
    fs::write(app.join("package.toml"), manifest).unwrap();
    assert!(lar()
        .args(["package", "pack"])
        .arg(&app)
        .output()
        .unwrap()
        .status
        .success());
    assert!(lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["store", "add"])
        .arg(app.join("org.example.app-0.1.0.lar"))
        .output()
        .unwrap()
        .status
        .success());

    // Refresh local package.toml from pack (content_hash) and resolve.
    assert!(lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["resolve"])
        .arg(&app)
        .output()
        .unwrap()
        .status
        .success());

    let build = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["runtime", "build"])
        .arg(&app)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&build.stderr)
    );
    let stdout = String::from_utf8_lossy(&build.stdout);
    assert!(
        stdout.contains("built") || stdout.contains("reused"),
        "{stdout}"
    );
    assert!(stdout.contains("runtimes"), "{stdout}");

    let run = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["run"])
        .arg(&app)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&run.stderr)
    );
    let run_out = String::from_utf8_lossy(&run.stdout);
    assert!(run_out.contains("hello-from-runtime"), "{run_out}");

    let list = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["runtime", "list"])
        .output()
        .unwrap();
    assert!(list.status.success());
    let list_out = String::from_utf8_lossy(&list.stdout);
    assert!(list_out.contains("org.example.app"), "{list_out}");

    let runtime_id = list_out.split_whitespace().next().unwrap();
    let inspected = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["runtime", "inspect", "--json", runtime_id])
        .output()
        .unwrap();
    assert!(
        inspected.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    let inspected_out = String::from_utf8_lossy(&inspected.stdout);
    assert!(inspected_out.contains("runtime_id"), "{inspected_out}");
    assert!(inspected_out.contains("org.example.app"), "{inspected_out}");

    // Default gc keeps healthy runtimes.
    let gc_keep = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["runtime", "gc"])
        .output()
        .unwrap();
    assert!(
        gc_keep.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&gc_keep.stderr)
    );
    let gc_keep_out = String::from_utf8_lossy(&gc_keep.stdout);
    assert!(gc_keep_out.contains("kept 1"), "{gc_keep_out}");
    assert!(gc_keep_out.contains("0 orphan(s)"), "{gc_keep_out}");

    // Force-remove store packages, then default gc removes the broken runtime.
    assert!(lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["store", "remove", "--force", "org.example.app", "0.1.0"])
        .output()
        .unwrap()
        .status
        .success());
    assert!(lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["store", "remove", "--force", "org.example.lib", "1.0.0"])
        .output()
        .unwrap()
        .status
        .success());
    let gc_broken = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["runtime", "gc"])
        .output()
        .unwrap();
    assert!(
        gc_broken.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&gc_broken.stderr)
    );
    let gc_broken_out = String::from_utf8_lossy(&gc_broken.stdout);
    assert!(gc_broken_out.contains("removed"), "{gc_broken_out}");
    assert!(gc_broken_out.contains("kept 0"), "{gc_broken_out}");
    assert!(
        gc_broken_out.contains("1 broken runtime(s), 0 orphan(s)"),
        "{gc_broken_out}"
    );

    let list_after = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["runtime", "list"])
        .output()
        .unwrap();
    assert!(list_after.status.success());
    assert!(String::from_utf8_lossy(&list_after.stdout)
        .trim()
        .is_empty());
}

#[test]
fn runtime_gc_all() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let prefix = dir.path().join("prefix");
    let app = dir.path().join("app");

    assert!(lar()
        .args([
            "package",
            "init",
            "--id",
            "org.example.app",
            "--name",
            "App"
        ])
        .arg(&app)
        .output()
        .unwrap()
        .status
        .success());
    let bin = app.join("files/bin");
    fs::create_dir_all(&bin).unwrap();
    let script = bin.join("app");
    fs::write(&script, "#!/bin/sh\necho ok\n").unwrap();
    let mut perms = fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).unwrap();
    let mut manifest = fs::read_to_string(app.join("package.toml")).unwrap();
    manifest.push_str(
        r#"
[entry]
default = "bin/app"
binaries = ["bin/app"]
"#,
    );
    fs::write(app.join("package.toml"), manifest).unwrap();
    assert!(lar()
        .args(["package", "pack"])
        .arg(&app)
        .output()
        .unwrap()
        .status
        .success());
    assert!(lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["store", "add"])
        .arg(app.join("org.example.app-0.1.0.lar"))
        .output()
        .unwrap()
        .status
        .success());
    assert!(lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["resolve"])
        .arg(&app)
        .output()
        .unwrap()
        .status
        .success());
    assert!(lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["runtime", "build"])
        .arg(&app)
        .output()
        .unwrap()
        .status
        .success());

    let gc = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["runtime", "gc", "--all"])
        .output()
        .unwrap();
    assert!(
        gc.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&gc.stderr)
    );
    let gc_out = String::from_utf8_lossy(&gc.stdout);
    assert!(
        gc_out.contains("gc removed 1 runtime(s), 0 orphan(s)"),
        "{gc_out}"
    );

    let list = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["runtime", "list"])
        .output()
        .unwrap();
    assert!(list.status.success());
    assert!(String::from_utf8_lossy(&list.stdout).trim().is_empty());
}

#[test]
fn install_list_uninstall() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let prefix = dir.path().join("prefix");

    let lib = dir.path().join("lib");
    assert!(lar()
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
        .unwrap()
        .status
        .success());
    fs::write(lib.join("files/lib.txt"), b"lib").unwrap();
    assert!(lar()
        .args(["package", "pack"])
        .arg(&lib)
        .output()
        .unwrap()
        .status
        .success());
    assert!(lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["store", "add"])
        .arg(lib.join("org.example.lib-1.0.0.lar"))
        .output()
        .unwrap()
        .status
        .success());

    let app = dir.path().join("app");
    assert!(lar()
        .args([
            "package",
            "init",
            "--id",
            "org.example.app",
            "--name",
            "App"
        ])
        .arg(&app)
        .output()
        .unwrap()
        .status
        .success());
    let bin = app.join("files/bin");
    fs::create_dir_all(&bin).unwrap();
    let script = bin.join("app");
    fs::write(&script, "#!/bin/sh\necho installed-app\n").unwrap();
    let mut perms = fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).unwrap();
    let mut manifest = fs::read_to_string(app.join("package.toml")).unwrap();
    manifest.push_str(
        r#"
[dependencies]
"org.example.lib" = "1.0.0"

[entry]
default = "bin/app"
binaries = ["bin/app"]
"#,
    );
    fs::write(app.join("package.toml"), manifest).unwrap();
    assert!(lar()
        .args(["package", "pack"])
        .arg(&app)
        .output()
        .unwrap()
        .status
        .success());
    let lar_path = app.join("org.example.app-0.1.0.lar");

    let install = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["install"])
        .arg(&lar_path)
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&install.stderr)
    );
    let install_out = String::from_utf8_lossy(&install.stdout);
    assert!(
        install_out.contains("installed org.example.app"),
        "{install_out}"
    );

    let list = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["list"])
        .output()
        .unwrap();
    assert!(list.status.success());
    let list_out = String::from_utf8_lossy(&list.stdout);
    assert!(list_out.contains("org.example.app"), "{list_out}");
    assert!(list_out.contains("0.1.0"), "{list_out}");

    let blocked = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["store", "remove", "--force", "org.example.lib", "1.0.0"])
        .output()
        .unwrap();
    assert!(!blocked.status.success());
    let blocked_err = String::from_utf8_lossy(&blocked.stderr);
    assert!(
        blocked_err.contains("install:org.example.app"),
        "{blocked_err}"
    );

    let uninstall = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["uninstall", "org.example.app"])
        .output()
        .unwrap();
    assert!(
        uninstall.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&uninstall.stderr)
    );

    let list_after = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["list"])
        .output()
        .unwrap();
    assert!(list_after.status.success());
    assert!(String::from_utf8_lossy(&list_after.stdout)
        .trim()
        .is_empty());

    let remove_lib = lar()
        .env("LAR_USER_PREFIX", &prefix)
        .args(["store", "remove", "--force", "org.example.lib", "1.0.0"])
        .output()
        .unwrap();
    assert!(
        remove_lib.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&remove_lib.stderr)
    );
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
