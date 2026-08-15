use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::Error;

/// How package `files/` are materialized into a runtime tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ComposeMode {
    /// Relative symlinks into the SxS store (default).
    #[default]
    Symlink,
    /// Hard links to store files (same filesystem required).
    Hardlink,
    /// Byte copies of store files into the runtime.
    Copy,
}

impl ComposeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Symlink => "symlink",
            Self::Hardlink => "hardlink",
            Self::Copy => "copy",
        }
    }
}

impl fmt::Display for ComposeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ComposeMode {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "symlink" => Ok(Self::Symlink),
            "hardlink" => Ok(Self::Hardlink),
            "copy" => Ok(Self::Copy),
            other => Err(Error::InvalidComposeMode(other.to_string())),
        }
    }
}
