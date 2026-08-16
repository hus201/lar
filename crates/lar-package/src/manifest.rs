use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path};

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

use crate::id::validate_package_id;
use crate::Error;

/// Current LAR package format version written by this crate.
pub const FORMAT_VERSION: u32 = 1;

/// Root package manifest (`package.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageManifest {
    pub package: PackageMeta,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<Entry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desktop: Option<Desktop>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<PlatformRequirements>,
}

/// Built-in host platform capability ids (MVP).
pub const PLATFORM_CAPABILITIES: &[&str] = &[
    "wayland",
    "x11",
    "vulkan",
    "opengl",
    "dbus",
    "dri",
    "systemd-user",
];

/// Optional `[platform]` table: host OS capabilities (not LAR packages).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformRequirements {
    /// Capabilities that must be present on the host.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    /// Capabilities that are nice to have; warn if missing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub optional: Vec<String>,
}

impl PlatformRequirements {
    pub fn is_empty(&self) -> bool {
        self.requires.is_empty() && self.optional.is_empty()
    }
}

/// `[package]` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageMeta {
    /// Package format version (currently `1`).
    pub format: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

/// Optional launchable binaries relative to `files/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    pub binaries: Vec<String>,
}

/// Optional desktop integration metadata (v1 allows empty).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Desktop {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<String>>,
}

pub(crate) fn validate_manifest(manifest: &PackageManifest) -> Result<(), Error> {
    if manifest.package.format != FORMAT_VERSION {
        return Err(Error::Validation(format!(
            "unsupported package.format {} (supported: {FORMAT_VERSION})",
            manifest.package.format
        )));
    }
    validate_package_id(&manifest.package.id)?;
    if manifest.package.name.trim().is_empty() {
        return Err(Error::Validation("package.name must not be empty".into()));
    }
    parse_semver(&manifest.package.version, "package.version")?;

    if let Some(hash) = &manifest.package.content_hash {
        if !hash.starts_with("blake3:") || hash.len() <= "blake3:".len() {
            return Err(Error::Validation(
                "package.content_hash must look like blake3:<hex>".into(),
            ));
        }
    }

    for (dep_id, dep_ver) in &manifest.dependencies {
        validate_package_id(dep_id).map_err(|err| match err {
            Error::InvalidPackageId { id, reason } => {
                Error::Validation(format!("dependency id `{id}` is invalid: {reason}"))
            }
            other => other,
        })?;
        parse_version_req(dep_ver, &format!("dependency `{dep_id}`"))?;
    }

    if let Some(entry) = &manifest.entry {
        if entry.binaries.is_empty() {
            return Err(Error::Validation(
                "entry.binaries must not be empty when [entry] is present".into(),
            ));
        }
        for binary in &entry.binaries {
            validate_relative_payload_path(binary, "entry.binaries")?;
        }
        if let Some(default) = &entry.default {
            validate_relative_payload_path(default, "entry.default")?;
            if !entry.binaries.iter().any(|b| b == default) {
                return Err(Error::Validation(format!(
                    "entry.default `{default}` must be listed in entry.binaries"
                )));
            }
        }
    }

    if let Some(platform) = &manifest.platform {
        validate_platform(platform)?;
    }

    Ok(())
}

fn validate_platform(platform: &PlatformRequirements) -> Result<(), Error> {
    validate_platform_list(&platform.requires, "platform.requires")?;
    validate_platform_list(&platform.optional, "platform.optional")?;
    let requires: std::collections::BTreeSet<_> = platform.requires.iter().collect();
    for cap in &platform.optional {
        if requires.contains(cap) {
            return Err(Error::Validation(format!(
                "platform capability `{cap}` cannot be both required and optional"
            )));
        }
    }
    Ok(())
}

fn validate_platform_list(list: &[String], label: &str) -> Result<(), Error> {
    let mut seen = std::collections::BTreeSet::new();
    for cap in list {
        if cap.trim().is_empty() {
            return Err(Error::Validation(format!(
                "{label} entries must not be empty"
            )));
        }
        if !PLATFORM_CAPABILITIES.contains(&cap.as_str()) {
            return Err(Error::Validation(format!(
                "unknown platform capability `{cap}` (supported: {})",
                PLATFORM_CAPABILITIES.join(", ")
            )));
        }
        if !seen.insert(cap.clone()) {
            return Err(Error::Validation(format!(
                "duplicate platform capability `{cap}` in {label}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn validate_entry_files(
    manifest: &PackageManifest,
    package_dir: &Path,
) -> Result<(), Error> {
    let Some(entry) = &manifest.entry else {
        return Ok(());
    };
    let files_dir = package_dir.join("files");
    for binary in &entry.binaries {
        let path = files_dir.join(binary);
        let meta = fs::symlink_metadata(&path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                Error::Validation(format!(
                    "entry binary `{binary}` not found at {}",
                    path.display()
                ))
            } else {
                Error::Io {
                    path: path.clone(),
                    source,
                }
            }
        })?;
        let file_type = meta.file_type();
        if file_type.is_symlink() {
            return Err(Error::Validation(format!(
                "entry binary `{binary}` must be a regular file, not a symlink"
            )));
        }
        if !file_type.is_file() {
            return Err(Error::Validation(format!(
                "entry binary `{binary}` must be a regular file"
            )));
        }
    }
    Ok(())
}

fn parse_semver(value: &str, label: &str) -> Result<Version, Error> {
    Version::parse(value).map_err(|err| Error::InvalidVersion {
        version: value.to_string(),
        reason: format!("{label}: {err}"),
    })
}

fn parse_version_req(value: &str, label: &str) -> Result<VersionReq, Error> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidVersion {
            version: value.to_string(),
            reason: format!("{label}: version requirement must not be empty"),
        });
    }
    let req = VersionReq::parse(trimmed).map_err(|err| Error::InvalidVersion {
        version: value.to_string(),
        reason: format!("{label}: {err}"),
    })?;
    if trimmed == "*" || req == VersionReq::STAR {
        return Err(Error::InvalidVersion {
            version: value.to_string(),
            reason: format!(
                "{label}: wildcard `*` is not allowed; use an exact version or a bounded range (e.g. ^1.0, ~1.2.3)"
            ),
        });
    }
    Ok(req)
}

