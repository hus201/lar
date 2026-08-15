use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::manifest::{
    normalize_payload_rel_path, validate_entry_files, PackageManifest, FORMAT_VERSION,
};
use crate::{
    load_manifest, package_dir_from_manifest, parse_manifest, resolve_manifest_path, Error, Result,
};

/// Options for `lar package init`.
#[derive(Debug, Clone)]
pub struct InitOptions {
    pub id: String,
    pub name: String,
    pub version: String,
    pub force: bool,
}

/// One file recorded in `manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackedFile {
    pub path: String,
    pub blake3: String,
    pub size: u64,
}

/// Machine index written into the `.lar` archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveIndex {
    pub format: u32,
    pub id: String,
    pub version: String,
    pub content_hash: String,
    pub files: Vec<PackedFile>,
}

/// Metadata read back from a `.lar` archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageArchive {
    pub manifest: PackageManifest,
    pub index: ArchiveIndex,
}

/// Create a staged package directory with a template `package.toml`.
pub fn init_package(dir: &Path, opts: &InitOptions) -> Result<PathBuf> {
    fs::create_dir_all(dir).map_err(|source| Error::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    let files_dir = dir.join("files");
    fs::create_dir_all(&files_dir).map_err(|source| Error::Io {
        path: files_dir.clone(),
        source,
    })?;

    let manifest_path = dir.join("package.toml");
    if manifest_path.exists() && !opts.force {
        return Err(Error::ManifestExists(manifest_path));
    }

    let manifest = PackageManifest {
        package: crate::PackageMeta {
            format: FORMAT_VERSION,
            id: opts.id.clone(),
            name: opts.name.clone(),
            version: opts.version.clone(),
            description: Some(String::new()),
            content_hash: None,
        },
        dependencies: Default::default(),
        entry: None,
        desktop: None,
    };
    crate::manifest::validate_manifest(&manifest)?;
    let text = toml::to_string_pretty(&manifest)?;
    fs::write(&manifest_path, text).map_err(|source| Error::Io {
        path: manifest_path.clone(),
        source,
    })?;
    Ok(manifest_path)
}

/// Pack a staged package directory into a `.lar` (tar + zstd) archive.
pub fn pack(package_dir: &Path, output: &Path) -> Result<PackageArchive> {
    let manifest_path = resolve_manifest_path(package_dir)?;
    let package_dir = package_dir_from_manifest(&manifest_path)?;
    let mut manifest = load_manifest(&manifest_path)?;
    validate_entry_files(&manifest, &package_dir)?;

    let files_dir = package_dir.join("files");
    let entries = collect_payload(&files_dir)?;
    let packed_files: Vec<PackedFile> = entries.iter().map(|e| e.meta.clone()).collect();
    let content_hash = compute_content_hash(&packed_files);
    manifest.package.content_hash = Some(content_hash.clone());

    let package_toml = toml::to_string_pretty(&manifest)?;
    fs::write(&manifest_path, &package_toml).map_err(|source| Error::Io {
        path: manifest_path.clone(),
        source,
    })?;

    let index = ArchiveIndex {
        format: FORMAT_VERSION,
        id: manifest.package.id.clone(),
        version: manifest.package.version.clone(),
        content_hash: content_hash.clone(),
        files: packed_files,
    };

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
    }

    let out = File::create(output).map_err(|source| Error::Io {
        path: output.to_path_buf(),
        source,
    })?;
    let encoder = zstd::Encoder::new(out, 3).map_err(|source| Error::Io {
        path: output.to_path_buf(),
        source,
    })?;
    let mut builder = tar::Builder::new(encoder);

    append_bytes(&mut builder, "package.toml", package_toml.as_bytes())?;

    let index_json = serde_json::to_vec_pretty(&index)?;
    append_bytes(&mut builder, "manifest.json", &index_json)?;

    for entry in &entries {
        let archive_name = format!("files/{}", entry.meta.path);
        append_file_bytes(&mut builder, &archive_name, &entry.data, entry.mode)?;
    }

    let encoder = builder
        .into_inner()
        .map_err(|source| Error::Archive(source.to_string()))?;
    encoder
        .finish()
        .map_err(|source| Error::Archive(source.to_string()))?;

    Ok(PackageArchive { manifest, index })
}

/// Read a `.lar` archive, re-hash payload files, and verify digests.
pub fn inspect(archive_path: &Path) -> Result<PackageArchive> {
    let loaded = load_verified_archive(archive_path)?;
    Ok(PackageArchive {
        manifest: loaded.manifest,
        index: loaded.index,
    })
}

