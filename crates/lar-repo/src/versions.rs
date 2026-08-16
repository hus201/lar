use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use lar_store::Store;

use crate::advisories::verify_advisories;
use crate::sources::{load_sources, ordered_sources};
use crate::transport::{parse_uri, read_advisories, read_index};
use crate::trust::load_trust;
use crate::Result;

/// Result of probing configured sources while listing dependency versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceProbe {
    pub name: String,
    /// True when the source URI parsed and its index was read successfully.
    pub available: bool,
    /// Empty when available; otherwise a short failure reason.
    pub detail: String,
}

/// Candidate versions plus per-source probe results from [`list_dep_versions`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DepVersionList {
    pub versions: Vec<String>,
    pub sources: Vec<SourceProbe>,
}

impl DepVersionList {
    /// True when at least one configured source could not be read.
    pub fn any_source_unavailable(&self) -> bool {
        self.sources.iter().any(|s| !s.available)
    }
}

/// Format probe results for resolve / CLI errors.
pub fn format_source_probes(probes: &[SourceProbe]) -> String {
    if probes.is_empty() {
        return "Sources evaluated: (none configured)".into();
    }
    let width = probes.iter().map(|p| p.name.len()).max().unwrap_or(1);
    let mut out = String::from("Sources evaluated:");
    for p in probes {
        if p.available {
            let _ = fmt::Write::write_fmt(
                &mut out,
                format_args!("\n  {:width$} ✓", p.name, width = width),
            );
        } else {
            let _ = fmt::Write::write_fmt(
                &mut out,
                format_args!("\n  {:width$} ✗ {}", p.name, p.detail, width = width),
            );
        }
    }
    out
}

/// A yanked index pin that would otherwise be a resolve candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YankedDepVersion {
    pub version: String,
    pub advisory: String,
    pub source: String,
}

/// List candidate versions of `id` for dependency resolution.
///
/// Collects every version present in the local store, plus non-yanked versions
/// published in any configured source. Version selection (highest matching) and
/// source selection (highest-priority source for a chosen pin) happen elsewhere.
///
/// Sources whose URI cannot be parsed or whose index cannot be read are skipped
/// for candidates but recorded in [`DepVersionList::sources`] so resolve can
/// explain discovery failures.
pub fn list_dep_versions(store: &Store, id: &str) -> Result<DepVersionList> {
    let mut versions = BTreeSet::new();
    let mut sources = Vec::new();

    for pkg in store.list()? {
        if pkg.id == id {
            versions.insert(pkg.version);
        }
    }
    sources.push(SourceProbe {
        name: "(store)".into(),
        available: true,
        detail: String::new(),
    });

    let file = load_sources(store)?;
    let trust = load_trust(store)?;
    for src in ordered_sources(&file) {
        let base = match parse_uri(&src.uri) {
            Ok(base) => base,
            Err(err) => {
                sources.push(SourceProbe {
                    name: src.name.clone(),
                    available: false,
                    detail: short_probe_reason(&err),
                });
                continue;
            }
        };
        let index = match read_index(&base) {
            Ok(index) => index,
            Err(err) => {
                sources.push(SourceProbe {
                    name: src.name.clone(),
                    available: false,
                    detail: short_probe_reason(&err),
                });
                continue;
            }
        };
        // Advisories / trust failures are hard errors (security), not soft skips.
        let advisories = read_advisories(&base)?;
        verify_advisories(&advisories, &trust)?;
        sources.push(SourceProbe {
            name: src.name.clone(),
            available: true,
            detail: String::new(),
        });
        for pkg in &index.packages {
            if pkg.id != id {
                continue;
            }
            let yanked = advisories
                .matches(&pkg.id, &pkg.version, Some(&pkg.content_hash))
                .iter()
                .any(|a| a.yanked);
            if yanked {
                continue;
            }
            versions.insert(pkg.version.clone());
        }
    }

    Ok(DepVersionList {
        versions: versions.into_iter().collect(),
        sources,
    })
}

fn short_probe_reason(err: &crate::Error) -> String {
    match err {
        crate::Error::UnsupportedUri(_) => "invalid uri".into(),
        crate::Error::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            "unavailable (not found)".into()
        }
        crate::Error::Http { message, .. } => {
            if message.contains("404") {
                "unavailable (HTTP 404)".into()
            } else {
                format!("unavailable ({message})")
            }
        }
        crate::Error::InvalidIndex(msg) => format!("invalid index ({msg})"),
        other => format!("unavailable ({other})"),
    }
}

/// List yanked index pins for `id` (excluded from [`list_dep_versions`]).
///
/// Used to explain resolve failures when the only range matches are yanked.
/// If the same version is yanked in multiple sources, the highest-priority
/// source's advisory wins.
pub fn list_yanked_dep_versions(store: &Store, id: &str) -> Result<Vec<YankedDepVersion>> {
    let mut by_version: BTreeMap<String, YankedDepVersion> = BTreeMap::new();

    let sources = load_sources(store)?;
    let trust = load_trust(store)?;
    for src in ordered_sources(&sources) {
        let Ok(base) = parse_uri(&src.uri) else {
            continue;
        };
        let Ok(index) = read_index(&base) else {
            continue;
        };
        let advisories = read_advisories(&base)?;
        verify_advisories(&advisories, &trust)?;
        for pkg in &index.packages {
            if pkg.id != id {
                continue;
            }
            let Some(adv) = advisories
                .matches(&pkg.id, &pkg.version, Some(&pkg.content_hash))
                .into_iter()
                .find(|a| a.yanked)
            else {
                continue;
            };
            // First source in priority order wins for a given version.
            by_version
                .entry(pkg.version.clone())
                .or_insert_with(|| YankedDepVersion {
                    version: pkg.version.clone(),
                    advisory: adv.id.clone(),
                    source: src.name.clone(),
                });
        }
    }

    Ok(by_version.into_values().collect())
}
