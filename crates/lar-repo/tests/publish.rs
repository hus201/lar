use std::fs;
use std::path::Path;

use lar_package::{init_package, pack, InitOptions};
use lar_repo::{init_repo, keygen, publish_package, unpublish_package, validate_repo, Error};
use tempfile::tempdir;

fn pack_lib(dir: &Path, id: &str, version: &str, body: &[u8]) -> std::path::PathBuf {
    init_package(
        dir,
        &InitOptions {
            id: id.into(),
            name: "Lib".into(),
            version: version.into(),
            force: false,
        },
    )
    .unwrap();
    fs::write(dir.join("files/payload.txt"), body).unwrap();
    let out = dir.join(format!("{id}-{version}.lar"));
    pack(dir, &out).unwrap();
    out
}

#[test]
fn init_publish_validate_unpublish() {
    let tmp = tempdir().unwrap();
    let (public, secret, _) = keygen().unwrap();
    let repo = tmp.path().join("repo");

    let index_path = init_repo(&repo, &secret).unwrap();
    assert!(index_path.is_file());
    assert!(repo.join("packages").is_dir());

    let report = validate_repo(&repo, Some(&public)).unwrap();
    assert_eq!(report.packages, 0);
    assert_eq!(report.advisories, 0);

    let pkg_dir = tmp.path().join("pkg");
    let lar_path = pack_lib(&pkg_dir, "org.example.lib", "1.0.0", b"hello");
    let (info, index) = publish_package(&repo, &lar_path, &secret).unwrap();
    assert_eq!(info.id, "org.example.lib");
    assert_eq!(info.version, "1.0.0");
    assert_eq!(info.file, "packages/org.example.lib-1.0.0.lar");
    assert_eq!(index.packages.len(), 1);
    assert!(repo.join("packages/org.example.lib-1.0.0.lar").is_file());

    let report = validate_repo(&repo, Some(&public)).unwrap();
    assert_eq!(report.packages, 1);

    let index = unpublish_package(&repo, "org.example.lib", "1.0.0", &secret).unwrap();
    assert!(index.packages.is_empty());
    assert!(!repo.join("packages/org.example.lib-1.0.0.lar").is_file());

    let report = validate_repo(&repo, Some(&public)).unwrap();
    assert_eq!(report.packages, 0);
}

#[test]
fn init_refuses_existing_index() {
    let tmp = tempdir().unwrap();
    let (_public, secret, _) = keygen().unwrap();
    let repo = tmp.path().join("repo");
    init_repo(&repo, &secret).unwrap();
    let err = init_repo(&repo, &secret).unwrap_err();
    assert!(
        matches!(err, Error::Other(ref msg) if msg.contains("already initialized")),
        "{err}"
    );
}

#[test]
fn validate_detects_missing_file() {
    let tmp = tempdir().unwrap();
    let (public, secret, _) = keygen().unwrap();
    let repo = tmp.path().join("repo");
    init_repo(&repo, &secret).unwrap();

    let pkg_dir = tmp.path().join("pkg");
    let lar_path = pack_lib(&pkg_dir, "org.example.lib", "1.0.0", b"gone");
    publish_package(&repo, &lar_path, &secret).unwrap();
    fs::remove_file(repo.join("packages/org.example.lib-1.0.0.lar")).unwrap();

    let err = validate_repo(&repo, Some(&public)).unwrap_err();
    assert!(
        matches!(err, Error::Other(ref msg) if msg.contains("missing")),
        "{err}"
    );
}

#[test]
fn validate_detects_hash_mismatch() {
    let tmp = tempdir().unwrap();
    let (public, secret, _) = keygen().unwrap();
    let repo = tmp.path().join("repo");
    init_repo(&repo, &secret).unwrap();

    let pkg_a = tmp.path().join("pkg_a");
    let lar_a = pack_lib(&pkg_a, "org.example.lib", "1.0.0", b"aaa");
    publish_package(&repo, &lar_a, &secret).unwrap();

    let pkg_b = tmp.path().join("pkg_b");
    let lar_b = pack_lib(&pkg_b, "org.example.lib", "1.0.0", b"bbb");
    fs::copy(&lar_b, repo.join("packages/org.example.lib-1.0.0.lar")).unwrap();

    let err = validate_repo(&repo, Some(&public)).unwrap_err();
    assert!(matches!(err, Error::HashMismatch { .. }), "{err}");
}

#[test]
fn unpublish_missing_package_errors() {
    let tmp = tempdir().unwrap();
    let (_public, secret, _) = keygen().unwrap();
    let repo = tmp.path().join("repo");
    init_repo(&repo, &secret).unwrap();
    let err = unpublish_package(&repo, "org.example.lib", "1.0.0", &secret).unwrap_err();
    assert!(
        matches!(
            err,
            Error::PackageNotFound {
                ref id,
                ref version
            } if id == "org.example.lib" && version == "1.0.0"
        ),
        "{err}"
    );
}