fn validate_relative_payload_path(path: &str, label: &str) -> Result<(), Error> {
    if path.is_empty() {
        return Err(Error::Validation(format!("{label} path must not be empty")));
    }
    let path = Path::new(path);
    if path.is_absolute() {
        return Err(Error::Validation(format!(
            "{label} path must be relative to files/: `{path}`",
            path = path.display()
        )));
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {}
            _ => {
                return Err(Error::Validation(format!(
                    "{label} path must not contain `..` or prefixes: `{path}`",
                    path = path.display()
                )));
            }
        }
    }
    Ok(())
}

/// Normalize a path under `files/` to a relative string using `/`.
pub(crate) fn normalize_payload_rel_path(path: &Path) -> Result<String, Error> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                parts.push(
                    part.to_str()
                        .ok_or_else(|| Error::InvalidPath(path.to_path_buf()))?
                        .to_string(),
                );
            }
            Component::CurDir => {}
            _ => {
                return Err(Error::Validation(format!(
                    "invalid payload path: {}",
                    path.display()
                )));
            }
        }
    }
    if parts.is_empty() {
        return Err(Error::Validation("payload path must not be empty".into()));
    }
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use crate::parse_manifest;

    #[test]
    fn parses_valid_manifest() {
        let manifest = parse_manifest(
            r#"
[package]
format = 1
id = "org.example.editor"
name = "Example Editor"
version = "0.1.0"
description = "demo"

[dependencies]
"org.qt.qtbase" = "6.8.1"

[entry]
default = "bin/editor"
binaries = ["bin/editor"]
"#,
        )
        .unwrap();
        assert_eq!(manifest.package.format, 1);
        assert_eq!(manifest.package.id, "org.example.editor");
        assert_eq!(manifest.dependencies["org.qt.qtbase"], "6.8.1");
    }

    #[test]
    fn rejects_unsupported_format() {
        let err = parse_manifest(
            r#"
[package]
format = 99
id = "org.example.editor"
name = "Example Editor"
version = "0.1.0"
"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("unsupported package.format"));
    }

    #[test]
    fn rejects_type_field() {
        let err = parse_manifest(
            r#"
[package]
format = 1
id = "org.example.editor"
name = "Example Editor"
version = "0.1.0"
type = "application"
"#,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("type") || msg.contains("unknown"), "{msg}");
    }

    #[test]
    fn rejects_unknown_top_level() {
        let err = parse_manifest(
            r#"
[package]
format = 1
id = "org.example.editor"
name = "Example Editor"
version = "0.1.0"

[hooks]
after_install = "bin/setup"
"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("hooks") || err.to_string().contains("unknown"));
    }

    #[test]
    fn rejects_bad_version() {
        assert!(parse_manifest(
            r#"
[package]
format = 1
id = "org.example.editor"
name = "Example Editor"
version = "not-a-version"
"#,
        )
        .is_err());
    }

    #[test]
    fn accepts_dependency_version_req() {
        let manifest = parse_manifest(
            r#"
[package]
format = 1
id = "org.example.editor"
name = "Example Editor"
version = "0.1.0"

[dependencies]
"org.example.lib" = "^1.0"
"org.example.base" = "~2.1.0"
"org.example.util" = ">=1.0, <2"
"#,
        )
        .unwrap();
        assert_eq!(manifest.dependencies["org.example.lib"], "^1.0");
        assert_eq!(manifest.dependencies["org.example.base"], "~2.1.0");
    }

    #[test]
    fn accepts_platform_requirements() {
        let manifest = parse_manifest(
            r#"
[package]
format = 1
id = "org.example.app"
name = "App"
version = "0.1.0"

[platform]
requires = ["wayland", "dbus"]
optional = ["vulkan"]
"#,
        )
        .unwrap();
        let platform = manifest.platform.unwrap();
        assert_eq!(platform.requires, ["wayland", "dbus"]);
        assert_eq!(platform.optional, ["vulkan"]);
    }

    #[test]
    fn rejects_unknown_platform_capability() {
        let err = parse_manifest(
            r#"
[package]
format = 1
id = "org.example.app"
name = "App"
version = "0.1.0"

[platform]
requires = ["wayland", "magic-bus"]
"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("magic-bus"), "{err}");
    }

    #[test]
    fn rejects_platform_overlap() {
        let err = parse_manifest(
            r#"
[package]
format = 1
id = "org.example.app"
name = "App"
version = "0.1.0"

[platform]
requires = ["wayland"]
optional = ["wayland"]
"#,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("both required and optional"),
            "{err}"
        );
    }

    #[test]
    fn rejects_wildcard_dependency() {
        let err = parse_manifest(
            r#"
[package]
format = 1
id = "org.example.editor"
name = "Example Editor"
version = "0.1.0"

[dependencies]
"org.example.lib" = "*"
"#,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains('*') || err.to_string().contains("wildcard"),
            "{err}"
        );
    }
}
