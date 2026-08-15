use std::path::PathBuf;

use thiserror::Error;

/// Errors from the SxS store.
#[derive(Debug, Error)]
pub enum Error {
    #[error("package already exists in store: {id} {version}")]
    AlreadyExists { id: String, version: String },

    #[error("package not found in store: {id} {version}")]
    NotFound { id: String, version: String },

    #[error("cannot remove {id} {version}: still required by {dependents}")]
    InUse {
        id: String,
        version: String,
        dependents: String,
    },

    #[error(transparent)]
    Package(#[from] lar_package::Error),

    #[error("IO error at {}: {source}", .path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("{0}")]
    Other(String),
}