/// Verify a `.lar` archive and extract it into `dest` (`package.toml`, `manifest.json`, `files/`).
pub fn extract(archive_path: &Path, dest: &Path) -> Result<PackageArchive> {
    let loaded = load_verified_archive(archive_path)?;

    if dest.exists() {
        return Err(Error::Validation(format!(
            "extract destination already exists: {}",
            dest.display()
        )));
    }
    fs::create_dir_all(dest).map_err(|source| Error::Io {
        path: dest.to_path_buf(),
        source,
    })?;

    let package_toml_path = dest.join("package.toml");
    fs::write(&package_toml_path, &loaded.package_toml).map_err(|source| Error::Io {
        path: package_toml_path,
        source,
    })?;

    let manifest_json_path = dest.join("manifest.json");
    fs::write(&manifest_json_path, &loaded.manifest_json).map_err(|source| Error::Io {
        path: manifest_json_path,
        source,
    })?;

    let files_root = dest.join("files");
    for (rel, (data, mode)) in &loaded.payload {
        let out_path = files_root.join(rel);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::write(&out_path, data).map_err(|source| Error::Io {
            path: out_path.clone(),
            source,
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&out_path, fs::Permissions::from_mode(*mode)).map_err(
                |source| Error::Io {
                    path: out_path.clone(),
                    source,
                },
            )?;
        }
    }

    let archive = PackageArchive {
        manifest: loaded.manifest,
        index: loaded.index,
    };
    if let Err(err) = verify_package_dir(dest) {
        let _ = fs::remove_dir_all(dest);
        return Err(err);
    }
    Ok(archive)
}

/// Re-hash an on-disk package directory (`package.toml`, `manifest.json`, `files/`) and verify digests.
pub fn verify_package_dir(dir: &Path) -> Result<PackageArchive> {
    let manifest_path = dir.join("package.toml");
    let manifest = load_manifest(&manifest_path)?;
    let index_path = dir.join("manifest.json");
    let index_bytes = fs::read(&index_path).map_err(|source| Error::Io {
        path: index_path,
        source,
    })?;
    let index: ArchiveIndex = serde_json::from_slice(&index_bytes)?;
    let payload = load_payload_map(&dir.join("files"))?;
    verify_archive_integrity(&manifest, &index, &payload)?;
    Ok(PackageArchive { manifest, index })
}

fn load_payload_map(files_dir: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    let entries = collect_payload(files_dir)?;
    Ok(entries
        .into_iter()
        .map(|entry| (entry.meta.path, entry.data))
        .collect())
}

struct VerifiedArchive {
    package_toml: String,
    manifest_json: Vec<u8>,
    /// Relative path under `files/` -> (bytes, unix mode)
    payload: BTreeMap<String, (Vec<u8>, u32)>,
    manifest: PackageManifest,
    index: ArchiveIndex,
}

fn load_verified_archive(archive_path: &Path) -> Result<VerifiedArchive> {
    let file = File::open(archive_path).map_err(|source| Error::Io {
        path: archive_path.to_path_buf(),
        source,
    })?;
    let decoder = zstd::Decoder::new(file).map_err(|source| Error::Io {
        path: archive_path.to_path_buf(),
        source,
    })?;
    let mut archive = tar::Archive::new(decoder);

    let mut package_toml = None;
    let mut manifest_json = None;
    let mut payload: BTreeMap<String, (Vec<u8>, u32)> = BTreeMap::new();

    for entry in archive
        .entries()
        .map_err(|source| Error::Archive(source.to_string()))?
    {
        let mut entry = entry.map_err(|source| Error::Archive(source.to_string()))?;
        let path = entry
            .path()
            .map_err(|source| Error::Archive(source.to_string()))?
            .into_owned();
        let path_str = path.to_string_lossy();

        if path_str == "package.toml" {
            let mut buf = String::new();
            entry
                .read_to_string(&mut buf)
                .map_err(|source| Error::Archive(source.to_string()))?;
            package_toml = Some(buf);
            continue;
        }
        if path_str == "manifest.json" {
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .map_err(|source| Error::Archive(source.to_string()))?;
            manifest_json = Some(buf);
            continue;
        }

        let Some(rel) = path_str.strip_prefix("files/") else {
            return Err(Error::Archive(format!(
                "unexpected archive member: {path_str}"
            )));
        };
        if rel.is_empty() || entry.header().entry_type().is_dir() {
            continue;
        }
        if entry.header().entry_type().is_symlink() || entry.header().entry_type().is_hard_link() {
            return Err(Error::Integrity(format!(
                "archive payload must not contain links: {path_str}"
            )));
        }
        if !entry.header().entry_type().is_file() {
            return Err(Error::Integrity(format!(
                "archive payload must contain only regular files: {path_str}"
            )));
        }

        let rel = normalize_payload_rel_path(Path::new(rel))?;
        let mode = entry.header().mode().unwrap_or(0o644);
        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|source| Error::Archive(source.to_string()))?;
        if payload.insert(rel.clone(), (buf, mode)).is_some() {
            return Err(Error::Integrity(format!(
                "duplicate payload path in archive: {rel}"
            )));
        }
    }

    let package_toml =
        package_toml.ok_or_else(|| Error::Archive("archive is missing package.toml".into()))?;
    let manifest_json =
        manifest_json.ok_or_else(|| Error::Archive("archive is missing manifest.json".into()))?;

    let manifest = parse_manifest(&package_toml)?;
    let index: ArchiveIndex = serde_json::from_slice(&manifest_json)?;
    let payload_bytes: BTreeMap<String, Vec<u8>> = payload
        .iter()
        .map(|(k, (data, _))| (k.clone(), data.clone()))
        .collect();
    verify_archive_integrity(&manifest, &index, &payload_bytes)?;

    Ok(VerifiedArchive {
        package_toml,
        manifest_json,
        payload,
        manifest,
        index,
    })
}

