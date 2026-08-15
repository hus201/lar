use std::path::PathBuf;

use thiserror::Error;

/// Errors produced while loading, validating, or packing packages.
#[derive(Debug, Error)]
pub enum Error {
    #[error("manifest not found: {}", .0.display())]
    ManifestNotFound(PathBuf),

    #[error("invalid path: {}", .0.display())]
    InvalidPath(PathBuf),

    #[error("invalid package id `{id}`: {reason}")]
    InvalidPackageId { id: String, reason: String },

    #[error("invalid version `{version}`: {reason}")]
    InvalidVersion { version: String, reason: String },

    #[error("{0}")]
    Validation(String),

    #[error("package.toml already exists at {}", .0.display())]
    ManifestExists(PathBuf),

    #[error("IO error at {}: {source}", .path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse package.toml: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("failed to serialize package.toml: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("failed to parse JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("archive error: {0}")]
    Archive(String),

    #[error("integrity check failed: {0}")]
    Integrity(String),
}
