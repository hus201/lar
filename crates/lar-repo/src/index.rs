use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::trust::{key_id_from_public, sign_message, verify_message};
use crate::Error;
use crate::Result;

/// Current index format (dependencies included in the signed pin payload).
pub const INDEX_FORMAT: u32 = 1;

/// Domain-separated prefix for index pin signatures.
const INDEX_PIN_SIGNING_V1: &str = "lar-index-pin-v1";

/// Published package index (`index.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PackageIndex {
    #[serde(default = "default_format")]
    pub format: u32,
    #[serde(default)]
    pub packages: Vec<IndexPackage>,
}

fn default_format() -> u32 {
    INDEX_FORMAT
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexPackage {
    pub id: String,
    pub version: String,
    pub content_hash: String,
    pub file: String,
    pub key_id: String,
    pub signature: String,
    /// Declared `[dependencies]` from the package manifest.
    ///
    /// Included in the signed pin payload so resolve can search without
    /// downloading `.lar` archives.
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}

impl PackageIndex {
    pub fn validate(&self) -> Result<()> {
        if self.format != INDEX_FORMAT {
            return Err(Error::InvalidIndex(format!(
                "unsupported format {} (supported: {INDEX_FORMAT})",
                self.format
            )));
        }
        for pkg in &self.packages {
            if pkg.id.is_empty() || pkg.version.is_empty() {
                return Err(Error::InvalidIndex(
                    "package id and version must be non-empty".into(),
                ));
            }
            validate_relative_file(&pkg.file)?;
            if !pkg.content_hash.starts_with("blake3:") {
                return Err(Error::InvalidIndex(format!(
                    "invalid content_hash for {} {}",
                    pkg.id, pkg.version
                )));
            }
            if pkg.key_id.is_empty() || pkg.signature.is_empty() {
                return Err(Error::InvalidIndex(format!(
                    "package {} {} missing key_id/signature",
                    pkg.id, pkg.version
                )));
            }
        }
        Ok(())
    }

    pub fn find(&self, id: &str, version: &str) -> Option<&IndexPackage> {
        self.packages
            .iter()
            .find(|p| p.id == id && p.version == version)
    }
}

/// Canonical message signed for an index pin.
///
/// Covers identity, archive location, content hash, and dependencies so resolve
/// can trust index metadata without downloading the archive.
pub fn index_pin_signing_message(pkg: &IndexPackage) -> String {
    let mut lines = vec![
        INDEX_PIN_SIGNING_V1.to_string(),
        format!("id={}", pkg.id),
        format!("version={}", pkg.version),
        format!("content_hash={}", pkg.content_hash),
        format!("file={}", pkg.file),
    ];
    for (dep_id, req) in &pkg.dependencies {
        lines.push(format!("dep={dep_id}\t{req}"));
    }
    lines.join("\n")
}

/// Sign an index pin.
pub fn sign_index_package(secret_key: &str, pkg: &IndexPackage) -> Result<String> {
    sign_message(secret_key, index_pin_signing_message(pkg).as_bytes())
}

/// Verify an index pin signature (covers [`index_pin_signing_message`]).
pub fn verify_index_package(public_key: &str, pkg: &IndexPackage) -> Result<()> {
    verify_message(
        public_key,
        index_pin_signing_message(pkg).as_bytes(),
        &pkg.signature,
    )
    .map_err(|_| Error::BadSignature {
        id: pkg.id.clone(),
        version: pkg.version.clone(),
    })
}

pub fn validate_relative_file(file: &str) -> Result<()> {
    let path = Path::new(file);
    if path.is_absolute() {
        return Err(Error::InvalidRelativePath(file.into()));
    }
    for c in path.components() {
        match c {
            Component::Normal(_) => {}
            Component::CurDir => {}
            _ => return Err(Error::InvalidRelativePath(file.into())),
        }
    }
    if file.is_empty() {
        return Err(Error::InvalidRelativePath(file.into()));
    }
    Ok(())
}

pub fn parse_index(text: &str) -> Result<PackageIndex> {
    let index: PackageIndex =
        toml::from_str(text).map_err(|err| Error::InvalidIndex(err.to_string()))?;
    index.validate()?;
    Ok(index)
}

/// Build a signed index from `.lar` files under `dir` (and `dir/packages`).
pub fn build_index(dir: &Path, secret_key: &str) -> Result<PackageIndex> {
    let public = {
        use ed25519_dalek::SigningKey;
        let bytes = {
            let raw = secret_key
                .strip_prefix("base64:")
                .ok_or_else(|| Error::InvalidSecretKey("secret must use base64: prefix".into()))?;
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, raw)
                .map_err(|err| Error::InvalidSecretKey(err.to_string()))?
        };
        if bytes.len() != 32 {
            return Err(Error::InvalidSecretKey(
                "Ed25519 secret key must be 32 seed bytes".into(),
            ));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        let signing = SigningKey::from_bytes(&arr);
        format!(
            "base64:{}",
            base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                signing.verifying_key().as_bytes()
            )
        )
    };
    let key_id = key_id_from_public(&public)?;

    let mut packages = Vec::new();
    let candidates = [dir.to_path_buf(), dir.join("packages")];
    for root in candidates {
        if !root.is_dir() {
            continue;
        }
        let entries = fs::read_dir(&root).map_err(|source| Error::Io {
            path: root.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| Error::Io {
                path: root.clone(),
                source,
            })?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("lar") {
                continue;
            }
            let archive = lar_package::inspect(&path)?;
            let rel = if root == dir {
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            } else {
                PathBuf::from("packages")
                    .join(path.file_name().unwrap())
                    .to_string_lossy()
                    .into_owned()
            };
            let content_hash = archive.index.content_hash.clone();
            let mut pkg = IndexPackage {
                id: archive.index.id,
                version: archive.index.version,
                content_hash,
                file: rel,
                key_id: key_id.clone(),
                signature: String::new(),
                dependencies: archive.manifest.dependencies,
            };
            pkg.signature = sign_index_package(secret_key, &pkg)?;
            packages.push(pkg);
        }
    }
    packages.sort_by(|a, b| (&a.id, &a.version).cmp(&(&b.id, &b.version)));
    let index = PackageIndex {
        format: INDEX_FORMAT,
        packages,
    };
    index.validate()?;
    Ok(index)
}

/// Write index.toml into `dir`.
pub fn write_index(dir: &Path, index: &PackageIndex) -> Result<PathBuf> {
    index.validate()?;
    let path = dir.join("index.toml");
    let text = toml::to_string_pretty(index)
        .map_err(|err| Error::Other(format!("serialize index.toml: {err}")))?;
    fs::write(&path, text).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}
