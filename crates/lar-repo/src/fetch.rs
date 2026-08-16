use std::fs;
use std::io::{self, Write};

use lar_package::inspect;
use lar_store::{Store, StoredPackage};

use crate::advisories::{verify_advisories, AdvisoriesFile};
use crate::policy::LookupMode;
use crate::sources::{load_sources, ordered_apps_sources, ordered_deps_sources, SourceEntry};
use crate::transport::{fetch_blob, parse_uri, read_advisories, read_index};
use crate::trust::{find_trusted_key, load_trust, verify_content_hash};
use crate::Error;
use crate::Result;

/// Warning emitted for a matching advisory (non-yanked).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvisoryWarning {
    pub source: String,
    pub advisory_id: String,
    pub package_id: String,
    pub version: String,
    pub severity: String,
    pub summary: String,
    pub url: String,
    pub yanked: bool,
}

impl AdvisoryWarning {
    pub fn format_line(&self) -> String {
        format!(
            "warning: {} {} [{}] {} — {} ({}){}",
            self.package_id,
            self.version,
            self.severity,
            self.advisory_id,
            self.summary,
            self.source,
            if self.url.is_empty() {
                String::new()
            } else {
                format!(" {}", self.url)
            }
        )
    }
}

/// Fetch `id@version` into the store from configured sources.
pub fn fetch_into_store(
    store: &Store,
    id: &str,
    version: &str,
    mode: LookupMode,
    warn_out: &mut dyn Write,
) -> Result<StoredPackage> {
    if let Some(existing) = store.get(id, version)? {
        emit_store_hit_warnings(store, id, version, Some(&existing.content_hash), warn_out)?;
        return Ok(existing);
    }

    let sources = load_sources(store)?;
    let ordered: Vec<&SourceEntry> = match mode {
        LookupMode::Deps => ordered_deps_sources(&sources),
        LookupMode::Apps => ordered_apps_sources(&sources),
    };

    let mut last_miss = None;
    for src in ordered {
        match fetch_from_source(store, src, id, version, warn_out) {
            Ok(stored) => return Ok(stored),
            Err(Error::PackageNotFound { .. }) => {
                last_miss = Some(());
                continue;
            }
            Err(err) => return Err(err),
        }
    }

    let _ = last_miss;
    Err(Error::PackageNotFound {
        id: id.to_string(),
        version: version.to_string(),
    })
}

fn fetch_from_source(
    store: &Store,
    src: &SourceEntry,
    id: &str,
    version: &str,
    warn_out: &mut dyn Write,
) -> Result<StoredPackage> {
    let base = parse_uri(&src.uri)?;
    let index = read_index(&base)?;
    let trust = load_trust(store)?;
    let advisories = read_advisories(&base)?;
    verify_advisories(&advisories, &trust)?;
    let pkg = index
        .find(id, version)
        .ok_or_else(|| Error::PackageNotFound {
            id: id.to_string(),
            version: version.to_string(),
        })?;

    check_advisories_for_fetch(
        &advisories,
        src,
        id,
        version,
        Some(&pkg.content_hash),
        warn_out,
    )?;

    let trust = load_trust(store)?;
    let key = find_trusted_key(&trust, &pkg.key_id)
        .ok_or_else(|| Error::UntrustedKey(pkg.key_id.clone()))?;
    verify_content_hash(&key.public_key, &pkg.content_hash, &pkg.signature).map_err(|_| {
        Error::BadSignature {
            id: id.to_string(),
            version: version.to_string(),
        }
    })?;

    let tmp = fetch_blob(&base, &pkg.file)?;
    let result = (|| {
        let archive = inspect(&tmp)?;
        if archive.index.content_hash != pkg.content_hash {
            return Err(Error::HashMismatch {
                id: id.to_string(),
                version: version.to_string(),
                index: pkg.content_hash.clone(),
                archive: archive.index.content_hash,
            });
        }
        match store.add(&tmp) {
            Ok(stored) => Ok(stored),
            Err(lar_store::Error::AlreadyExists {
                id: ref eid,
                version: ref ever,
            }) => {
                let stored = store.get(eid, ever)?.ok_or_else(|| {
                    Error::Other(format!(
                        "package disappeared after AlreadyExists: {eid} {ever}"
                    ))
                })?;
                if stored.content_hash != pkg.content_hash {
                    return Err(Error::HashMismatch {
                        id: id.to_string(),
                        version: version.to_string(),
                        index: pkg.content_hash.clone(),
                        archive: stored.content_hash,
                    });
                }
                Ok(stored)
            }
            Err(err) => Err(err.into()),
        }
    })();
    let _ = fs::remove_file(&tmp);
    result
}

fn check_advisories_for_fetch(
    advisories: &AdvisoriesFile,
    src: &SourceEntry,
    id: &str,
    version: &str,
    content_hash: Option<&str>,
    warn_out: &mut dyn Write,
) -> Result<()> {
    let matches = advisories.matches(id, version, content_hash);
    for adv in matches {
        if adv.yanked {
            return Err(Error::Yanked {
                id: id.to_string(),
                version: version.to_string(),
                advisory: adv.id.clone(),
            });
        }
        let warning = AdvisoryWarning {
            source: src.name.clone(),
            advisory_id: adv.id.clone(),
            package_id: id.to_string(),
            version: version.to_string(),
            severity: adv.severity.as_str().to_string(),
            summary: adv.summary.clone(),
            url: adv.url.clone(),
            yanked: false,
        };
        let _ = writeln!(warn_out, "{}", warning.format_line());
    }
    Ok(())
}

/// Warn about advisories for a package already in the store (including yanked-in-use).
pub fn emit_store_hit_warnings(
    store: &Store,
    id: &str,
    version: &str,
    content_hash: Option<&str>,
    warn_out: &mut dyn Write,
) -> Result<()> {
    let warnings = collect_warnings_for_pin(store, id, version, content_hash)?;
    for w in warnings {
        let mut line = w.format_line();
        if w.yanked {
            line = format!("{line} (yanked but already present in store)");
        }
        let _ = writeln!(warn_out, "{line}");
    }
    Ok(())
}

/// Collect advisory warnings from all configured sources for one pin.
pub fn collect_warnings_for_pin(
    store: &Store,
    id: &str,
    version: &str,
    content_hash: Option<&str>,
) -> Result<Vec<AdvisoryWarning>> {
    let sources = load_sources(store)?;
    let trust = load_trust(store)?;
    let mut out = Vec::new();
    // main first among all sources for audit consistency
    let mut ordered = sources.sources.clone();
    ordered.sort_by_key(|s| if s.main { 0u8 } else { 1u8 });
    for src in &ordered {
        let Ok(base) = parse_uri(&src.uri) else {
            continue;
        };
        let advisories = read_advisories(&base)?;
        verify_advisories(&advisories, &trust)?;
        for adv in advisories.matches(id, version, content_hash) {
            out.push(AdvisoryWarning {
                source: src.name.clone(),
                advisory_id: adv.id.clone(),
                package_id: id.to_string(),
                version: version.to_string(),
                severity: adv.severity.as_str().to_string(),
                summary: adv.summary.clone(),
                url: adv.url.clone(),
                yanked: adv.yanked,
            });
        }
    }
    Ok(out)
}

/// Ensure package is in the store, fetching if needed; then warn on advisories.
pub fn ensure_package(
    store: &Store,
    id: &str,
    version: &str,
    mode: LookupMode,
) -> Result<StoredPackage> {
    let mut stderr = io::stderr();
    fetch_into_store(store, id, version, mode, &mut stderr)
}
