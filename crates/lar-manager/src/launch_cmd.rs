//! Stable `{prefix}/libexec/lar` symlink and shell quoting helpers.

use std::env;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

use lar_store::Store;

use crate::Error;
use crate::Result;

/// Refresh `{prefix}/libexec/lar` → current executable. Returns the link path.
pub fn ensure_libexec_lar(store: &Store) -> Result<PathBuf> {
    let link = store.paths().libexec_lar();
    let target = env::current_exe().map_err(|source| Error::Io {
        path: PathBuf::from("lar"),
        source,
    })?;
    let target = fs::canonicalize(&target).unwrap_or(target);

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
