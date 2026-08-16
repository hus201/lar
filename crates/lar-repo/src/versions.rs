use std::collections::{BTreeMap, BTreeSet};

use lar_store::Store;

use crate::advisories::verify_advisories;
use crate::sources::{load_sources, ordered_sources};
use crate::transport::{parse_uri, read_advisories, read_index};
use crate::trust::load_trust;
use crate::Result;

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
pub fn list_dep_versions(store: &Store, id: &str) -> Result<Vec<String>> {
    let mut versions = BTreeSet::new();

    for pkg in store.list()? {
        if pkg.id == id {
            versions.insert(pkg.version);
        }
    }

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

    Ok(versions.into_iter().collect())
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
