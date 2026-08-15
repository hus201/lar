use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::compose::ComposeMode;

/// Current runtime metadata format.
pub const RUNTIME_FORMAT: u32 = 1;

/// Metadata written as `runtime.toml` inside a composed runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeMeta {
    pub format: u32,
    pub runtime_id: String,
    #[serde(default)]
    pub compose: ComposeMode,
    pub root: RuntimeRoot,
    pub packages: Vec<RuntimePackage>,
}

/// Root application identity recorded in the runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeRoot {
    pub id: String,
    pub version: String,
}

/// One package included in the runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimePackage {
    pub id: String,
    pub version: String,
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, String>,
}
