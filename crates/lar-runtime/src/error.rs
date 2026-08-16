use std::path::PathBuf;

use thiserror::Error;

/// Errors from runtime composition and launch.
#[derive(Debug, Error)]
pub enum Error {
    #[error("lockfile not found at {}", .0.display())]
    LockfileNotFound(PathBuf),

    #[error("path conflict in runtime for `{path}`: provided by both {first} and {second}")]
    PathConflict {
        path: String,
        first: String,
        second: String,
    },

    #[error("root package {id} {version} has no [entry] binaries to run")]
    NoEntry { id: String, version: String },

    #[error("entry binary not found in runtime: {0}")]
    EntryMissing(String),

    #[error("runtime not found: {}", .0.display())]
    RuntimeNotFound(PathBuf),

    #[error("invalid runtime metadata: {0}")]
    InvalidRuntime(String),

    #[error("runtime filesystem verification failed: {0}")]
    VerifyFailed(String),

    #[error("invalid compose mode `{0}` (expected symlink, hardlink, or copy)")]
    InvalidComposeMode(String),

    #[error("hardlink failed for {}: {source} (store and runtimes must share a filesystem)", .path.display())]
    Hardlink {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error(transparent)]
    Resolver(#[from] lar_resolver::Error),

    #[error(transparent)]
    Package(#[from] lar_package::Error),

    #[error(transparent)]
    Store(#[from] lar_store::Error),

    #[error("IO error at {}: {source}", .path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("{0}")]
    Other(String),
}
