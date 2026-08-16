use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use tempfile::tempdir;

/// Path to a built `lar-exec` for CLI tests (`LAR_EXEC`).
fn lar_exec_path() -> &'static Path {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let lar = PathBuf::from(env!("CARGO_BIN_EXE_lar"));
        let sibling = lar.with_file_name("lar-exec");
        if sibling.is_file() {
            return sibling;
        }

        // `cargo test` may not place `target/*/lar-exec` until an explicit build.
        let cargo = option_env!("CARGO").unwrap_or("cargo");
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace = manifest_dir
            .parent()
            .and_then(|p| p.parent())
            .expect("crates/lar -> workspace root");
        let status = Command::new(cargo)
            .current_dir(workspace)
            .args(["build", "-p", "lar-exec"])
            .status()
            .expect("failed to spawn cargo build -p lar-exec");
        assert!(
            status.success(),
            "cargo build -p lar-exec failed with {status}"
        );

        if sibling.is_file() {
            return sibling;
        }

        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        let mut candidates = vec![workspace.join("target").join(profile).join("lar-exec")];
        if let Ok(td) = std::env::var("CARGO_TARGET_DIR") {
            candidates.push(PathBuf::from(td).join(profile).join("lar-exec"));
        }
        if let Some(parent) = lar.parent() {
            candidates.push(parent.join("lar-exec"));
        }
        for candidate in candidates {
            if candidate.is_file() {
                return candidate;
            }
        }
        panic!(
            "lar-exec not found after build (looked next to {} and under target/{profile}/)",
            lar.display()
        );
    })
}

fn lar() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lar"));
    cmd.env("LAR_EXEC", lar_exec_path());
    cmd
}

/// `lar` with a user prefix and isolated session dirs under the temp root.
fn lar_user(prefix: &std::path::Path) -> Command {
    let root = prefix.parent().unwrap_or(prefix);
    let xdg = prefix.with_file_name("xdg-data");
    let mut cmd = lar();
    cmd.env("LAR_USER_PREFIX", prefix);
    cmd.env("XDG_DATA_HOME", xdg);
    // Session PATH exports resolve to `$HOME/.local/bin`.
    cmd.env("HOME", root);
    cmd
}

#[test]
fn update_requires_installed_app() {
    let dir = tempdir().unwrap();
    let prefix = dir.path().join("prefix");
    let output = lar_user(&prefix)
        .args(["update", "org.example.app"])
        .output()
        .expect("failed to run lar");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not installed"), "{stderr}");
}

