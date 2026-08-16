use thiserror::Error;

/// Errors from platform capability parsing / checks.
#[derive(Debug, Error)]
pub enum Error {
    #[error("unknown platform capability `{0}`")]
    UnknownCapability(String),

    #[error("platform requirements not met: {0}")]
    RequirementsNotMet(String),

    #[error("{0}")]
    Other(String),
}
