use std::path::PathBuf;

use thiserror::Error;

/// Errors from dependency resolution and lockfile IO.
#[derive(Debug, Error)]
pub enum Error {
    #[error("package not found in store: {id} {version}")]
    Missing { id: String, version: String },

    #[error(
        "dependency conflict for {id}: required {required} but already resolved as {resolved}"
    )]
    Conflict {
        id: String,
        required: String,
        resolved: String,
    },

    #[error("dependency cycle detected involving {id} {version}")]
    Cycle { id: String, version: String },

    #[error(transparent)]
    Package(#[from] lar_package::Error),

    #[error(transparent)]
    Store(#[from] lar_store::Error),

    #[error(transparent)]
    Repo(#[from] lar_repo::Error),

    #[error("IO error at {}: {source}", .path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("invalid lockfile: {0}")]
    InvalidLockfile(String),

    #[error("content_hash mismatch for {id} {version}: lock has {locked}, store has {store}")]
    HashMismatch {
        id: String,
        version: String,
        locked: String,
        store: String,
    },

    #[error("dependencies mismatch for {id} {version}: lock does not match store manifest")]
    DependencyMismatch { id: String, version: String },

    #[error("{0}")]
    Other(String),
}