#[test]
fn rollback_requires_previous() {
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
            "App",
        ])
        .arg(&app)
        .output()
        .unwrap()
        .status
        .success());
    let bin = app.join("files/bin");
    fs::create_dir_all(&bin).unwrap();
    let script = bin.join("app");
    fs::write(&script, "#!/bin/sh\necho hi\n").unwrap();
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
    assert!(lar_user(&prefix)
        .args(["install"])
        .arg(app.join("org.example.app-0.1.0.lar"))
        .output()
        .unwrap()
        .status
        .success());

    let output = lar_user(&prefix)
        .args(["rollback", "org.example.app"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no previous"), "{stderr}");
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
        "launch",
        "install",
        "list",
        "update",
        "rollback",
        "uninstall",
        "repo",
        "audit",
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

    let add = lar_user(&prefix)
        .args(["store", "add"])
        .arg(&lar_path)
        .output()
        .unwrap();
    assert!(
        add.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&add.stderr)
    );

    let list = lar_user(&prefix).args(["store", "list"]).output().unwrap();
    assert!(list.status.success());
    let stdout = String::from_utf8_lossy(&list.stdout);
    assert!(stdout.contains("org.example.editor"), "{stdout}");
    assert!(stdout.contains("0.1.0"), "{stdout}");
    assert!(stdout.contains("blake3:"), "{stdout}");

    let remove = lar_user(&prefix)
        .args(["store", "remove", "org.example.editor", "0.1.0"])
        .output()
        .unwrap();
    assert!(
        remove.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&remove.stderr)
    );

    let list_after = lar_user(&prefix).args(["store", "list"]).output().unwrap();
    assert!(list_after.status.success());
    assert!(String::from_utf8_lossy(&list_after.stdout)
        .trim()
        .is_empty());

    let config = lar_user(&prefix)
        .args(["config", "--json"])
        .output()
        .unwrap();
    assert!(config.status.success());
    let cfg = String::from_utf8_lossy(&config.stdout);
    assert!(cfg.contains("prefix"), "{cfg}");
    assert!(cfg.contains("store"), "{cfg}");
    assert!(cfg.contains("config"), "{cfg}");
    assert!(cfg.contains("sources"), "{cfg}");
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
    let add_lib = lar_user(&prefix)
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

    let resolve = lar_user(&prefix)
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
    assert!(lar_user(&prefix)
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
    assert!(lar_user(&prefix)
        .args(["store", "add"])
        .arg(app.join("org.example.app-0.1.0.lar"))
        .output()
        .unwrap()
        .status
        .success());

    // Refresh local package.toml from pack (content_hash) and resolve.
    assert!(lar_user(&prefix)
        .args(["resolve"])
        .arg(&app)
        .output()
        .unwrap()
        .status
        .success());

    let build = lar_user(&prefix)
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

    let run = lar_user(&prefix).args(["run"]).arg(&app).output().unwrap();
    assert!(
        run.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&run.stderr)
    );
    let run_out = String::from_utf8_lossy(&run.stdout);
    assert!(run_out.contains("hello-from-runtime"), "{run_out}");

    let list = lar_user(&prefix)
        .args(["runtime", "list"])
        .output()
        .unwrap();
    assert!(list.status.success());
    let list_out = String::from_utf8_lossy(&list.stdout);
    assert!(list_out.contains("org.example.app"), "{list_out}");

    let runtime_id = list_out.split_whitespace().next().unwrap();
    let inspected = lar_user(&prefix)
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
    let gc_keep = lar_user(&prefix).args(["runtime", "gc"]).output().unwrap();
    assert!(
        gc_keep.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&gc_keep.stderr)
    );
    let gc_keep_out = String::from_utf8_lossy(&gc_keep.stdout);
    assert!(gc_keep_out.contains("kept 1"), "{gc_keep_out}");
    assert!(gc_keep_out.contains("0 orphan(s)"), "{gc_keep_out}");

    // Force-remove store packages, then default gc removes the broken runtime.
    assert!(lar_user(&prefix)
        .args(["store", "remove", "--force", "org.example.app", "0.1.0"])
        .output()
        .unwrap()
        .status
        .success());
    assert!(lar_user(&prefix)
        .args(["store", "remove", "--force", "org.example.lib", "1.0.0"])
        .output()
        .unwrap()
        .status
        .success());
    let gc_broken = lar_user(&prefix).args(["runtime", "gc"]).output().unwrap();
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

    let list_after = lar_user(&prefix)
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
    assert!(lar_user(&prefix)
        .args(["store", "add"])
        .arg(app.join("org.example.app-0.1.0.lar"))
        .output()
        .unwrap()
        .status
        .success());
    assert!(lar_user(&prefix)
        .args(["resolve"])
        .arg(&app)
        .output()
        .unwrap()
        .status
        .success());
    assert!(lar_user(&prefix)
        .args(["runtime", "build"])
        .arg(&app)
        .output()
        .unwrap()
        .status
        .success());

    let gc = lar_user(&prefix)
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

    let list = lar_user(&prefix)
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
    assert!(lar_user(&prefix)
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

    let install = lar_user(&prefix)
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

    let list = lar_user(&prefix).args(["list"]).output().unwrap();
    assert!(list.status.success());
    let list_out = String::from_utf8_lossy(&list.stdout);
    assert!(list_out.contains("org.example.app"), "{list_out}");
    assert!(list_out.contains("0.1.0"), "{list_out}");

    let blocked = lar_user(&prefix)
        .args(["store", "remove", "--force", "org.example.lib", "1.0.0"])
        .output()
        .unwrap();
    assert!(!blocked.status.success());
    let blocked_err = String::from_utf8_lossy(&blocked.stderr);
    assert!(
        blocked_err.contains("install:org.example.app"),
        "{blocked_err}"
    );

    let uninstall = lar_user(&prefix)
        .args(["uninstall", "org.example.app"])
        .output()
        .unwrap();
    assert!(
        uninstall.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&uninstall.stderr)
    );

    let list_after = lar_user(&prefix).args(["list"]).output().unwrap();
    assert!(list_after.status.success());
    assert!(String::from_utf8_lossy(&list_after.stdout)
        .trim()
        .is_empty());

    let remove_lib = lar_user(&prefix)
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
fn install_publishes_desktop_and_launch() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let prefix = dir.path().join("prefix");
    let xdg = dir.path().join("xdg-data");

    let app = dir.path().join("app");
    assert!(lar()
        .args([
            "package",
            "init",
            "--id",
            "org.example.desk",
            "--name",
            "Desk",
        ])
        .arg(&app)
        .output()
        .unwrap()
        .status
        .success());
    let bin = app.join("files/bin");
    fs::create_dir_all(&bin).unwrap();
    let script = bin.join("app");
    fs::write(&script, "#!/bin/sh\necho desk-launch\n").unwrap();
    let mut perms = fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).unwrap();
    fs::write(app.join("files/app.png"), b"png").unwrap();
    let mut manifest = fs::read_to_string(app.join("package.toml")).unwrap();
    manifest.push_str(
        r#"
[entry]
default = "bin/app"
binaries = ["bin/app"]

[desktop]
name = "Desk App"
icon = "app.png"
categories = ["Utility"]
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
    let lar_path = app.join("org.example.desk-0.1.0.lar");

    let install = lar_user(&prefix)
        .args(["install"])
        .arg(&lar_path)
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&install.stderr)
    );

    let prefix_desktop = prefix.join("share/applications/org.example.desk.desktop");
    let xdg_desktop = xdg.join("applications/lar-org.example.desk.desktop");
    assert!(prefix_desktop.is_file(), "{}", prefix_desktop.display());
    assert!(xdg_desktop.is_file(), "{}", xdg_desktop.display());
    let body = fs::read_to_string(&prefix_desktop).unwrap();
    assert!(body.contains("Name=Desk App"), "{body}");
    assert!(body.contains("Categories=Utility;"), "{body}");
    assert!(body.contains("/bin/app"), "{body}");
    let libexec = prefix.join("libexec/lar-exec");
    assert!(libexec.exists(), "{}", libexec.display());

    let prefix_shim = prefix.join("bin/app");
    let session_shim = dir.path().join(".local/bin/app");
    assert!(
        prefix_shim
            .symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false),
        "{}",
        prefix_shim.display()
    );
    assert!(
        session_shim
            .symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false),
        "{}",
        session_shim.display()
    );
    let meta = fs::read_to_string(prefix.join("share/lar/exports/app.toml")).unwrap();
    assert!(meta.contains("org.example.desk"), "{meta}");
    assert!(meta.contains("/files/bin/app"), "{meta}");

    let launch = lar_user(&prefix)
        .args(["launch", "org.example.desk"])
        .output()
        .unwrap();
    assert!(
        launch.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&launch.stderr)
    );
    assert!(
        String::from_utf8_lossy(&launch.stdout).contains("desk-launch"),
        "stdout={}",
        String::from_utf8_lossy(&launch.stdout)
    );

    let via_shim = Command::new(&session_shim).output().unwrap();
    assert!(
        via_shim.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&via_shim.stderr)
    );
    assert!(
        String::from_utf8_lossy(&via_shim.stdout).contains("desk-launch"),
        "stdout={}",
        String::from_utf8_lossy(&via_shim.stdout)
    );

    let uninstall = lar_user(&prefix)
        .args(["uninstall", "org.example.desk"])
        .output()
        .unwrap();
    assert!(uninstall.status.success());
    assert!(!prefix_desktop.exists());
    assert!(!xdg_desktop.exists());
    assert!(!prefix_shim.exists());
    assert!(!session_shim.exists());
    assert!(!prefix.join("share/lar/exports/app.toml").exists());
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

    let resolve = lar_user(&prefix)
        .args(["resolve"])
        .arg(&app)
        .output()
        .unwrap();
    assert!(!resolve.status.success());
    let stderr = String::from_utf8_lossy(&resolve.stderr);
    assert!(
        stderr.contains("not found in store")
            || stderr.contains("matches requirement")
            || stderr.contains("org.example.lib"),
        "{stderr}"
    );
    assert!(!app.join("lar.lock").exists());
}

