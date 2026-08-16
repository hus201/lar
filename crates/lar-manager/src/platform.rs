//! Platform requirement checks for install / launch.

use std::io::{self, Write};

use lar_package::load_manifest;
use lar_platform::{check_host, collect_from_manifests, CheckReport, PlatformNeed};
use lar_store::Store;

use crate::record::InstallRecord;
use crate::Error;
use crate::Result;

/// Load manifests for every package pinned by an install (or lock-equivalent list).
pub fn manifests_for_packages(
    store: &Store,
    packages: &[(String, String)],
) -> Result<Vec<lar_package::PackageManifest>> {
    let mut out = Vec::with_capacity(packages.len());
    for (id, version) in packages {
        let stored = store.get(id, version)?.ok_or_else(|| Error::NotInStore {
            id: id.clone(),
            version: version.clone(),
        })?;
        out.push(load_manifest(&stored.path.join("package.toml"))?);
    }
    Ok(out)
}

/// Collect platform needs from an install record's package set.
pub fn need_for_record(store: &Store, record: &InstallRecord) -> Result<PlatformNeed> {
    let pairs: Vec<(String, String)> = record
        .packages
        .iter()
        .map(|p| (p.id.clone(), p.version.clone()))
        .collect();
    let manifests = manifests_for_packages(store, &pairs)?;
    let refs: Vec<_> = manifests.iter().collect();
    collect_from_manifests(&refs).map_err(|err| Error::Platform(err.to_string()))
}

/// Collect platform needs from a lockfile-shaped package list already in the store.
pub fn need_for_lock_packages(
    store: &Store,
    packages: &[lar_resolver::LockedPackage],
) -> Result<PlatformNeed> {
    let pairs: Vec<(String, String)> = packages
        .iter()
        .map(|p| (p.id.clone(), p.version.clone()))
        .collect();
    let manifests = manifests_for_packages(store, &pairs)?;
    let refs: Vec<_> = manifests.iter().collect();
    collect_from_manifests(&refs).map_err(|err| Error::Platform(err.to_string()))
}

/// Check host against needs: warn on optional gaps; Err on required gaps.
pub fn enforce_platform(need: &PlatformNeed) -> Result<CheckReport> {
    let report = check_host(need);
    report.emit_optional_warnings(&mut io::stderr());
    if !report.ok() {
        return Err(Error::Platform(report.required_error_message()));
    }
    Ok(report)
}

/// Enforce platform requirements for an install record.
pub fn enforce_for_record(store: &Store, record: &InstallRecord) -> Result<CheckReport> {
    let need = need_for_record(store, record)?;
    if need.is_empty() {
        return Ok(CheckReport::default());
    }
    enforce_platform(&need)
}

/// Build string lists for export metadata.
pub fn need_to_export_lists(need: &PlatformNeed) -> (Vec<String>, Vec<String>) {
    let requires = need
        .requires
        .iter()
        .map(|c| c.as_str().to_string())
        .collect();
    let optional = need
        .optional
        .iter()
        .map(|c| c.as_str().to_string())
        .collect();
    (requires, optional)
}

/// Shared helper used by tests to write warnings without failing.
#[allow(dead_code)]
pub fn write_optional_warnings(report: &CheckReport, out: &mut dyn Write) {
    report.emit_optional_warnings(out);
}
