use serde::{Deserialize, Serialize};

use crate::Error;
use crate::Result;

pub const ADVISORIES_FORMAT: u32 = 1;

/// Repo-published vulnerability metadata (`advisories.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct AdvisoriesFile {
    #[serde(default = "default_format")]
    pub format: u32,
    #[serde(default)]
    pub advisories: Vec<Advisory>,
}

fn default_format() -> u32 {
    ADVISORIES_FORMAT
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Advisory {
    pub id: String,
    pub package_id: String,
    #[serde(default)]
    pub versions: Vec<String>,
    #[serde(default)]
    pub content_hashes: Vec<String>,
    pub severity: Severity,
    #[serde(default)]
    pub yanked: bool,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    pub fn is_elevated(self) -> bool {
        matches!(self, Self::High | Self::Critical)
    }
}

impl AdvisoriesFile {
    pub fn validate(&self) -> Result<()> {
        if self.format != ADVISORIES_FORMAT {
            return Err(Error::InvalidAdvisories(format!(
                "unsupported format {} (supported: {ADVISORIES_FORMAT})",
                self.format
            )));
        }
        for adv in &self.advisories {
            if adv.id.is_empty() || adv.package_id.is_empty() {
                return Err(Error::InvalidAdvisories(
                    "advisory id and package_id must be non-empty".into(),
                ));
            }
            if adv.versions.is_empty() && adv.content_hashes.is_empty() {
                return Err(Error::InvalidAdvisories(format!(
                    "advisory {} must list versions and/or content_hashes",
                    adv.id
                )));
            }
        }
        Ok(())
    }

    pub fn matches(
        &self,
        package_id: &str,
        version: &str,
        content_hash: Option<&str>,
    ) -> Vec<&Advisory> {
        self.advisories
            .iter()
            .filter(|a| {
                if a.package_id != package_id {
                    return false;
                }
                let ver_ok = a.versions.is_empty() || a.versions.iter().any(|v| v == version);
                let hash_ok = a.content_hashes.is_empty()
                    || content_hash
                        .map(|h| a.content_hashes.iter().any(|c| c == h))
                        .unwrap_or(false);
                // If both lists non-empty, both must match; if one empty, the other gates.
                match (!a.versions.is_empty(), !a.content_hashes.is_empty()) {
                    (true, true) => ver_ok && hash_ok,
                    (true, false) => ver_ok,
                    (false, true) => hash_ok,
                    (false, false) => false,
                }
            })
            .collect()
    }
}

pub fn parse_advisories(text: &str) -> Result<AdvisoriesFile> {
    let file: AdvisoriesFile =
        toml::from_str(text).map_err(|err| Error::InvalidAdvisories(err.to_string()))?;
    file.validate()?;
    Ok(file)
}

/// Empty advisories when the file is absent.
pub fn empty_advisories() -> AdvisoriesFile {
    AdvisoriesFile {
        format: ADVISORIES_FORMAT,
        advisories: Vec::new(),
    }
}
