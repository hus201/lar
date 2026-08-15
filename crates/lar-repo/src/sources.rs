use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::policy::SourcePolicy;
use crate::Error;
use crate::Result;

pub const SOURCES_FORMAT: u32 = 1;

/// Configured package sources (`sources.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SourcesFile {
    #[serde(default = "default_format")]
    pub format: u32,
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
    pub policy: SourcePolicy,
    #[serde(default)]
    pub main: bool,
}

impl SourcesFile {
    pub fn validate(&self) -> Result<()> {
        if self.format != SOURCES_FORMAT {
            return Err(Error::InvalidSources(format!(
                "unsupported format {} (supported: {SOURCES_FORMAT})",
                self.format
            )));
        }
        let mut mains = 0usize;
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
            if src.main {
                mains += 1;
                if !src.policy.allows_deps() {
                    return Err(Error::InvalidSources(
                        "main source must allow deps (policy deps or both)".into(),
                    ));
                }
            }
        }
        if mains > 1 {
            return Err(Error::InvalidSources(
                "at most one source may be marked main".into(),
            ));
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

/// Add a source; `main` must be unique.
pub fn add_source(
    store: &lar_store::Store,
    name: String,
    uri: String,
    policy: SourcePolicy,
    main: bool,
) -> Result<SourceEntry> {
    let mut file = load_sources(store)?;
    if file.sources.iter().any(|s| s.name == name) {
        return Err(Error::SourceExists(name));
    }
    if main {
        if let Some(existing) = file.sources.iter().find(|s| s.main) {
            return Err(Error::MainAlreadySet(existing.name.clone()));
        }
    }
    let entry = SourceEntry {
        name,
        uri,
        policy,
        main,
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

/// Sources that may supply dependencies, **main first**.
pub fn ordered_deps_sources(file: &SourcesFile) -> Vec<&SourceEntry> {
    let mut out: Vec<&SourceEntry> = file
        .sources
        .iter()
        .filter(|s| s.policy.allows_deps())
        .collect();
    out.sort_by_key(|s| if s.main { 0u8 } else { 1u8 });
    out
}

/// Sources that may supply installable apps (config order; skip deps-only).
pub fn ordered_apps_sources(file: &SourcesFile) -> Vec<&SourceEntry> {
    file.sources
        .iter()
        .filter(|s| s.policy.allows_apps())
        .collect()
}

/// Default source name from uri/path.
pub fn default_source_name(uri: &str, main: bool) -> String {
    if main {
        return "main".into();
    }
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