#[test]
fn resolve_version_range_picks_highest() {
    let dir = tempdir().unwrap();
    let prefix = dir.path().join("prefix");

    for version in ["1.0.0", "1.3.0", "2.0.0"] {
        let lib = dir.path().join(format!("lib-{version}"));
        assert!(lar()
            .args([
                "package",
                "init",
                "--id",
                "org.example.lib",
                "--name",
                "Lib",
                "--version",
                version,
            ])
            .arg(&lib)
            .output()
            .unwrap()
            .status
            .success());
        fs::write(lib.join("files/lib.txt"), version.as_bytes()).unwrap();
        assert!(lar()
            .args(["package", "pack"])
            .arg(&lib)
            .output()
            .unwrap()
            .status
            .success());
        assert!(lar_user(&prefix)
            .args(["store", "add"])
            .arg(lib.join(format!("org.example.lib-{version}.lar")))
            .output()
            .unwrap()
            .status
            .success());
    }

    let app = dir.path().join("app");
    assert!(lar()
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
        .unwrap()
        .status
        .success());
    let mut manifest = fs::read_to_string(app.join("package.toml")).unwrap();
    manifest.push_str("\n[dependencies]\n\"org.example.lib\" = \"^1.0\"\n");
    fs::write(app.join("package.toml"), manifest).unwrap();

    let resolve = lar_user(&prefix)
        .args(["resolve"])
        .arg(&app)
        .output()
        .unwrap();
    assert!(
        resolve.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&resolve.stderr)
    );
    let lock = fs::read_to_string(app.join("lar.lock")).unwrap();
    assert!(
        lock.contains("id = \"org.example.lib\"") && lock.contains("version = \"1.3.0\""),
        "{lock}"
    );
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
        let add = lar_user(&prefix)
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

    let resolve = lar_user(&prefix)
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

