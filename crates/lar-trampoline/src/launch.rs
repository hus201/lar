//! Shared runtime launch environment (`PATH` / `LD_LIBRARY_PATH` / `LAR_RUNTIME`).

use std::fs;
use std::path::{Path, PathBuf};

/// Environment directories prepended when launching from a composed runtime.
#[derive(Debug, Clone)]
pub struct RuntimeLaunchEnv {
    /// Colon-joined bin dirs to prepend to `PATH` (may be empty).
    pub path_prepend: std::ffi::OsString,
    /// Colon-joined lib dirs to prepend to `LD_LIBRARY_PATH` (may be empty).
    pub ld_library_path_prepend: std::ffi::OsString,
    /// Absolute runtime directory (`LAR_RUNTIME`).
    pub lar_runtime: PathBuf,
}

/// Compute the shared launch env used by `lar-exec`, `lar launch`, and `lar run`.
pub fn runtime_launch_env(runtime_path: &Path) -> RuntimeLaunchEnv {
    let files = runtime_path.join("files");
    RuntimeLaunchEnv {
        path_prepend: join_paths(&bin_search_paths(&files)),
        ld_library_path_prepend: join_paths(&library_search_paths(&files)),
        lar_runtime: runtime_path.to_path_buf(),
    }
}

/// Directories prepended to `PATH` when launching.
pub fn bin_search_paths(files: &Path) -> Vec<PathBuf> {
    const ROOTS: &[&str] = &["bin", "usr/bin", "sbin", "usr/sbin"];
    let mut dirs = Vec::new();
    for rel in ROOTS {
        let dir = files.join(rel);
        if dir.is_dir() {
            dirs.push(dir);
        }
    }
    dirs
}

/// Directories prepended to `LD_LIBRARY_PATH` when launching.
///
/// Includes common FHS roots under `files/` (`lib`, `lib64`, `lib32`, and
/// `usr/...` equivalents) plus one level of subdirectories (e.g.
/// `lib/x86_64-linux-gnu`).
pub fn library_search_paths(files: &Path) -> Vec<PathBuf> {
    const ROOTS: &[&str] = &["lib", "lib64", "lib32", "usr/lib", "usr/lib64", "usr/lib32"];
    let mut dirs = Vec::new();
    for rel in ROOTS {
        let root = files.join(rel);
        if !root.is_dir() {
            continue;
        }
        dirs.push(root.clone());
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        let mut children = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                children.push(path);
            }
        }
        children.sort();
        dirs.extend(children);
    }
    dirs
}

fn join_paths(dirs: &[PathBuf]) -> std::ffi::OsString {
    let mut s = std::ffi::OsString::new();
    for (i, dir) in dirs.iter().enumerate() {
        if i > 0 {
            s.push(":");
        }
        s.push(dir.as_os_str());
    }
    s
}
