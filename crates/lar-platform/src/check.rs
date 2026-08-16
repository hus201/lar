use std::collections::BTreeSet;

use lar_package::{PackageManifest, PlatformRequirements};

use crate::probe::{parse_capability, probe_with, Capability, ProbeEnv, ProbeResult};
use crate::Error;
use crate::Result;

/// Aggregated platform needs from one or more manifests.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlatformNeed {
    pub requires: BTreeSet<Capability>,
    pub optional: BTreeSet<Capability>,
}

impl PlatformNeed {
    pub fn is_empty(&self) -> bool {
        self.requires.is_empty() && self.optional.is_empty()
    }

    pub fn merge_requirements(&mut self, reqs: &PlatformRequirements) -> Result<()> {
        for id in &reqs.requires {
            let cap = parse_capability(id)?;
            self.optional.remove(&cap);
            self.requires.insert(cap);
        }
        for id in &reqs.optional {
            let cap = parse_capability(id)?;
            if !self.requires.contains(&cap) {
                self.optional.insert(cap);
            }
        }
        Ok(())
    }
}

/// Union `[platform]` from many manifests (root + dependencies).
pub fn collect_from_manifests(manifests: &[&PackageManifest]) -> Result<PlatformNeed> {
    let mut need = PlatformNeed::default();
    for manifest in manifests {
        if let Some(ref platform) = manifest.platform {
            need.merge_requirements(platform)?;
        }
    }
    Ok(need)
}

/// Build needs from explicit require/optional string lists (e.g. export metadata).
pub fn collect_platform_need(requires: &[String], optional: &[String]) -> Result<PlatformNeed> {
    let mut need = PlatformNeed::default();
    need.merge_requirements(&PlatformRequirements {
        requires: requires.to_vec(),
        optional: optional.to_vec(),
    })?;
    Ok(need)
}

/// Result of checking a [`PlatformNeed`] against the host.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CheckReport {
    pub satisfied: Vec<ProbeResult>,
    pub missing_required: Vec<ProbeResult>,
    pub missing_optional: Vec<ProbeResult>,
}

impl CheckReport {
    pub fn ok(&self) -> bool {
        self.missing_required.is_empty()
    }

    pub fn emit_optional_warnings(&self, out: &mut dyn std::io::Write) {
        for probe in &self.missing_optional {
            let _ = writeln!(
                out,
                "warning: optional platform capability `{}` missing ({})",
                probe.capability, probe.detail
            );
        }
    }

    pub fn required_error_message(&self) -> String {
        let list = self
            .missing_required
            .iter()
            .map(|p| format!("{} ({})", p.capability, p.detail))
            .collect::<Vec<_>>()
            .join("; ");
        format!("missing required platform capabilities: {list}")
    }
}

/// Check needs against the current process host environment.
pub fn check_host(need: &PlatformNeed) -> CheckReport {
    check_host_with(need, &ProbeEnv::from_process())
}

/// Check needs with an explicit probe environment.
pub fn check_host_with(need: &PlatformNeed, env: &ProbeEnv) -> CheckReport {
    let mut report = CheckReport::default();
    for cap in &need.requires {
        let result = probe_with(*cap, env);
        if result.present {
            report.satisfied.push(result);
        } else {
            report.missing_required.push(result);
        }
    }
    for cap in &need.optional {
        let result = probe_with(*cap, env);
        if result.present {
            report.satisfied.push(result);
        } else {
            report.missing_optional.push(result);
        }
    }
    report
}

/// Convenience: check and return Err if required caps are missing.
pub fn ensure_host(need: &PlatformNeed) -> Result<CheckReport> {
    let report = check_host(need);
    if report.ok() {
        Ok(report)
    } else {
        Err(Error::RequirementsNotMet(report.required_error_message()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lar_package::parse_manifest;

    #[test]
    fn collect_unions_and_promotes_optional_to_required() {
        let a = parse_manifest(
            r#"
[package]
format = 1
id = "org.example.a"
name = "A"
version = "1.0.0"

[platform]
requires = ["wayland"]
optional = ["vulkan"]
"#,
        )
        .unwrap();
        let b = parse_manifest(
            r#"
[package]
format = 1
id = "org.example.b"
name = "B"
version = "1.0.0"

[platform]
requires = ["vulkan"]
optional = ["dbus"]
"#,
        )
        .unwrap();
        let need = collect_from_manifests(&[&a, &b]).unwrap();
        assert!(need.requires.contains(&Capability::Wayland));
        assert!(need.requires.contains(&Capability::Vulkan));
        assert!(need.optional.contains(&Capability::Dbus));
        assert!(!need.optional.contains(&Capability::Vulkan));
    }

    #[test]
    fn check_host_with_override() {
        let need = PlatformNeed {
            requires: [Capability::Wayland].into_iter().collect(),
            optional: [Capability::X11].into_iter().collect(),
        };
        let env = ProbeEnv {
            wayland_display: None,
            display: None,
            dbus_session_bus_address: None,
            xdg_runtime_dir: None,
            uid: 0,
            ld_library_path: None,
            dri_dir: "/tmp".into(),
            run_systemd_system: "/tmp".into(),
            override_spec: Some("missing=wayland,present=x11".into()),
        };
        let report = check_host_with(&need, &env);
        assert!(!report.ok());
        assert_eq!(report.missing_required.len(), 1);
        assert!(report.missing_optional.is_empty()); // x11 forced present, so optional satisfied
    }
}