#[test]
fn repo_fetch_resolve_advisory_and_audit() {
    let dir = tempdir().unwrap();
    let prefix = dir.path().join("prefix");
    let keys = dir.path().join("keys");
    let repo = dir.path().join("repo");
    fs::create_dir_all(repo.join("packages")).unwrap();

    let keygen = lar()
        .args(["package", "keygen", "--out"])
        .arg(&keys)
        .output()
        .unwrap();
    assert!(
        keygen.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&keygen.stderr)
    );

    let trust = lar_user(&prefix)
        .args(["repo", "trust", "add"])
        .arg(keys.join("ed25519.pub"))
        .args(["--comment", "test"])
        .output()
        .unwrap();
    assert!(
        trust.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&trust.stderr)
    );

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
    fs::copy(
        lib.join("org.example.lib-1.0.0.lar"),
        repo.join("packages/org.example.lib-1.0.0.lar"),
    )
    .unwrap();

    let index = lar()
        .args(["repo", "index"])
        .arg(&repo)
        .args(["--sign-key"])
        .arg(keys.join("ed25519.sec"))
        .output()
        .unwrap();
    assert!(
        index.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&index.stderr)
    );

    fs::write(
        repo.join("advisories.toml"),
        r#"
format = 1

[[advisories]]
id = "LAR-2026-0099"
package_id = "org.example.lib"
versions = ["1.0.0"]
severity = "medium"
yanked = false
summary = "Test advisory"
url = "https://example.test/LAR-2026-0099"
"#,
    )
    .unwrap();
    let sign_adv = lar()
        .args(["repo", "index"])
        .arg(&repo)
        .args(["--sign-key"])
        .arg(keys.join("ed25519.sec"))
        .output()
        .unwrap();
    assert!(
        sign_adv.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&sign_adv.stderr)
    );
    let sign_out = String::from_utf8_lossy(&sign_adv.stdout);
    assert!(
        sign_out.contains("advisories.toml") && sign_out.contains("signed"),
        "expected signed advisories, got {sign_out}"
    );

    let add = lar_user(&prefix)
        .args(["repo", "add", "--main"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(
        add.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&add.stderr)
    );
    let add_out = String::from_utf8_lossy(&add.stdout);
    assert!(
        add_out.contains("(deps)") && add_out.contains("main"),
        "expected deps main default, got {add_out}"
    );

    let app = dir.path().join("app");
    assert!(lar()
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
        .unwrap()
        .status
        .success());
    let mut manifest = fs::read_to_string(app.join("package.toml")).unwrap();
    manifest.push_str("\n[dependencies]\n\"org.example.lib\" = \"1.0.0\"\n");
    fs::write(app.join("package.toml"), manifest).unwrap();

    let resolve = lar_user(&prefix)
        .args(["resolve"])
        .arg(&app)
        .output()
        .unwrap();
    assert!(
        resolve.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&resolve.stderr)
    );
    let stderr = String::from_utf8_lossy(&resolve.stderr);
    assert!(
        stderr.contains("LAR-2026-0099") || stderr.contains("Test advisory"),
        "expected advisory warning, stderr={stderr}"
    );

    let list = lar_user(&prefix).args(["store", "list"]).output().unwrap();
    assert!(list.status.success());
    assert!(
        String::from_utf8_lossy(&list.stdout).contains("org.example.lib"),
        "{}",
        String::from_utf8_lossy(&list.stdout)
    );

    // Yank for audit of store packages
    fs::write(
        repo.join("advisories.toml"),
        r#"
format = 1

[[advisories]]
id = "LAR-2026-0100"
package_id = "org.example.lib"
versions = ["1.0.0"]
severity = "high"
yanked = true
summary = "Yanked after ship"
"#,
    )
    .unwrap();
    let resign = lar()
        .args(["repo", "index"])
        .arg(&repo)
        .args(["--sign-key"])
        .arg(keys.join("ed25519.sec"))
        .output()
        .unwrap();
    assert!(
        resign.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&resign.stderr)
    );

    let audit = lar_user(&prefix)
        .args(["audit", "--store"])
        .output()
        .unwrap();
    assert!(!audit.status.success(), "audit should fail on high/yanked");
    let audit_out = format!(
        "{}{}",
        String::from_utf8_lossy(&audit.stdout),
        String::from_utf8_lossy(&audit.stderr)
    );
    assert!(audit_out.contains("LAR-2026-0100"), "{audit_out}");

    // New fetch of yanked pin must refuse
    let prefix2 = dir.path().join("prefix2");
    assert!(lar_user(&prefix2)
        .args(["repo", "trust", "add"])
        .arg(keys.join("ed25519.pub"))
        .output()
        .unwrap()
        .status
        .success());
    assert!(lar_user(&prefix2)
        .args(["repo", "add", "--main"])
        .arg(&repo)
        .output()
        .unwrap()
        .status
        .success());
    let app2 = dir.path().join("app2");
    assert!(lar()
        .args([
            "package",
            "init",
            "--id",
            "org.example.app2",
            "--name",
            "App2",
        ])
        .arg(&app2)
        .output()
        .unwrap()
        .status
        .success());
    let mut m2 = fs::read_to_string(app2.join("package.toml")).unwrap();
    m2.push_str("\n[dependencies]\n\"org.example.lib\" = \"1.0.0\"\n");
    fs::write(app2.join("package.toml"), m2).unwrap();
    let resolve_yanked = lar_user(&prefix2)
        .args(["resolve"])
        .arg(&app2)
        .output()
        .unwrap();
    assert!(!resolve_yanked.status.success());
    let err = String::from_utf8_lossy(&resolve_yanked.stderr);
    assert!(
        err.contains("yanked") || err.contains("matches requirement"),
        "{err}"
    );
}