fn verify_archive_integrity(
    manifest: &PackageManifest,
    index: &ArchiveIndex,
    payload: &BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    if index.format != FORMAT_VERSION {
        return Err(Error::Integrity(format!(
            "unsupported manifest.json format {} (supported: {FORMAT_VERSION})",
            index.format
        )));
    }
    if index.format != manifest.package.format {
        return Err(Error::Integrity(format!(
            "package.toml format {} does not match manifest.json format {}",
            manifest.package.format, index.format
        )));
    }
    if index.id != manifest.package.id {
        return Err(Error::Integrity(format!(
            "package.toml id `{}` does not match manifest.json id `{}`",
            manifest.package.id, index.id
        )));
    }
    if index.version != manifest.package.version {
        return Err(Error::Integrity(format!(
            "package.toml version `{}` does not match manifest.json version `{}`",
            manifest.package.version, index.version
        )));
    }

    let Some(manifest_hash) = &manifest.package.content_hash else {
        return Err(Error::Integrity(
            "package.toml is missing content_hash".into(),
        ));
    };
    if manifest_hash != &index.content_hash {
        return Err(Error::Integrity(
            "package.toml content_hash does not match manifest.json content_hash".into(),
        ));
    }

    let mut observed = Vec::with_capacity(payload.len());
    for (path, data) in payload {
        observed.push(PackedFile {
            path: path.clone(),
            blake3: blake3::hash(data).to_hex().to_string(),
            size: data.len() as u64,
        });
    }
    observed.sort_by(|a, b| a.path.cmp(&b.path));

    let mut expected = index.files.clone();
    expected.sort_by(|a, b| a.path.cmp(&b.path));

    if observed.len() != expected.len() {
        return Err(Error::Integrity(format!(
            "payload file count mismatch: archive has {}, manifest.json lists {}",
            observed.len(),
            expected.len()
        )));
    }

    for (got, want) in observed.iter().zip(expected.iter()) {
        if got.path != want.path {
            return Err(Error::Integrity(format!(
                "payload path mismatch: found `{}`, expected `{}`",
                got.path, want.path
            )));
        }
        if got.size != want.size {
            return Err(Error::Integrity(format!(
                "size mismatch for `{}`: archive {}, manifest.json {}",
                got.path, got.size, want.size
            )));
        }
        if got.blake3 != want.blake3 {
            return Err(Error::Integrity(format!(
                "blake3 mismatch for `{}`",
                got.path
            )));
        }
    }

    let computed = compute_content_hash(&observed);
    if computed != index.content_hash {
        return Err(Error::Integrity(
            "recomputed content_hash does not match manifest.json".into(),
        ));
    }

    Ok(())
}

struct PayloadEntry {
    meta: PackedFile,
    data: Vec<u8>,
    mode: u32,
}

