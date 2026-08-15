use serde::{Deserialize, Serialize};

use lar_runtime::ComposeMode;

/// Current install record format.
pub const INSTALL_FORMAT: u32 = 1;

/// On-disk / in-memory install record (`install.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallRecord {
    pub format: u32,
    pub id: String,
    pub version: String,
    pub content_hash: String,
    pub runtime_id: String,
    #[serde(default)]
    pub compose: ComposeMode,
    pub packages: Vec<InstallPackage>,
}

/// One package pinned by an install.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallPackage {
    pub id: String,
    pub version: String,
    pub content_hash: String,
}

impl InstallRecord {
    pub fn validate(&self) -> Result<(), String> {
        if self.format != INSTALL_FORMAT {
            return Err(format!(
                "unsupported install format {} (supported: {INSTALL_FORMAT})",
                self.format
            ));
        }
        if self.id.is_empty() || self.version.is_empty() {
            return Err("id and version must be non-empty".into());
        }
        if self.runtime_id.is_empty() {
            return Err("runtime_id must be non-empty".into());
        }
        if !self.content_hash.starts_with("blake3:") {
            return Err("content_hash must look like blake3:<hex>".into());
        }
        if self.packages.is_empty() {
            return Err("packages must not be empty".into());
        }
        Ok(())
    }
}
