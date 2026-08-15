use std::io::Write;

use lar_store::Store;

use crate::advisories::Severity;
use crate::fetch::collect_warnings_for_pin;
use crate::sources::load_sources;
use crate::Error;
use crate::Result;

/// Scope for `lar audit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditScope {
    /// Pins from install records.
    Installed,
    /// Every package in the SxS store.
    Store,
}

/// One finding from audit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditFinding {
    pub package_id: String,
    pub version: String,
    pub content_hash: String,
    pub advisory_id: String,
    pub severity: String,
    pub yanked: bool,
    pub summary: String,
    pub url: String,
    pub source: String,
}

/// Run an advisory audit; writes human lines to `out`.
/// Returns findings; caller should exit non-zero if any high/critical or yanked.
pub fn audit(store: &Store, scope: AuditScope, out: &mut dyn Write) -> Result<Vec<AuditFinding>> {
    let _ = load_sources(store)?; // ensure config readable
    let mut pins: Vec<(String, String, String)> = Vec::new();

    match scope {
        AuditScope::Store => {
            for pkg in store.list()? {
                pins.push((pkg.id, pkg.version, pkg.content_hash));
            }
        }
        AuditScope::Installed => {
            let installs_root = &store.paths().installs;
            if installs_root.is_dir() {
                let entries = std::fs::read_dir(installs_root).map_err(|source| Error::Io {
                    path: installs_root.clone(),
                    source,
                })?;
                for entry in entries {
                    let entry = entry.map_err(|source| Error::Io {
                        path: installs_root.clone(),
                        source,
                    })?;
                    let meta = entry.path().join("install.toml");
                    if !meta.is_file() {
                        continue;
                    }
                    let text = std::fs::read_to_string(&meta).map_err(|source| Error::Io {
                        path: meta.clone(),
                        source,
                    })?;
                    // Minimal parse: reuse toml into a loose table
                    #[derive(serde::Deserialize)]
                    struct InstallPinFile {
                        #[serde(default)]
                        packages: Vec<InstallPinPkg>,
                    }
                    #[derive(serde::Deserialize)]
                    struct InstallPinPkg {
                        id: String,
                        version: String,
                        content_hash: String,
                    }
                    if let Ok(file) = toml::from_str::<InstallPinFile>(&text) {
                        for p in file.packages {
                            pins.push((p.id, p.version, p.content_hash));
                        }
                    }
                }
            }
        }
    }

    pins.sort();
    pins.dedup();

    let mut findings = Vec::new();
    for (id, version, hash) in pins {
        let warnings = collect_warnings_for_pin(store, &id, &version, Some(&hash))?;
        for w in warnings {
            let finding = AuditFinding {
                package_id: id.clone(),
                version: version.clone(),
                content_hash: hash.clone(),
                advisory_id: w.advisory_id,
                severity: w.severity,
                yanked: w.yanked,
                summary: w.summary,
                url: w.url,
                source: w.source,
            };
            let _ = writeln!(
                out,
                "{} {} [{}] {}{} — {} ({}){}",
                finding.package_id,
                finding.version,
                finding.severity,
                finding.advisory_id,
                if finding.yanked { " yanked" } else { "" },
                finding.summary,
                finding.source,
                if finding.url.is_empty() {
                    String::new()
                } else {
                    format!(" {}", finding.url)
                }
            );
            findings.push(finding);
        }
    }

    Ok(findings)
}

pub fn audit_should_fail(findings: &[AuditFinding]) -> bool {
    findings.iter().any(|f| {
        f.yanked
            || matches!(f.severity.as_str(), "high" | "critical")
            || f.severity == Severity::High.as_str()
            || f.severity == Severity::Critical.as_str()
    })
}
