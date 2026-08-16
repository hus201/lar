//! Stable `{prefix}/libexec/lar-exec` symlink and shell quoting helpers.

use std::cell::RefCell;
use std::env;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use lar_store::Store;

use crate::Error;
use crate::Result;

thread_local! {
    /// Per-thread override for unit tests (avoids racing on process-global `LAR_EXEC`).
    static LAR_EXEC_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Set a thread-local `lar-exec` path (tests). Pass `None` to clear.
pub fn set_lar_exec_override(path: Option<PathBuf>) {
    LAR_EXEC_OVERRIDE.with(|slot| {
        *slot.borrow_mut() = path;
    });
}

/// Refresh `{prefix}/libexec/lar-exec` → the slim trampoline binary. Returns the link path.
pub fn ensure_libexec_lar_exec(store: &Store) -> Result<PathBuf> {
    let link = store.paths().libexec_lar_exec();
    let target = resolve_lar_exec_path()?;

    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    match link.symlink_metadata() {
        Ok(_) => fs::remove_file(&link).map_err(|source| Error::Io {
            path: link.clone(),
            source,
        })?,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(Error::Io {
                path: link.clone(),
                source,
            });
        }
    }

    symlink(&target, &link).map_err(|source| Error::Io {
        path: link.clone(),
        source,
    })?;
    Ok(link)
}

/// Locate the `lar-exec` binary: thread override, `LAR_EXEC`, self, or sibling of `lar`.
pub fn resolve_lar_exec_path() -> Result<PathBuf> {
    if let Some(path) = LAR_EXEC_OVERRIDE.with(|slot| slot.borrow().clone()) {
        if path.as_os_str().is_empty() {
            return Err(Error::Other("lar-exec override path is empty".into()));
        }
        return Ok(fs::canonicalize(&path).unwrap_or(path));
    }

    if let Some(override_path) = env::var_os("LAR_EXEC") {
        let path = PathBuf::from(override_path);
        if path.as_os_str().is_empty() {
            return Err(Error::Other("LAR_EXEC is empty".into()));
        }
        return Ok(fs::canonicalize(&path).unwrap_or(path));
    }

    let exe = env::current_exe().map_err(|source| Error::Io {
        path: PathBuf::from("lar-exec"),
        source,
    })?;
    let exe = fs::canonicalize(&exe).unwrap_or(exe);

    if file_stem_eq(&exe, "lar-exec") {
        return Ok(exe);
    }

    let sibling = exe.with_file_name("lar-exec");
    if path_exists(&sibling) {
        return Ok(fs::canonicalize(&sibling).unwrap_or(sibling));
    }

    Err(Error::Other(
        "lar-exec not found next to the lar binary (build/install lar-exec, or set LAR_EXEC)"
            .into(),
    ))
}

fn file_stem_eq(path: &Path, name: &str) -> bool {
    path.file_name().and_then(|s| s.to_str()) == Some(name)
}

fn path_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

pub(crate) fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .any(|c| c.is_whitespace() || "\"'\\$`".contains(c))
    {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_string()
    }
}
