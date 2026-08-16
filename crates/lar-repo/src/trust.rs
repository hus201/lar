use std::fs;

use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

use crate::Error;
use crate::Result;

pub const TRUST_FORMAT: u32 = 1;

/// Trusted publisher keys (`trust.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TrustFile {
    #[serde(default = "default_format")]
    pub format: u32,
    #[serde(default)]
    pub keys: Vec<TrustedKey>,
}

fn default_format() -> u32 {
    TRUST_FORMAT
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustedKey {
    pub id: String,
    pub public_key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
}

impl TrustFile {
    pub fn validate(&self) -> Result<()> {
        if self.format != TRUST_FORMAT {
            return Err(Error::InvalidTrust(format!(
                "unsupported format {} (supported: {TRUST_FORMAT})",
                self.format
            )));
        }
        let mut ids = std::collections::BTreeSet::new();
        for key in &self.keys {
            if !ids.insert(key.id.clone()) {
                return Err(Error::InvalidTrust(format!(
                    "duplicate key id `{}`",
                    key.id
                )));
            }
            parse_public_key(&key.public_key)?;
            let expected = key_id_from_public(&key.public_key)?;
            if key.id != expected {
                return Err(Error::InvalidTrust(format!(
                    "key id `{}` does not match public key (expected `{expected}`)",
                    key.id
                )));
            }
        }
        Ok(())
    }
}

/// Load trust.toml (missing → empty).
pub fn load_trust(store: &lar_store::Store) -> Result<TrustFile> {
    let path = store.paths().trust_toml();
    if !path.is_file() {
        return Ok(TrustFile {
            format: TRUST_FORMAT,
            keys: Vec::new(),
        });
    }
    let text = fs::read_to_string(&path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    let file: TrustFile =
        toml::from_str(&text).map_err(|err| Error::InvalidTrust(err.to_string()))?;
    file.validate()?;
    Ok(file)
}

pub fn save_trust(store: &lar_store::Store, file: &TrustFile) -> Result<()> {
    file.validate()?;
    let path = store.paths().trust_toml();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let text = toml::to_string_pretty(file)
        .map_err(|err| Error::Other(format!("serialize trust.toml: {err}")))?;
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text).map_err(|source| Error::Io {
        path: tmp.clone(),
        source,
    })?;
    fs::rename(&tmp, &path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    Ok(())
}

pub fn trust_add(
    store: &lar_store::Store,
    public_key: &str,
    comment: impl Into<String>,
) -> Result<TrustedKey> {
    let mut file = load_trust(store)?;
    let id = key_id_from_public(public_key)?;
    if file.keys.iter().any(|k| k.id == id) {
        return Err(Error::KeyExists(id));
    }
    let entry = TrustedKey {
        id,
        public_key: normalize_public_key(public_key)?,
        comment: comment.into(),
    };
    file.keys.push(entry.clone());
    file.keys.sort_by(|a, b| a.id.cmp(&b.id));
    save_trust(store, &file)?;
    Ok(entry)
}

pub fn trust_remove(store: &lar_store::Store, key_id: &str) -> Result<TrustedKey> {
    let mut file = load_trust(store)?;
    let idx = file
        .keys
        .iter()
        .position(|k| k.id == key_id)
        .ok_or_else(|| Error::KeyNotFound(key_id.to_string()))?;
    let entry = file.keys.remove(idx);
    save_trust(store, &file)?;
    Ok(entry)
}

pub fn find_trusted_key<'a>(file: &'a TrustFile, key_id: &str) -> Option<&'a TrustedKey> {
    file.keys.iter().find(|k| k.id == key_id)
}

/// True when `want` matches `key_id` (full `ed25519:…` or bare hex, case-insensitive).
pub fn fingerprint_matches(key_id: &str, want: &str) -> bool {
    let key_id = key_id.trim();
    let want = want.trim();
    if want.eq_ignore_ascii_case(key_id) {
        return true;
    }
    let kid_hex = key_id
        .strip_prefix("ed25519:")
        .or_else(|| key_id.strip_prefix("ED25519:"))
        .unwrap_or(key_id);
    let want_hex = want
        .strip_prefix("ed25519:")
        .or_else(|| want.strip_prefix("ED25519:"))
        .unwrap_or(want);
    kid_hex.eq_ignore_ascii_case(want_hex)
}

/// Load a publisher pubkey from `--pubkey` override or `{uri}/ed25519.pub`.
///
/// Returns `(public_key, key_id)`.
pub fn load_source_pubkey(uri: &str, pubkey_override: Option<&str>) -> Result<(String, String)> {
    let raw = if let Some(pk) = pubkey_override {
        let trimmed = pk.trim();
        if trimmed.is_empty() {
            return Err(Error::InvalidKey("public key is empty".into()));
        }
        trimmed.to_string()
    } else {
        let base = crate::transport::parse_uri(uri)?;
        crate::transport::read_repo_pubkey(&base)?
    };
    let id = key_id_from_public(&raw)?;
    Ok((raw, id))
}

