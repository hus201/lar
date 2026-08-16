//! Slim PATH-export trampoline: apply runtime env and `exec` the entry ELF.
//!
//! Invoked via `{prefix}/bin/{cmd}` → `{prefix}/libexec/lar-exec` with `argv[0]`
//! basename `{cmd}`. Not the `lar` CLI.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use lar_manager::exec_path_export;

fn main() -> ExitCode {
    let argv0 = match env::args_os().next() {
        Some(a) => PathBuf::from(a),
        None => {
            eprintln!("lar-exec: missing argv[0]");
            return ExitCode::FAILURE;
        }
    };
    let args: Vec<String> = env::args().skip(1).collect();
    match exec_path_export(&argv0, &args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("lar-exec: {err}");
            ExitCode::FAILURE
        }
    }
}
