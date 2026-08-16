use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::trust::{
    find_trusted_key, key_id_from_public, sign_content_hash, verify_content_hash, TrustFile,
};
use crate::Error;
use crate::Result;

pub const ADVISORIES_FORMAT: u32 = 1;

/// Repo-published vulnerability metadata (`advisories.toml`).
///
/// When the file is present on a source, `content_hash`, `key_id`, and `signature`
/// are required. Signature is Ed25519 over the UTF-8 `content_hash` string (same
/// shape as package index entries). `content_hash` is BLAKE3 over the canonical
/// `format` + `[[advisories]]` payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct AdvisoriesFile {
    #[serde(default = "default_format")]
    pub format: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub key_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub signature: String,
    #[serde(default)]
    pub advisories: Vec<Advisory>,
}

fn default_format() -> u32 {
    ADVISORIES_FORMAT
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Advisory {
    pub id: String,
    pub package_id: String,
    #[serde(default)]
    pub versions: Vec<String>,
    #[serde(default)]
    pub content_hashes: Vec<String>,
    pub severity: Severity,
    #[serde(default)]
    pub yanked: bool,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    pub fn is_elevated(self) -> bool {
        matches!(self, Self::High | Self::Critical)
    }
}

/// Payload covered by `content_hash` (format + advisories only).
#[derive(Debug, Serialize)]
struct AdvisoriesHashPayload<'a> {
    format: u32,
    advisories: &'a [Advisory],
}

impl AdvisoriesFile {
    pub fn validate(&self) -> Result<()> {
        if self.format != ADVISORIES_FORMAT {
            return Err(Error::InvalidAdvisories(format!(
                "unsupported format {} (supported: {ADVISORIES_FORMAT})",
                self.format
            )));
        }
        for adv in &self.advisories {
            if adv.id.is_empty() || adv.package_id.is_empty() {
                return Err(Error::InvalidAdvisories(
                    "advisory id and package_id must be non-empty".into(),
                ));
            }
            if adv.versions.is_empty() && adv.content_hashes.is_empty() {
                return Err(Error::InvalidAdvisories(format!(
                    "advisory {} must list versions and/or content_hashes",
                    adv.id
                )));
            }
        }
        Ok(())
    }

    /// Present published files must carry hash + signature fields.
    pub fn require_signature_fields(&self) -> Result<()> {
        if self.content_hash.is_empty() || self.key_id.is_empty() || self.signature.is_empty() {
            return Err(Error::InvalidAdvisories(
                "advisories.toml requires content_hash, key_id, and signature (run `lar repo index --sign-key`)"
                    .into(),
            ));
        }
        if !self.content_hash.starts_with("blake3:") || self.content_hash.len() <= "blake3:".len() {
            return Err(Error::InvalidAdvisories(
                "advisories content_hash must look like blake3:<hex>".into(),
            ));
        }
        Ok(())
    }

    /// True for the in-memory placeholder when `advisories.toml` is absent.
    pub fn is_absent_placeholder(&self) -> bool {
        self.content_hash.is_empty()
            && self.key_id.is_empty()
            && self.signature.is_empty()
            && self.advisories.is_empty()
    }

    pub fn matches(
        &self,
        package_id: &str,
        version: &str,
        content_hash: Option<&str>,
    ) -> Vec<&Advisory> {
        self.advisories
            .iter()
            .filter(|a| {
                if a.package_id != package_id {
                    return false;
                }
                let ver_ok = a.versions.is_empty() || a.versions.iter().any(|v| v == version);
                let hash_ok = a.content_hashes.is_empty()
                    || content_hash
                        .map(|h| a.content_hashes.iter().any(|c| c == h))
                        .unwrap_or(false);
                // If both lists non-empty, both must match; if one empty, the other gates.
                match (!a.versions.is_empty(), !a.content_hashes.is_empty()) {
                    (true, true) => ver_ok && hash_ok,
                    (true, false) => ver_ok,
                    (false, true) => hash_ok,
                    (false, false) => false,
                }
            })
            .collect()
    }
}

pub fn parse_advisories(text: &str) -> Result<AdvisoriesFile> {
    let file: AdvisoriesFile =
        toml::from_str(text).map_err(|err| Error::InvalidAdvisories(err.to_string()))?;
    file.validate()?;
    Ok(file)
}

/// Empty advisories when the file is absent (no signature required).
pub fn empty_advisories() -> AdvisoriesFile {
    AdvisoriesFile {
        format: ADVISORIES_FORMAT,
        content_hash: String::new(),
        key_id: String::new(),
        signature: String::new(),
        advisories: Vec::new(),
    }
}

/// BLAKE3 `content_hash` over the canonical advisories payload (`format` + entries).
pub fn compute_advisories_content_hash(file: &AdvisoriesFile) -> Result<String> {
    let payload = AdvisoriesHashPayload {
        format: file.format,
        advisories: &file.advisories,
    };
    let text = toml::to_string(&payload)
        .map_err(|err| Error::Other(format!("serialize advisories payload: {err}")))?;
    let digest = blake3::hash(text.as_bytes());
    Ok(format!("blake3:{}", digest.to_hex()))
}

/// Sign advisories with a secret key (same form as package index signing).
pub fn sign_advisories(mut file: AdvisoriesFile, secret_key: &str) -> Result<AdvisoriesFile> {
    file.validate()?;
    let public = {
        use ed25519_dalek::SigningKey;
        let raw = secret_key
            .strip_prefix("base64:")
            .ok_or_else(|| Error::InvalidSecretKey("secret must use base64: prefix".into()))?;
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, raw)
            .map_err(|err| Error::InvalidSecretKey(err.to_string()))?;
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
    file.content_hash = compute_advisories_content_hash(&file)?;
    file.key_id = key_id_from_public(&public)?;
    file.signature = sign_content_hash(secret_key, &file.content_hash)?;
    file.require_signature_fields()?;
    Ok(file)
}

/// Verify a published advisories file against the trust store.
pub fn verify_advisories(file: &AdvisoriesFile, trust: &TrustFile) -> Result<()> {
    if file.is_absent_placeholder() {
        return Ok(());
    }
    file.require_signature_fields()?;
    let expected = compute_advisories_content_hash(file)?;
    if expected != file.content_hash {
        return Err(Error::InvalidAdvisories(format!(
            "advisories content_hash mismatch (file has {}, recomputed {})",
            file.content_hash, expected
        )));
    }
    let key = find_trusted_key(trust, &file.key_id)
        .ok_or_else(|| Error::UntrustedKey(file.key_id.clone()))?;
    verify_content_hash(&key.public_key, &file.content_hash, &file.signature).map_err(|_| {
        Error::BadAdvisoriesSignature {
            key_id: file.key_id.clone(),
        }
    })?;
    Ok(())
}

/// Write `advisories.toml` into `dir`.
pub fn write_advisories(dir: &Path, file: &AdvisoriesFile) -> Result<PathBuf> {
    file.validate()?;
    file.require_signature_fields()?;
    let path = dir.join("advisories.toml");
    let text = toml::to_string_pretty(file)
        .map_err(|err| Error::Other(format!("serialize advisories.toml: {err}")))?;
    fs::write(&path, text).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

/// If `dir/advisories.toml` exists, parse, sign, and rewrite it.
pub fn sign_advisories_in_dir(dir: &Path, secret_key: &str) -> Result<Option<PathBuf>> {
    let path = dir.join("advisories.toml");
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    let file = parse_advisories(&text)?;
    let signed = sign_advisories(file, secret_key)?;
    Ok(Some(write_advisories(dir, &signed)?))
}
