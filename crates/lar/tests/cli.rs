use std::process::Command;

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
fn commands_are_stubbed() {
    let output = lar()
        .args(["store", "list"])
        .output()
        .expect("failed to run lar");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("lar store list: not implemented yet"),
        "{stderr}"
    );
}