/// Walk `files/` once: read each regular file, hash it, and keep bytes for the archive.
fn collect_payload(files_dir: &Path) -> Result<Vec<PayloadEntry>> {
    let mut entries = Vec::new();
    if !files_dir.exists() {
        return Ok(entries);
    }
    if !files_dir.is_dir() {
        return Err(Error::Validation(format!(
            "files path is not a directory: {}",
            files_dir.display()
        )));
    }
    walk_payload(files_dir, files_dir, &mut entries)?;
    entries.sort_by(|a, b| a.meta.path.cmp(&b.meta.path));
    Ok(entries)
}

fn walk_payload(root: &Path, dir: &Path, out: &mut Vec<PayloadEntry>) -> Result<()> {
    let entries = fs::read_dir(dir).map_err(|source| Error::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| Error::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        match payload_entry_kind(&path, &file_type)? {
            PayloadKind::Dir => walk_payload(root, &path, out)?,
            PayloadKind::File => {
                let rel = path.strip_prefix(root).map_err(|_| {
                    Error::Validation(format!("file outside files/: {}", path.display()))
                })?;
                let rel_str = normalize_payload_rel_path(rel)?;
                let data = fs::read(&path).map_err(|source| Error::Io {
                    path: path.clone(),
                    source,
                })?;
                let meta = fs::metadata(&path).map_err(|source| Error::Io {
                    path: path.clone(),
                    source,
                })?;
                let mode = payload_file_mode(&meta);
                let digest = blake3::hash(&data);
                out.push(PayloadEntry {
                    meta: PackedFile {
                        path: rel_str,
                        blake3: digest.to_hex().to_string(),
                        size: data.len() as u64,
                    },
                    data,
                    mode,
                });
            }
        }
    }
    Ok(())
}

enum PayloadKind {
    Dir,
    File,
}

/// v1 payload may only contain real directories and regular files.
fn payload_entry_kind(path: &Path, file_type: &std::fs::FileType) -> Result<PayloadKind> {
    if file_type.is_symlink() {
        return Err(Error::Validation(format!(
            "symlinks are not allowed in package payload (v1): {}",
            path.display()
        )));
    }
    if file_type.is_dir() {
        return Ok(PayloadKind::Dir);
    }
    if file_type.is_file() {
        return Ok(PayloadKind::File);
    }
    Err(Error::Validation(format!(
        "non-regular files are not allowed in package payload (v1): {}",
        path.display()
    )))
}

fn payload_file_mode(meta: &std::fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode()
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        0o644
    }
}

fn compute_content_hash(files: &[PackedFile]) -> String {
    let mut hasher = blake3::Hasher::new();
    for file in files {
        hasher.update(file.path.as_bytes());
        hasher.update(&[0]);
        hasher.update(file.blake3.as_bytes());
        hasher.update(&[0]);
        hasher.update(&file.size.to_le_bytes());
        hasher.update(&[0]);
    }
    format!("blake3:{}", hasher.finalize().to_hex())
}

fn append_bytes<W: Write>(builder: &mut tar::Builder<W>, name: &str, data: &[u8]) -> Result<()> {
    append_file_bytes(builder, name, data, 0o644)
}

