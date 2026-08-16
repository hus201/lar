//! Publisher helpers for local package-source directories.

use std::fs;
use std::path::{Path, PathBuf};

use lar_package::inspect;

use crate::advisories::{
    compute_advisories_content_hash, parse_advisories, sign_advisories_in_dir, verify_advisories,
};
use crate::index::{build_index, parse_index, write_index, PackageIndex, INDEX_FORMAT};
use crate::trust::{key_id_from_public, verify_content_hash, TrustFile, TrustedKey};
use crate::Error;
use crate::Result;

/// Result of validating a local package source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidateReport {
    pub packages: usize,
    pub advisories: usize,
}

fn public_from_secret(secret_key: &str) -> Result<String> {
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
    Ok(format!(
        "base64:{}",
        base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            signing.verifying_key().as_bytes()
        )
    ))
}

fn package_filename(id: &str, version: &str) -> String {
    format!("{id}-{version}.lar")
}

fn rebuild_and_sign(dir: &Path, secret_key: &str) -> Result<(PackageIndex, Option<PathBuf>)> {
    let index = build_index(dir, secret_key)?;
    write_index(dir, &index)?;
    let adv = sign_advisories_in_dir(dir, secret_key)?;
    Ok((index, adv))
}

/// Create `packages/` and a signed empty `index.toml` under `dir`.
pub fn init_repo(dir: &Path, secret_key: &str) -> Result<PathBuf> {
    let packages = dir.join("packages");
    fs::create_dir_all(&packages).map_err(|source| Error::Io {
        path: packages.clone(),
        source,
    })?;

    let index_path = dir.join("index.toml");
    if index_path.is_file() {
        return Err(Error::Other(format!(
            "repo already initialized at {}",
            dir.display()
        )));
    }

    // Ensure the secret key is valid (and derive key material) before writing.
    let _public = public_from_secret(secret_key)?;
    let index = PackageIndex {
        format: INDEX_FORMAT,
        packages: Vec::new(),
    };
    write_index(dir, &index)
}

/// Copy a `.lar` into `dir/packages/` and rebuild+sign the index (and advisories).
pub fn publish_package(
    dir: &Path,
    lar_path: &Path,
    secret_key: &str,
) -> Result<(IndexPackageInfo, PackageIndex)> {
    let archive = inspect(lar_path)?;
    let id = archive.index.id.clone();
    let version = archive.index.version.clone();
    let dest_name = package_filename(&id, &version);
    let packages = dir.join("packages");
    fs::create_dir_all(&packages).map_err(|source| Error::Io {
        path: packages.clone(),
        source,
    })?;
    let dest = packages.join(&dest_name);
    fs::copy(lar_path, &dest).map_err(|source| Error::Io {
        path: dest.clone(),
        source,
    })?;

    let (index, _) = rebuild_and_sign(dir, secret_key)?;
    let info = IndexPackageInfo {
        id,
        version,
        file: format!("packages/{dest_name}"),
    };
    Ok((info, index))
}

/// Published package location after `publish_package`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexPackageInfo {
    pub id: String,
    pub version: String,
    pub file: String,
}

/// Remove the `.lar` for `id`/`version` and rebuild+sign the index.
pub fn unpublish_package(
    dir: &Path,
    package_id: &str,
    version: &str,
    secret_key: &str,
) -> Result<PackageIndex> {
    let index_path = dir.join("index.toml");
    let mut removed = false;

    if index_path.is_file() {
        let text = fs::read_to_string(&index_path).map_err(|source| Error::Io {
            path: index_path.clone(),
            source,
        })?;
        let index = parse_index(&text)?;
        if let Some(pkg) = index.find(package_id, version) {
            let path = dir.join(&pkg.file);
            if path.is_file() {
                fs::remove_file(&path).map_err(|source| Error::Io {
                    path: path.clone(),
                    source,
                })?;
                removed = true;
            }
        }
    }

    // Also remove canonical packages/ name and root-level name if present.
    let candidates = [
        dir.join("packages")
            .join(package_filename(package_id, version)),
        dir.join(package_filename(package_id, version)),
    ];
    for path in candidates {
        if path.is_file() {
            fs::remove_file(&path).map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
            removed = true;
        }
    }

    if !removed {
        return Err(Error::PackageNotFound {
            id: package_id.into(),
            version: version.into(),
        });
    }

    let (index, _) = rebuild_and_sign(dir, secret_key)?;
    Ok(index)
}

fn trust_from_pubkey(public_key: &str) -> Result<TrustFile> {
    let id = key_id_from_public(public_key)?;
    Ok(TrustFile {
        format: crate::trust::TRUST_FORMAT,
        keys: vec![TrustedKey {
            id,
            public_key: public_key.to_string(),
            comment: String::new(),
        }],
    })
}

/// Validate layout, package hashes, and signatures for a local source tree.
///
/// When `public_key` is `Some`, package and advisories signatures are verified
/// against that key. When `None`, hashes and layout are checked but signatures
/// are only checked for non-empty fields (no cryptographic verify).
pub fn validate_repo(dir: &Path, public_key: Option<&str>) -> Result<ValidateReport> {
    let index_path = dir.join("index.toml");
    if !index_path.is_file() {
        return Err(Error::InvalidIndex(format!(
            "missing index.toml under {}",
            dir.display()
        )));
    }
    let text = fs::read_to_string(&index_path).map_err(|source| Error::Io {
        path: index_path.clone(),
        source,
    })?;
    let index = parse_index(&text)?;

    let trust = public_key.map(trust_from_pubkey).transpose()?;

    for pkg in &index.packages {
        let path = dir.join(&pkg.file);
        if !path.is_file() {
            return Err(Error::Other(format!(
                "index lists {} {} but file {} is missing",
                pkg.id, pkg.version, pkg.file
            )));
        }
        let archive = inspect(&path)?;
        if archive.index.id != pkg.id || archive.index.version != pkg.version {
            return Err(Error::Other(format!(
                "archive at {} is {} {}, index says {} {}",
                pkg.file, archive.index.id, archive.index.version, pkg.id, pkg.version
            )));
        }
        if archive.index.content_hash != pkg.content_hash {
            return Err(Error::HashMismatch {
                id: pkg.id.clone(),
                version: pkg.version.clone(),
                index: pkg.content_hash.clone(),
                archive: archive.index.content_hash,
            });
        }
        if let Some(ref trust) = trust {
            let key = crate::trust::find_trusted_key(trust, &pkg.key_id)
                .ok_or_else(|| Error::UntrustedKey(pkg.key_id.clone()))?;
            verify_content_hash(&key.public_key, &pkg.content_hash, &pkg.signature).map_err(
                |_| Error::BadSignature {
                    id: pkg.id.clone(),
                    version: pkg.version.clone(),
                },
            )?;
        }
    }

    let adv_path = dir.join("advisories.toml");
    let mut advisories_count = 0;
    if adv_path.is_file() {
        let text = fs::read_to_string(&adv_path).map_err(|source| Error::Io {
            path: adv_path.clone(),
            source,
        })?;
        let file = parse_advisories(&text)?;
        file.require_signature_fields()?;
        let expected = compute_advisories_content_hash(&file)?;
        if expected != file.content_hash {
            return Err(Error::InvalidAdvisories(format!(
                "advisories content_hash mismatch (file has {}, recomputed {})",
                file.content_hash, expected
            )));
        }
        if let Some(ref trust) = trust {
            verify_advisories(&file, trust)?;
        }
        advisories_count = file.advisories.len();
    }

    Ok(ValidateReport {
        packages: index.packages.len(),
        advisories: advisories_count,
    })
}
