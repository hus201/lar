//! LAR package manifest parsing, validation, and `.lar` archive packing.

mod error;
mod id;
mod manifest;
mod pack;

pub use error::Error;
pub use id::validate_package_id;
pub use manifest::{Desktop, Entry, PackageManifest, PackageMeta, FORMAT_VERSION};
pub use pack::{inspect, pack, InitOptions, PackageArchive, PackedFile};

use std::path::{Path, PathBuf};

use manifest::validate_manifest;

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Resolve a path that may be a `package.toml` file or a package directory.
pub fn resolve_manifest_path(path: &Path) -> Result<PathBuf> {
    if path.is_dir() {
        let candidate = path.join("package.toml");
        if candidate.is_file() {
            return Ok(candidate);
        }
        return Err(Error::ManifestNotFound(candidate));
    }
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    Err(Error::ManifestNotFound(path.to_path_buf()))
}

/// Package root directory containing `package.toml` and optional `files/`.
pub fn package_dir_from_manifest(manifest_path: &Path) -> Result<PathBuf> {
    let parent = manifest_path
        .parent()
        .ok_or_else(|| Error::InvalidPath(manifest_path.to_path_buf()))?;
    if parent.as_os_str().is_empty() {
        Ok(PathBuf::from("."))
    } else {
        Ok(parent.to_path_buf())
    }
}

/// Load `package.toml` without checking that entry binaries exist on disk.
pub fn load_manifest(path: &Path) -> Result<PackageManifest> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse_manifest(&text)
}

/// Parse manifest TOML from a string.
pub fn parse_manifest(text: &str) -> Result<PackageManifest> {
    let manifest: PackageManifest = toml::from_str(text)?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

/// Load and validate a package, including entry paths under `files/` when present.
pub fn validate_package(path: &Path) -> Result<PackageManifest> {
    let manifest_path = resolve_manifest_path(path)?;
    let package_dir = package_dir_from_manifest(&manifest_path)?;
    let manifest = load_manifest(&manifest_path)?;
    manifest::validate_entry_files(&manifest, &package_dir)?;
    Ok(manifest)
}

/// Write a new `package.toml` template into `dir`.
pub fn init_package(dir: &Path, opts: &InitOptions) -> Result<PathBuf> {
    pack::init_package(dir, opts)
}
