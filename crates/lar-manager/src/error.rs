use std::path::PathBuf;

use thiserror::Error;

/// Errors from install / uninstall.
#[derive(Debug, Error)]
pub enum Error {
    #[error("application already installed: {0} (use --force to replace)")]
    AlreadyInstalled(String),

    #[error("application not installed: {0}")]
    NotInstalled(String),

    #[error("no previous install to roll back for {0}")]
    NoPrevious(String),

    #[error("runtime missing for {id} (runtime_id {runtime_id}); reinstall the application")]
    RuntimeMissing { id: String, runtime_id: String },

    #[error("application {0} has no [entry] binaries to launch")]
    NoEntry(String),

    #[error("refusing to overwrite non-LAR file at {path}")]
    ExportCollision { path: String },

    #[error("entry binary `{binary}` is not listed in [entry] for {id}")]
    UnknownBinary { id: String, binary: String },

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

    #[error("platform requirements not met: {0}")]
    Platform(String),

    #[error(transparent)]
    Package(#[from] lar_package::Error),

    #[error(transparent)]
    Resolver(#[from] lar_resolver::Error),

    #[error(transparent)]
    Runtime(#[from] lar_runtime::Error),

    #[error(transparent)]
    Store(#[from] lar_store::Error),

    #[error(transparent)]
    Repo(#[from] lar_repo::Error),

    #[error(transparent)]
    Trampoline(#[from] lar_trampoline::Error),

    #[error("IO error at {}: {source}", .path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("{0}")]
    Other(String),
}
