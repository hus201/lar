use std::path::PathBuf;

use thiserror::Error;

/// Errors from install / uninstall.
#[derive(Debug, Error)]
pub enum Error {
    #[error("application already installed: {0} (use --force to replace)")]
    AlreadyInstalled(String),

    #[error("application not installed: {0}")]
    NotInstalled(String),

    #[error("package not found in store: {id} {version}")]
    NotInStore { id: String, version: String },

    #[error(
        "ambiguous package id `{id}`: multiple versions in store ({versions}); use id@version"
    )]
    AmbiguousVersion { id: String, versions: String },

    #[error(
        "package {id} {version} already in store with different content (archive {archive}, store {store})"
    )]
    HashMismatch {
        id: String,
        version: String,
        archive: String,
        store: String,
    },

    #[error("invalid install source `{0}`")]
    InvalidSource(String),

    #[error("invalid install record: {0}")]
    InvalidRecord(String),

    #[error(transparent)]
    Package(#[from] lar_package::Error),

    #[error(transparent)]
    Resolver(#[from] lar_resolver::Error),

    #[error(transparent)]
    Runtime(#[from] lar_runtime::Error),

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
