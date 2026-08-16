//! Native PATH-export trampoline: apply runtime env and `exec` the entry ELF.

use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

use lar_runtime::runtime_launch_env;

use crate::exports::resolve_export_from_argv0;
use crate::Error;
use crate::Result;

/// If `argv0` resolves to a LAR PATH export, apply runtime env and `exec` the entry.
///
/// On success this function does not return.
pub fn exec_path_export(argv0: &Path, args: &[String]) -> Result<()> {
    let Some((_cmd, meta)) = resolve_export_from_argv0(argv0)? else {
        return Err(Error::Other(format!(
            "not a LAR PATH export: {}",
            argv0.display()
        )));
    };

    if !meta.binary.is_file() {
        return Err(Error::Other(format!(
            "export binary missing at {} (reinstall the application?)",
            meta.binary.display()
        )));
    }
    if !meta.runtime.is_dir() {
        return Err(Error::Other(format!(
            "export runtime missing at {} (reinstall the application?)",
            meta.runtime.display()
        )));
    }

    let env = runtime_launch_env(&meta.runtime);
    let mut cmd = Command::new(&meta.binary);
    cmd.args(args);

    if !env.path_prepend.is_empty() {
        let current = std::env::var_os("PATH").unwrap_or_default();
        let mut new_path = env.path_prepend.clone();
        if !current.is_empty() {
            new_path.push(":");
            new_path.push(current);
        }
        cmd.env("PATH", new_path);
    }
    if !env.ld_library_path_prepend.is_empty() {
        let current = std::env::var_os("LD_LIBRARY_PATH").unwrap_or_default();
        let mut new_path = env.ld_library_path_prepend.clone();
        if !current.is_empty() {
            new_path.push(":");
            new_path.push(current);
        }
        cmd.env("LD_LIBRARY_PATH", new_path);
    }
    cmd.env("LAR_RUNTIME", &env.lar_runtime);

    let err = cmd.exec();
    Err(Error::Io {
        path: meta.binary,
        source: err,
    })
}
