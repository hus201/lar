//! Light PATH-export trampoline support (no install/repo/CLI stack).

mod error;
mod exec;
mod export;
mod launch;

pub use error::Error;
pub use exec::exec_path_export;
pub use export::{load_export_meta, resolve_export_from_argv0, ExportMeta, EXPORT_FORMAT};
pub use launch::{bin_search_paths, library_search_paths, runtime_launch_env, RuntimeLaunchEnv};

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;
