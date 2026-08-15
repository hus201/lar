use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::Error;

/// What a package source may provide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourcePolicy {
    Deps,
    Apps,
    Both,
}

impl SourcePolicy {
    pub fn allows_deps(self) -> bool {
        matches!(self, Self::Deps | Self::Both)
    }

    pub fn allows_apps(self) -> bool {
        matches!(self, Self::Apps | Self::Both)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deps => "deps",
            Self::Apps => "apps",
            Self::Both => "both",
        }
    }
}

impl fmt::Display for SourcePolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SourcePolicy {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "deps" => Ok(Self::Deps),
            "apps" => Ok(Self::Apps),
            "both" => Ok(Self::Both),
            other => Err(Error::Other(format!(
                "invalid policy `{other}` (expected deps, apps, or both)"
            ))),
        }
    }
}

/// Lookup mode when searching sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupMode {
    Deps,
    Apps,
}
