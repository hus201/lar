use std::fmt;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::Error;
use crate::Result;

pub const SOURCES_FORMAT: u32 = 1;

/// How to choose among sources that publish the same exact pin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum FetchPriority {
    /// First matching source in configuration order wins (default).
    #[default]
    FirstWin,
    /// Last matching source in configuration order wins.
    LastWin,
}

impl FetchPriority {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FirstWin => "first-win",
            Self::LastWin => "last-win",
        }
    }
}

impl fmt::Display for FetchPriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for FetchPriority {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "first-win" | "first" => Ok(Self::FirstWin),
            "last-win" | "last" => Ok(Self::LastWin),
            other => Err(Error::InvalidSources(format!(
                "invalid fetch_priority `{other}` (expected first-win or last-win)"
            ))),
        }
    }
}

/// Configured package sources (`sources.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SourcesFile {
    #[serde(default = "default_format")]
    pub format: u32,
    /// Fetch conflict rule when multiple sources publish the same pin.
    #[serde(default)]
    pub fetch_priority: FetchPriority,
    #[serde(default)]
    pub sources: Vec<SourceEntry>,
}

fn default_format() -> u32 {
    SOURCES_FORMAT
}

/// One configured package source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEntry {
    pub name: String,
    pub uri: String,
    /// Legacy fields from older configs; accepted on read, never written back.
    #[serde(default, skip_serializing)]
    policy: Option<String>,
    #[serde(default, skip_serializing)]
    main: Option<bool>,
}

impl SourcesFile {
    pub fn validate(&self) -> Result<()> {
        if self.format != SOURCES_FORMAT {
            return Err(Error::InvalidSources(format!(
                "unsupported format {} (supported: {SOURCES_FORMAT})",
                self.format
            )));
        }
        let mut names = std::collections::BTreeSet::new();
        for src in &self.sources {
            if src.name.is_empty() || src.uri.is_empty() {
                return Err(Error::InvalidSources(
                    "source name and uri must be non-empty".into(),
                ));
            }
            if !names.insert(src.name.clone()) {
                return Err(Error::InvalidSources(format!(
                    "duplicate source name `{}`",
                    src.name
                )));
            }
        }
        Ok(())
    }
}

/// Load sources from the store prefix config (missing file → empty).
pub fn load_sources(store: &lar_store::Store) -> Result<SourcesFile> {
    let path = store.paths().sources_toml();
    if !path.is_file() {
        return Ok(SourcesFile {
            format: SOURCES_FORMAT,
            fetch_priority: FetchPriority::FirstWin,
            sources: Vec::new(),
        });
    }
    let text = fs::read_to_string(&path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    let file: SourcesFile =
        toml::from_str(&text).map_err(|err| Error::InvalidSources(err.to_string()))?;
    file.validate()?;
    Ok(file)
}

/// Atomically write sources.toml.
pub fn save_sources(store: &lar_store::Store, file: &SourcesFile) -> Result<()> {
    file.validate()?;
    let path = store.paths().sources_toml();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let text = toml::to_string_pretty(file)
        .map_err(|err| Error::Other(format!("serialize sources.toml: {err}")))?;
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text).map_err(|source| Error::Io {
        path: tmp.clone(),
        source,
    })?;
    fs::rename(&tmp, &path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    Ok(())
}

/// Add a source.
pub fn add_source(store: &lar_store::Store, name: String, uri: String) -> Result<SourceEntry> {
    let mut file = load_sources(store)?;
    if file.sources.iter().any(|s| s.name == name) {
        return Err(Error::SourceExists(name));
    }
    let entry = SourceEntry {
        name,
        uri,
        policy: None,
        main: None,
    };
    file.sources.push(entry.clone());
    save_sources(store, &file)?;
    Ok(entry)
}

/// Remove by name or uri.
pub fn remove_source(store: &lar_store::Store, name_or_uri: &str) -> Result<SourceEntry> {
    let mut file = load_sources(store)?;
    let idx = file
        .sources
        .iter()
        .position(|s| s.name == name_or_uri || s.uri == name_or_uri)
        .ok_or_else(|| Error::SourceNotFound(name_or_uri.to_string()))?;
    let entry = file.sources.remove(idx);
    save_sources(store, &file)?;
    Ok(entry)
}

/// Set fetch priority (`first-win` or `last-win`).
pub fn set_fetch_priority(store: &lar_store::Store, priority: FetchPriority) -> Result<FetchPriority> {
    let mut file = load_sources(store)?;
    file.fetch_priority = priority;
    save_sources(store, &file)?;
    Ok(priority)
}

/// Sources in the order fetch should try them (respects `fetch_priority`).
pub fn ordered_sources(file: &SourcesFile) -> Vec<&SourceEntry> {
    let mut out: Vec<&SourceEntry> = file.sources.iter().collect();
    if file.fetch_priority == FetchPriority::LastWin {
        out.reverse();
    }
    out
}

/// Default source name from uri/path.
pub fn default_source_name(uri: &str) -> String {
    let trimmed = uri.trim_end_matches('/');
    if let Ok(url) = url::Url::parse(trimmed) {
        if let Some(seg) = url
            .path_segments()
            .and_then(|mut s| s.next_back())
            .filter(|s| !s.is_empty())
        {
            return seg.to_string();
        }
        if let Some(host) = url.host_str() {
            return host.to_string();
        }
    }
    Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("source")
        .to_string()
}