#[test]
fn install_from_apps_source() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let prefix = dir.path().join("prefix");
    let keys = dir.path().join("keys");
    let repo = dir.path().join("repo");
    fs::create_dir_all(repo.join("packages")).unwrap();

    assert!(lar()
        .args(["package", "keygen", "--out"])
        .arg(&keys)
        .output()
        .unwrap()
        .status
        .success());
    assert!(lar_user(&prefix)
        .args(["repo", "trust", "add"])
        .arg(keys.join("ed25519.pub"))
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
            "org.example.vendorapp",
            "--name",
            "Vendor App",
        ])
        .arg(&app)
        .output()
        .unwrap()
        .status
        .success());
    let bin = app.join("files/bin");
    fs::create_dir_all(&bin).unwrap();
    let script = bin.join("app");
    fs::write(&script, "#!/bin/sh\necho from-apps-source\n").unwrap();
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
    fs::copy(
        app.join("org.example.vendorapp-0.1.0.lar"),
        repo.join("packages/org.example.vendorapp-0.1.0.lar"),
    )
    .unwrap();

    assert!(lar()
        .args(["repo", "index"])
        .arg(&repo)
        .args(["--sign-key"])
        .arg(keys.join("ed25519.sec"))
        .output()
        .unwrap()
        .status
        .success());

    // deps-only main must not satisfy install-by-id
    let deps_only = dir.path().join("deps-repo");
    fs::create_dir_all(deps_only.join("packages")).unwrap();
    fs::copy(
        app.join("org.example.vendorapp-0.1.0.lar"),
        deps_only.join("packages/org.example.vendorapp-0.1.0.lar"),
    )
    .unwrap();
    assert!(lar()
        .args(["repo", "index"])
        .arg(&deps_only)
        .args(["--sign-key"])
        .arg(keys.join("ed25519.sec"))
        .output()
        .unwrap()
        .status
        .success());
    assert!(lar_user(&prefix)
        .args(["repo", "add", "--main"])
        .arg(&deps_only)
        .output()
        .unwrap()
        .status
        .success());

    let miss = lar_user(&prefix)
        .args(["install", "org.example.vendorapp"])
        .output()
        .unwrap();
    assert!(
        !miss.status.success(),
        "deps-only main should not install apps"
    );

    let add_apps = lar_user(&prefix)
        .args(["repo", "add", "--policy", "apps", "--name", "vendor"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(
        add_apps.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&add_apps.stderr)
    );

    let install = lar_user(&prefix)
        .args(["install", "org.example.vendorapp"])
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&install.stderr)
    );
    let stdout = String::from_utf8_lossy(&install.stdout);
    assert!(
        stdout.contains("installed") && stdout.contains("org.example.vendorapp"),
        "{stdout}"
    );

    let list = lar_user(&prefix).args(["list"]).output().unwrap();
    assert!(list.status.success());
    assert!(
        String::from_utf8_lossy(&list.stdout).contains("org.example.vendorapp"),
        "{}",
        String::from_utf8_lossy(&list.stdout)
    );

    let store_list = lar_user(&prefix).args(["store", "list"]).output().unwrap();
    assert!(
        String::from_utf8_lossy(&store_list.stdout).contains("org.example.vendorapp"),
        "app should be fetched into the store"
    );
}

