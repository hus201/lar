use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};

use lar_package::{inspect, load_manifest, PackageManifest};
use lar_store::{Store, StoredPackage};

use crate::advisories::{verify_advisories, AdvisoriesFile};
use crate::index::{IndexPackage, PackageIndex};
use crate::sources::{load_sources, ordered_sources, SourceEntry};
use crate::transport::{fetch_blob, parse_uri, read_advisories, read_index, SourceBase};
use crate::trust::{find_trusted_key, load_trust, verify_content_hash};
use crate::Error;
use crate::Result;

/// Package metadata for dependency resolution without committing to the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvePackage {
    pub id: String,
    pub version: String,
    pub content_hash: String,
    pub dependencies: BTreeMap<String, String>,
}

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

/// Load package metadata for resolution without adding to the store.
///
/// Store hits are read in place. Remote pins use dependency metadata from
/// format 2+ indexes (no `.lar` download). Legacy format 1 indexes fall back to
/// downloading and inspecting the archive, then discarding the temp blob.
pub fn load_package_for_resolve(
    store: &Store,
    id: &str,
    version: &str,
    warn_out: &mut dyn Write,
) -> Result<ResolvePackage> {
    if let Some(existing) = store.get(id, version)? {
        emit_store_hit_warnings(store, id, version, Some(&existing.content_hash), warn_out)?;
        let manifest = load_manifest(&existing.path.join("package.toml"))?;
        return resolve_package_from_manifest(id, version, &existing.content_hash, manifest);
    }

    let sources = load_sources(store)?;
    let mut last_miss = None;
    for src in ordered_sources(&sources) {
        match peek_from_source(store, src, id, version, warn_out) {
            Ok(pkg) => return Ok(pkg),
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

/// Fetch `id@version` into the store from configured sources.
///
/// When multiple sources publish the same pin, the highest-priority source
/// (earliest in `sources.toml`) wins. Contents are never merged across sources.
pub fn fetch_into_store(
    store: &Store,
    id: &str,
    version: &str,
    warn_out: &mut dyn Write,
) -> Result<StoredPackage> {
    if let Some(existing) = store.get(id, version)? {
        emit_store_hit_warnings(store, id, version, Some(&existing.content_hash), warn_out)?;
        return Ok(existing);
    }

    let sources = load_sources(store)?;
    let ordered = ordered_sources(&sources);

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

fn resolve_package_from_manifest(
    id: &str,
    version: &str,
    content_hash: &str,
    manifest: PackageManifest,
) -> Result<ResolvePackage> {
    if manifest.package.id != id || manifest.package.version != version {
        return Err(Error::Other(format!(
            "package metadata has id/version {} {}, expected {id} {version}",
            manifest.package.id, manifest.package.version
        )));
    }
    Ok(ResolvePackage {
        id: id.to_string(),
        version: version.to_string(),
        content_hash: content_hash.to_string(),
        dependencies: manifest.dependencies,
    })
}

fn peek_from_source(
    store: &Store,
    src: &SourceEntry,
    id: &str,
    version: &str,
    warn_out: &mut dyn Write,
) -> Result<ResolvePackage> {
    let (base, index, pkg) = locate_verified_pin(store, src, id, version, warn_out)?;
    if index.has_resolve_metadata() {
        return Ok(ResolvePackage {
            id: id.to_string(),
            version: version.to_string(),
            content_hash: pkg.content_hash.clone(),
            dependencies: pkg.dependencies.clone(),
        });
    }

    // Format 1 indexes lack dependency metadata — inspect the archive once.
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
        resolve_package_from_manifest(id, version, &pkg.content_hash, archive.manifest)
    })();
    let _ = fs::remove_file(&tmp);
    result
}

fn fetch_from_source(
    store: &Store,
    src: &SourceEntry,
    id: &str,
    version: &str,
    warn_out: &mut dyn Write,
) -> Result<StoredPackage> {
    let (base, _index, pkg) = locate_verified_pin(store, src, id, version, warn_out)?;
    let tmp = fetch_blob(&base, &pkg.file)?;
    let index_hash = pkg.content_hash.clone();
    let result = (|| {
        let archive = inspect(&tmp)?;
        if archive.index.content_hash != index_hash {
            return Err(Error::HashMismatch {
                id: id.to_string(),
                version: version.to_string(),
                index: index_hash.clone(),
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
                if stored.content_hash != index_hash {
                    return Err(Error::HashMismatch {
                        id: id.to_string(),
                        version: version.to_string(),
                        index: index_hash.clone(),
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

/// Find `id@version` in `src`, check advisories, and verify the index signature.
fn locate_verified_pin(
    store: &Store,
    src: &SourceEntry,
    id: &str,
    version: &str,
    warn_out: &mut dyn Write,
) -> Result<(SourceBase, PackageIndex, IndexPackage)> {
    let base = parse_uri(&src.uri)?;
    let index = read_index(&base)?;
    let trust = load_trust(store)?;
    let advisories = read_advisories(&base)?;
    verify_advisories(&advisories, &trust)?;
    let pkg = index
        .find(id, version)
        .cloned()
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

    let key = find_trusted_key(&trust, &pkg.key_id)
        .ok_or_else(|| Error::UntrustedKey(pkg.key_id.clone()))?;
    verify_content_hash(&key.public_key, &pkg.content_hash, &pkg.signature).map_err(|_| {
        Error::BadSignature {
            id: id.to_string(),
            version: version.to_string(),
        }
    })?;

    Ok((base, index, pkg))
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
    for src in &sources.sources {
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
pub fn ensure_package(store: &Store, id: &str, version: &str) -> Result<StoredPackage> {
    let mut stderr = io::stderr();
    fetch_into_store(store, id, version, &mut stderr)
}