fn append_file_bytes<W: Write>(
    builder: &mut tar::Builder<W>,
    name: &str,
    data: &[u8],
    mode: u32,
) -> Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(mode);
    header.set_cksum();
    builder
        .append_data(&mut header, name, data)
        .map_err(|source| Error::Archive(source.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn pack_round_trip() {
        let dir = tempdir().unwrap();
        let pkg = dir.path().join("pkg");
        init_package(
            &pkg,
            &InitOptions {
                id: "org.example.editor".into(),
                name: "Example Editor".into(),
                version: "0.1.0".into(),
                force: false,
            },
        )
        .unwrap();

        let bin = pkg.join("files/bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("editor"), b"#!/bin/sh\necho hi\n").unwrap();

        let mut manifest = load_manifest(&pkg.join("package.toml")).unwrap();
        manifest.entry = Some(crate::Entry {
            default: Some("bin/editor".into()),
            binaries: vec!["bin/editor".into()],
        });
        fs::write(
            pkg.join("package.toml"),
            toml::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let out = dir.path().join("org.example.editor-0.1.0.lar");
        let packed = pack(&pkg, &out).unwrap();
        assert!(out.is_file());
        assert!(packed
            .manifest
            .package
            .content_hash
            .as_ref()
            .unwrap()
            .starts_with("blake3:"));
        assert_eq!(packed.index.files.len(), 1);
        assert_eq!(packed.index.files[0].path, "bin/editor");

        let staged = load_manifest(&pkg.join("package.toml")).unwrap();
        assert_eq!(
            staged.package.content_hash,
            packed.manifest.package.content_hash
        );

        let inspected = inspect(&out).unwrap();
        assert_eq!(inspected.manifest.package.id, "org.example.editor");
        assert_eq!(inspected.manifest.package.version, "0.1.0");
        assert_eq!(
            inspected.index.content_hash,
            packed.manifest.package.content_hash.clone().unwrap()
        );
    }

    #[test]
    fn pack_rejects_symlink_in_payload() {
        let dir = tempdir().unwrap();
        let pkg = dir.path().join("pkg");
        init_package(
            &pkg,
            &InitOptions {
                id: "org.example.editor".into(),
                name: "Example Editor".into(),
                version: "0.1.0".into(),
                force: false,
            },
        )
        .unwrap();

        let files = pkg.join("files");
        fs::write(files.join("real.txt"), b"data").unwrap();
        std::os::unix::fs::symlink("real.txt", files.join("link.txt")).unwrap();

        let out = dir.path().join("out.lar");
        let err = pack(&pkg, &out).unwrap_err();
        assert!(
            err.to_string().contains("symlink"),
            "expected symlink error, got: {err}"
        );
    }

    #[test]
    fn inspect_rejects_tampered_payload() {
        let dir = tempdir().unwrap();
        let pkg = dir.path().join("pkg");
        init_package(
            &pkg,
            &InitOptions {
                id: "org.example.editor".into(),
                name: "Example Editor".into(),
                version: "0.1.0".into(),
                force: false,
            },
        )
        .unwrap();
        fs::write(pkg.join("files/hello.txt"), b"hello").unwrap();

        let good = dir.path().join("good.lar");
        let packed = pack(&pkg, &good).unwrap();

        let bad = dir.path().join("bad.lar");
        let out = File::create(&bad).unwrap();
        let encoder = zstd::Encoder::new(out, 3).unwrap();
        let mut builder = tar::Builder::new(encoder);

        let package_toml = toml::to_string_pretty(&packed.manifest).unwrap();
        append_bytes(&mut builder, "package.toml", package_toml.as_bytes()).unwrap();
        let index_json = serde_json::to_vec_pretty(&packed.index).unwrap();
        append_bytes(&mut builder, "manifest.json", &index_json).unwrap();
        append_bytes(&mut builder, "files/hello.txt", b"TAMPERED").unwrap();

        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();

        let err = inspect(&bad).unwrap_err();
        assert!(
            err.to_string().contains("blake3 mismatch") || err.to_string().contains("integrity"),
            "expected integrity error, got: {err}"
        );
    }

    #[test]
    fn extract_round_trip() {
        let dir = tempdir().unwrap();
        let pkg = dir.path().join("pkg");
        init_package(
            &pkg,
            &InitOptions {
                id: "org.example.editor".into(),
                name: "Example Editor".into(),
                version: "0.1.0".into(),
                force: false,
            },
        )
        .unwrap();
        fs::write(pkg.join("files/hello.txt"), b"hello").unwrap();

        let archive = dir.path().join("pkg.lar");
        pack(&pkg, &archive).unwrap();

        let dest = dir.path().join("extracted");
        let extracted = extract(&archive, &dest).unwrap();
        assert_eq!(extracted.manifest.package.id, "org.example.editor");
        assert!(dest.join("package.toml").is_file());
        assert!(dest.join("manifest.json").is_file());
        assert_eq!(fs::read(dest.join("files/hello.txt")).unwrap(), b"hello");

        let err = extract(&archive, &dest).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn verify_package_dir_rejects_tampered_disk() {
        let dir = tempdir().unwrap();
        let pkg = dir.path().join("pkg");
        init_package(
            &pkg,
            &InitOptions {
                id: "org.example.editor".into(),
                name: "Example Editor".into(),
                version: "0.1.0".into(),
                force: false,
            },
        )
        .unwrap();
        fs::write(pkg.join("files/hello.txt"), b"hello").unwrap();

        let archive = dir.path().join("pkg.lar");
        pack(&pkg, &archive).unwrap();

        let dest = dir.path().join("extracted");
        extract(&archive, &dest).unwrap();
        fs::write(dest.join("files/hello.txt"), b"TAMPERED").unwrap();

        let err = verify_package_dir(&dest).unwrap_err();
        assert!(
            err.to_string().contains("blake3 mismatch") || err.to_string().contains("integrity"),
            "expected integrity error, got: {err}"
        );
    }
}