/// Whether `key_id` is already in the trust store.
pub fn is_key_trusted(store: &lar_store::Store, key_id: &str) -> Result<bool> {
    let file = load_trust(store)?;
    Ok(find_trusted_key(&file, key_id).is_some())
}

/// Generate an Ed25519 keypair; returns (public_key base64 form, secret_key base64 form, key_id).
pub fn keygen() -> Result<(String, String, String)> {
    let signing = SigningKey::generate(&mut OsRng);
    let verifying = signing.verifying_key();
    let public = format!(
        "base64:{}",
        base64::engine::general_purpose::STANDARD.encode(verifying.as_bytes())
    );
    let secret = format!(
        "base64:{}",
        base64::engine::general_purpose::STANDARD.encode(signing.to_bytes())
    );
    let id = key_id_from_public(&public)?;
    Ok((public, secret, id))
}

/// Sign a content_hash string with a secret key (`base64:…` of 32 seed bytes).
pub fn sign_content_hash(secret_key: &str, content_hash: &str) -> Result<String> {
    sign_message(secret_key, content_hash.as_bytes())
}

/// Verify signature over content_hash with a public key string.
pub fn verify_content_hash(public_key: &str, content_hash: &str, signature: &str) -> Result<()> {
    verify_message(public_key, content_hash.as_bytes(), signature)
}

/// Sign arbitrary message bytes (Ed25519); returns `base64:…` signature.
pub fn sign_message(secret_key: &str, message: &[u8]) -> Result<String> {
    let signing = parse_secret_key(secret_key)?;
    let sig = signing.sign(message);
    Ok(format!(
        "base64:{}",
        base64::engine::general_purpose::STANDARD.encode(sig.to_bytes())
    ))
}

/// Verify an Ed25519 signature over message bytes.
pub fn verify_message(public_key: &str, message: &[u8], signature: &str) -> Result<()> {
    let verifying = parse_public_key(public_key)?;
    let sig = parse_signature(signature)?;
    verifying
        .verify(message, &sig)
        .map_err(|_| Error::BadSignature {
            id: String::new(),
            version: String::new(),
        })?;
    Ok(())
}

pub fn key_id_from_public(public_key: &str) -> Result<String> {
    let verifying = parse_public_key(public_key)?;
    Ok(format!("ed25519:{}", hex::encode(verifying.as_bytes())))
}

fn normalize_public_key(public_key: &str) -> Result<String> {
    let verifying = parse_public_key(public_key)?;
    Ok(format!(
        "base64:{}",
        base64::engine::general_purpose::STANDARD.encode(verifying.as_bytes())
    ))
}

fn parse_public_key(public_key: &str) -> Result<VerifyingKey> {
    let bytes = decode_key_bytes(public_key, "public")?;
    if bytes.len() != 32 {
        return Err(Error::InvalidKey(
            "Ed25519 public key must be 32 bytes".into(),
        ));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    VerifyingKey::from_bytes(&arr).map_err(|err| Error::InvalidKey(err.to_string()))
}

fn parse_secret_key(secret_key: &str) -> Result<SigningKey> {
    let bytes = decode_key_bytes(secret_key, "secret")?;
    if bytes.len() != 32 {
        return Err(Error::InvalidSecretKey(
            "Ed25519 secret key must be 32 seed bytes".into(),
        ));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(SigningKey::from_bytes(&arr))
}

fn parse_signature(signature: &str) -> Result<Signature> {
    let bytes = decode_key_bytes(signature, "signature")?;
    if bytes.len() != 64 {
        return Err(Error::Other("Ed25519 signature must be 64 bytes".into()));
    }
    let mut arr = [0u8; 64];
    arr.copy_from_slice(&bytes);
    Ok(Signature::from_bytes(&arr))
}

fn decode_key_bytes(value: &str, kind: &str) -> Result<Vec<u8>> {
    let raw = value
        .strip_prefix("base64:")
        .ok_or_else(|| Error::InvalidKey(format!("{kind} must use base64: prefix")))?;
    base64::engine::general_purpose::STANDARD
        .decode(raw)
        .map_err(|err| Error::InvalidKey(format!("invalid {kind} base64: {err}")))
}

/// Minimal hex encode without extra dep — wait, I used hex::encode. Add hex crate or implement.
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_matches_accepts_hex_or_prefixed() {
        let id = "ed25519:aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899";
        assert!(fingerprint_matches(id, id));
        assert!(fingerprint_matches(
            id,
            "AABBCCDDEEFF00112233445566778899AABBCCDDEEFF00112233445566778899"
        ));
        assert!(!fingerprint_matches(
            id,
            "ed25519:0000000000000000000000000000000000000000000000000000000000000000"
        ));
    }
}
