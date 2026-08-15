use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Error;
use crate::Result;

/// Current lockfile format version.
pub const LOCKFILE_FORMAT: u32 = 1;

/// Deterministic resolution result written as `lar.lock`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lockfile {
    pub format: u32,
    pub root: LockRoot,
    #[serde(default)]
    pub packages: Vec<LockedPackage>,
}

/// Root package identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockRoot {
    pub id: String,
    pub version: String,
}

/// One package in the resolved graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedPackage {
    pub id: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, String>,
}

impl Lockfile {
    pub fn validate(&self) -> Result<()> {
        if self.format != LOCKFILE_FORMAT {
            return Err(Error::InvalidLockfile(format!(
                "unsupported format {} (supported: {LOCKFILE_FORMAT})",
                self.format
            )));
        }
        if self.root.id.is_empty() || self.root.version.is_empty() {
            return Err(Error::InvalidLockfile(
                "root id and version must be non-empty".into(),
            ));
        }
        let mut seen = BTreeMap::new();
        for pkg in &self.packages {
            if pkg.id.is_empty() || pkg.version.is_empty() {
                return Err(Error::InvalidLockfile(
                    "package id and version must be non-empty".into(),
                ));
            }
            let is_root = pkg.id == self.root.id && pkg.version == self.root.version;
            if !is_root {
                match &pkg.content_hash {
                    Some(hash) if hash.starts_with("blake3:") && hash.len() > "blake3:".len() => {}
                    Some(_) => {
                        return Err(Error::InvalidLockfile(format!(
                            "package {} {} content_hash must look like blake3:<hex>",
                            pkg.id, pkg.version
                        )));
                    }
                    None => {
                        return Err(Error::InvalidLockfile(format!(
                            "package {} {} missing required content_hash",
                            pkg.id, pkg.version
                        )));
                    }
                }
            }
            let key = (pkg.id.clone(), pkg.version.clone());
            if seen.insert(key, ()).is_some() {
                return Err(Error::InvalidLockfile(format!(
                    "duplicate package entry {} {}",
                    pkg.id, pkg.version
                )));
            }
        }
        if !self
            .packages
            .iter()
            .any(|p| p.id == self.root.id && p.version == self.root.version)
        {
            return Err(Error::InvalidLockfile(format!(
                "root {} {} missing from packages",
                self.root.id, self.root.version
            )));
        }
        Ok(())
    }
}

/// Parse a lockfile from TOML text.
pub fn parse_lockfile(text: &str) -> Result<Lockfile> {
    let lock: Lockfile =
        toml::from_str(text).map_err(|err| Error::InvalidLockfile(err.to_string()))?;
    lock.validate()?;
    Ok(lock)
}

/// Load a lockfile from disk.
pub fn load_lockfile(path: &Path) -> Result<Lockfile> {
    let text = fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse_lockfile(&text)
}

/// Serialize and write a lockfile atomically (temp + rename).
pub fn write_lockfile(path: &Path, lock: &Lockfile) -> Result<()> {
    lock.validate()?;
    let text = toml::to_string_pretty(lock)
        .map_err(|err| Error::Other(format!("serialize lockfile: {err}")))?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
    }
    let tmp = path.with_extension("lock.tmp");
    fs::write(&tmp, text.as_bytes()).map_err(|source| Error::Io {
        path: tmp.clone(),
        source,
    })?;
    fs::rename(&tmp, path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

/// Default lockfile path beside a root `package.toml`.
pub fn lockfile_path_for_manifest(manifest_path: &Path) -> Result<std::path::PathBuf> {
    let dir = lar_package::package_dir_from_manifest(manifest_path)?;
    Ok(dir.join("lar.lock"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_root_without_content_hash() {
        let lock = Lockfile {
            format: LOCKFILE_FORMAT,
            root: LockRoot {
                id: "org.example.app".into(),
                version: "0.1.0".into(),
            },
            packages: vec![
                LockedPackage {
                    id: "org.example.app".into(),
                    version: "0.1.0".into(),
                    content_hash: None,
                    dependencies: BTreeMap::from([("org.example.lib".into(), "1.0.0".into())]),
                },
                LockedPackage {
                    id: "org.example.lib".into(),
                    version: "1.0.0".into(),
                    content_hash: None,
                    dependencies: BTreeMap::new(),
                },
            ],
        };
        let err = lock.validate().unwrap_err();
        assert!(
            matches!(err, Error::InvalidLockfile(ref msg) if msg.contains("missing required content_hash")),
            "{err}"
        );
    }

    #[test]
    fn accepts_root_without_content_hash() {
        let lock = Lockfile {
            format: LOCKFILE_FORMAT,
            root: LockRoot {
                id: "org.example.app".into(),
                version: "0.1.0".into(),
            },
            packages: vec![LockedPackage {
                id: "org.example.app".into(),
                version: "0.1.0".into(),
                content_hash: None,
                dependencies: BTreeMap::new(),
            }],
        };
        lock.validate().unwrap();
    }
}