#[test]
fn update_and_rollback_from_apps_source() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let prefix = dir.path().join("prefix");
    let keys = dir.path().join("keys");
    let repo = dir.path().join("repo");
    fs::create_dir_all(repo.join("packages")).unwrap();

    assert!(lar()
        .args(["package", "keygen", "--out"])
        .arg(&keys)
        .output()
        .unwrap()
        .status
        .success());
    assert!(lar_user(&prefix)
        .args(["repo", "trust", "add"])
        .arg(keys.join("ed25519.pub"))
        .output()
        .unwrap()
        .status
        .success());

    for (version, body) in [("0.1.0", "v1"), ("0.2.0", "v2")] {
        let app = dir.path().join(format!("app-{version}"));
        assert!(lar()
            .args([
                "package",
                "init",
                "--id",
                "org.example.upapp",
                "--name",
                "Up App",
                "--version",
                version,
            ])
            .arg(&app)
            .output()
            .unwrap()
            .status
            .success());
        let bin = app.join("files/bin");
        fs::create_dir_all(&bin).unwrap();
        let script = bin.join("app");
        fs::write(&script, format!("#!/bin/sh\necho {body}\n")).unwrap();
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
        fs::copy(
            app.join(format!("org.example.upapp-{version}.lar")),
            repo.join(format!("packages/org.example.upapp-{version}.lar")),
        )
        .unwrap();
    }

    assert!(lar()
        .args(["repo", "index"])
        .arg(&repo)
        .args(["--sign-key"])
        .arg(keys.join("ed25519.sec"))
        .output()
        .unwrap()
        .status
        .success());
    assert!(lar_user(&prefix)
        .args(["repo", "add", "--policy", "apps", "--name", "vendor"])
        .arg(&repo)
        .output()
        .unwrap()
        .status
        .success());

    let install = lar_user(&prefix)
        .args(["install", "org.example.upapp@0.1.0"])
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&install.stderr)
    );

    let update = lar_user(&prefix)
        .args(["update", "org.example.upapp"])
        .output()
        .unwrap();
    assert!(
        update.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&update.stderr)
    );
    let out = String::from_utf8_lossy(&update.stdout);
    assert!(
        out.contains("updated") && out.contains("0.1.0") && out.contains("0.2.0"),
        "{out}"
    );

    let up_to_date = lar_user(&prefix)
        .args(["update", "org.example.upapp"])
        .output()
        .unwrap();
    assert!(up_to_date.status.success());
    assert!(
        String::from_utf8_lossy(&up_to_date.stdout).contains("up to date"),
        "{}",
        String::from_utf8_lossy(&up_to_date.stdout)
    );

    let list = lar_user(&prefix).args(["list"]).output().unwrap();
    assert!(String::from_utf8_lossy(&list.stdout).contains("0.2.0"));

    // previous.toml pins still block store remove
    let remove_old = lar_user(&prefix)
        .args(["store", "remove", "org.example.upapp", "0.1.0"])
        .output()
        .unwrap();
    assert!(!remove_old.status.success());
    assert!(
        String::from_utf8_lossy(&remove_old.stderr).contains("install:org.example.upapp"),
        "{}",
        String::from_utf8_lossy(&remove_old.stderr)
    );

    let rollback = lar_user(&prefix)
        .args(["rollback", "org.example.upapp"])
        .output()
        .unwrap();
    assert!(
        rollback.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&rollback.stderr)
    );
    let rb = String::from_utf8_lossy(&rollback.stdout);
    assert!(rb.contains("rolled back") && rb.contains("0.1.0"), "{rb}");

    let list2 = lar_user(&prefix).args(["list"]).output().unwrap();
    assert!(
        String::from_utf8_lossy(&list2.stdout).contains("0.1.0"),
        "{}",
        String::from_utf8_lossy(&list2.stdout)
    );
}

