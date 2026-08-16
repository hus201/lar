use std::path::PathBuf;

use thiserror::Error;

/// Errors from package sources, fetch, trust, and advisories.
#[derive(Debug, Error)]
pub enum Error {
    #[error("source already configured: {0}")]
    SourceExists(String),

    #[error("source not found: {0}")]
    SourceNotFound(String),

    #[error("unsupported source uri `{0}` (use a path, file://, http://, or https://)")]
    UnsupportedUri(String),

    #[error("package {id} {version} not found in configured sources")]
    PackageNotFound { id: String, version: String },

    #[error("package {id} {version} is yanked ({advisory})")]
    Yanked {
        id: String,
        version: String,
        advisory: String,
    },

    #[error("untrusted key_id `{0}` (add it with `lar repo trust add`)")]
    UntrustedKey(String),

    #[error("invalid signature for {id} {version}")]
    BadSignature { id: String, version: String },

    #[error("invalid advisories signature (key_id `{key_id}`)")]
    BadAdvisoriesSignature { key_id: String },

    #[error("content hash mismatch for {id} {version}: index {index}, archive {archive}")]
    HashMismatch {
        id: String,
        version: String,
        index: String,
        archive: String,
    },

    #[error("invalid relative path in index: {0}")]
    InvalidRelativePath(String),

    #[error("key already trusted: {0}")]
    KeyExists(String),

    #[error("trusted key not found: {0}")]
    KeyNotFound(String),

    #[error("invalid public key: {0}")]
    InvalidKey(String),

    #[error("invalid secret key: {0}")]
    InvalidSecretKey(String),

    #[error("HTTP error fetching {url}: {message}")]
    Http { url: String, message: String },

    #[error("invalid index: {0}")]
    InvalidIndex(String),

    #[error("invalid advisories: {0}")]
    InvalidAdvisories(String),

    #[error("invalid sources config: {0}")]
    InvalidSources(String),

    #[error("invalid trust config: {0}")]
    InvalidTrust(String),

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
