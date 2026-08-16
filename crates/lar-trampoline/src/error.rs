use std::path::PathBuf;

use thiserror::Error;

/// Errors from export resolve / trampoline exec.
#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error at {}: {source}", .path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("{0}")]
    Other(String),
}