#[test]
fn repo_init_publish_validate_unpublish() {
    let dir = tempdir().unwrap();
    let keys = dir.path().join("keys");
    let repo = dir.path().join("repo");

    let keygen = lar()
        .args(["package", "keygen", "--out"])
        .arg(&keys)
        .output()
        .unwrap();
    assert!(
        keygen.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&keygen.stderr)
    );

    let init = lar()
        .args(["repo", "init"])
        .arg(&repo)
        .args(["--sign-key"])
        .arg(keys.join("ed25519.sec"))
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&init.stderr)
    );
    assert!(repo.join("packages").is_dir());
    assert!(repo.join("index.toml").is_file());

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

    let publish = lar()
        .args(["repo", "publish"])
        .arg(&repo)
        .arg(lib.join("org.example.lib-1.0.0.lar"))
        .args(["--sign-key"])
        .arg(keys.join("ed25519.sec"))
        .output()
        .unwrap();
    assert!(
        publish.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&publish.stderr)
    );
    let pub_out = String::from_utf8_lossy(&publish.stdout);
    assert!(
        pub_out.contains("published org.example.lib 1.0.0"),
        "{pub_out}"
    );

    let validate = lar()
        .args(["repo", "validate"])
        .arg(&repo)
        .args(["--pubkey"])
        .arg(keys.join("ed25519.pub"))
        .output()
        .unwrap();
    assert!(
        validate.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&validate.stderr)
    );
    let val_out = String::from_utf8_lossy(&validate.stdout);
    assert!(val_out.contains("signatures verified"), "{val_out}");

    // Corrupt the archive path listed in the index.
    fs::remove_file(repo.join("packages/org.example.lib-1.0.0.lar")).unwrap();
    let bad = lar()
        .args(["repo", "validate"])
        .arg(&repo)
        .args(["--pubkey"])
        .arg(keys.join("ed25519.pub"))
        .output()
        .unwrap();
    assert!(!bad.status.success());
    assert!(
        String::from_utf8_lossy(&bad.stderr).contains("missing"),
        "{}",
        String::from_utf8_lossy(&bad.stderr)
    );

    // Restore via publish, then unpublish.
    assert!(lar()
        .args(["repo", "publish"])
        .arg(&repo)
        .arg(lib.join("org.example.lib-1.0.0.lar"))
        .args(["--sign-key"])
        .arg(keys.join("ed25519.sec"))
        .output()
        .unwrap()
        .status
        .success());

    let unpublish = lar()
        .args(["repo", "unpublish"])
        .arg(&repo)
        .args(["org.example.lib", "1.0.0"])
        .args(["--sign-key"])
        .arg(keys.join("ed25519.sec"))
        .output()
        .unwrap();
    assert!(
        unpublish.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&unpublish.stderr)
    );
    let un_out = String::from_utf8_lossy(&unpublish.stdout);
    assert!(
        un_out.contains("unpublished org.example.lib 1.0.0"),
        "{un_out}"
    );
    assert!(!repo.join("packages/org.example.lib-1.0.0.lar").is_file());
}
