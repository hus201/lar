use std::fs;
use std::io::Cursor;
use std::net::TcpListener;
use std::path::Path;
use std::thread;

use lar_package::{init_package, pack, InitOptions};
use lar_repo::{
    add_source, audit, audit_should_fail, build_index, fetch_into_store, keygen, list_dep_versions,
    trust_add, write_index, AuditScope, LookupMode, SourcePolicy,
};
use lar_store::{Paths, Store};
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

fn open_prefix(prefix: &Path) -> Store {
    Store::open(Paths::from_prefix(prefix.to_path_buf(), false))
}

#[test]
fn local_fetch_requires_trust_and_signature() {
    let tmp = tempdir().unwrap();
    let prefix = tmp.path().join("prefix");
    let store = open_prefix(&prefix);

    let (public, secret, _id) = keygen().unwrap();
    trust_add(&store, &public, "test").unwrap();

    let pkg_dir = tmp.path().join("pkg");
    let lar_path = pack_lib(&pkg_dir, "org.example.lib", "1.0.0", b"hello");

    let repo = tmp.path().join("repo");
    fs::create_dir_all(repo.join("packages")).unwrap();
    fs::copy(&lar_path, repo.join("packages/org.example.lib-1.0.0.lar")).unwrap();
    let index = build_index(&repo, &secret).unwrap();
    write_index(&repo, &index).unwrap();

    add_source(
        &store,
        "main".into(),
        repo.display().to_string(),
        SourcePolicy::Deps,
        true,
    )
    .unwrap();

    let mut warn = Cursor::new(Vec::new());
    let stored = fetch_into_store(
        &store,
        "org.example.lib",
        "1.0.0",
        LookupMode::Deps,
        &mut warn,
    )
    .unwrap();
    assert_eq!(stored.id, "org.example.lib");
    assert_eq!(stored.version, "1.0.0");
}

#[test]
fn untrusted_key_refuses_fetch() {
    let tmp = tempdir().unwrap();
    let prefix = tmp.path().join("prefix");
    let store = open_prefix(&prefix);

    let (_public, secret, key_id) = keygen().unwrap();
    // Deliberately do not trust the signing key.

    let pkg_dir = tmp.path().join("pkg");
    let lar_path = pack_lib(&pkg_dir, "org.example.lib", "1.0.0", b"untrusted");

    let repo = tmp.path().join("repo");
    fs::create_dir_all(repo.join("packages")).unwrap();
    fs::copy(&lar_path, repo.join("packages/org.example.lib-1.0.0.lar")).unwrap();
    let index = build_index(&repo, &secret).unwrap();
    write_index(&repo, &index).unwrap();

    add_source(
        &store,
        "main".into(),
        repo.display().to_string(),
        SourcePolicy::Deps,
        true,
    )
    .unwrap();

    let mut warn = Cursor::new(Vec::new());
    let err = fetch_into_store(
        &store,
        "org.example.lib",
        "1.0.0",
        LookupMode::Deps,
        &mut warn,
    )
    .unwrap_err();
    assert!(
        matches!(err, lar_repo::Error::UntrustedKey(ref id) if id == &key_id),
        "{err}"
    );
    assert!(store.get("org.example.lib", "1.0.0").unwrap().is_none());
}

#[test]
fn bad_signature_refuses_fetch() {
    let tmp = tempdir().unwrap();
    let prefix = tmp.path().join("prefix");
    let store = open_prefix(&prefix);

    let (public_a, secret_a, _) = keygen().unwrap();
    let (_public_b, secret_b, _) = keygen().unwrap();
    trust_add(&store, &public_a, "publisher-a").unwrap();

    let pkg_dir = tmp.path().join("pkg");
    let lar_path = pack_lib(&pkg_dir, "org.example.lib", "1.0.0", b"bad-sig");

    let repo = tmp.path().join("repo");
    fs::create_dir_all(repo.join("packages")).unwrap();
    fs::copy(&lar_path, repo.join("packages/org.example.lib-1.0.0.lar")).unwrap();
    let mut index = build_index(&repo, &secret_a).unwrap();
    // Replace with a valid Ed25519 signature from a different key over the same hash.
    let content_hash = index.packages[0].content_hash.clone();
    index.packages[0].signature = lar_repo::sign_content_hash(&secret_b, &content_hash).unwrap();
    write_index(&repo, &index).unwrap();

    add_source(
        &store,
        "main".into(),
        repo.display().to_string(),
        SourcePolicy::Deps,
        true,
    )
    .unwrap();

    let mut warn = Cursor::new(Vec::new());
    let err = fetch_into_store(
        &store,
        "org.example.lib",
        "1.0.0",
        LookupMode::Deps,
        &mut warn,
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            lar_repo::Error::BadSignature {
                ref id,
                ref version
            } if id == "org.example.lib" && version == "1.0.0"
        ),
        "{err}"
    );
    assert!(store.get("org.example.lib", "1.0.0").unwrap().is_none());
}

#[test]
fn yanked_refuses_new_fetch() {
    let tmp = tempdir().unwrap();
    let prefix = tmp.path().join("prefix");
    let store = open_prefix(&prefix);

    let (public, secret, _) = keygen().unwrap();
    trust_add(&store, &public, "").unwrap();

    let pkg_dir = tmp.path().join("pkg");
    let lar_path = pack_lib(&pkg_dir, "org.example.lib", "1.0.0", b"yank-me");

    let repo = tmp.path().join("repo");
    fs::create_dir_all(repo.join("packages")).unwrap();
    fs::copy(&lar_path, repo.join("packages/org.example.lib-1.0.0.lar")).unwrap();
    let index = build_index(&repo, &secret).unwrap();
    write_index(&repo, &index).unwrap();
    fs::write(
        repo.join("advisories.toml"),
        r#"
format = 1

[[advisories]]
id = "LAR-2026-0001"
package_id = "org.example.lib"
versions = ["1.0.0"]
severity = "critical"
yanked = true
summary = "Yanked pin"
"#,
    )
    .unwrap();

    add_source(
        &store,
        "main".into(),
        repo.display().to_string(),
        SourcePolicy::Both,
        true,
    )
    .unwrap();

    let mut warn = Cursor::new(Vec::new());
    let err = fetch_into_store(
        &store,
        "org.example.lib",
        "1.0.0",
        LookupMode::Deps,
        &mut warn,
    )
    .unwrap_err();
    assert!(err.to_string().contains("yanked"), "{err}");
}

#[test]
fn advisory_warns_on_fetch() {
    let tmp = tempdir().unwrap();
    let prefix = tmp.path().join("prefix");
    let store = open_prefix(&prefix);

    let (public, secret, _) = keygen().unwrap();
    trust_add(&store, &public, "").unwrap();

    let pkg_dir = tmp.path().join("pkg");
    let lar_path = pack_lib(&pkg_dir, "org.example.lib", "1.0.0", b"warn-me");

    let repo = tmp.path().join("repo");
    fs::create_dir_all(repo.join("packages")).unwrap();
    fs::copy(&lar_path, repo.join("packages/org.example.lib-1.0.0.lar")).unwrap();
    let index = build_index(&repo, &secret).unwrap();
    write_index(&repo, &index).unwrap();
    fs::write(
        repo.join("advisories.toml"),
        r#"
format = 1

[[advisories]]
id = "LAR-2026-0002"
package_id = "org.example.lib"
versions = ["1.0.0"]
severity = "medium"
yanked = false
summary = "Known issue"
url = "https://example.test/LAR-2026-0002"
"#,
    )
    .unwrap();

    add_source(
        &store,
        "main".into(),
        repo.display().to_string(),
        SourcePolicy::Deps,
        true,
    )
    .unwrap();

    let mut warn = Cursor::new(Vec::new());
    fetch_into_store(
        &store,
        "org.example.lib",
        "1.0.0",
        LookupMode::Deps,
        &mut warn,
    )
    .unwrap();
    let text = String::from_utf8(warn.into_inner()).unwrap();
    assert!(text.contains("LAR-2026-0002"), "{text}");
    assert!(text.contains("Known issue"), "{text}");
}

#[test]
fn invalid_advisories_fail_fetch() {
    let tmp = tempdir().unwrap();
    let prefix = tmp.path().join("prefix");
    let store = open_prefix(&prefix);

    let (public, secret, _) = keygen().unwrap();
    trust_add(&store, &public, "").unwrap();

    let pkg_dir = tmp.path().join("pkg");
    let lar_path = pack_lib(&pkg_dir, "org.example.lib", "1.0.0", b"bad-adv");

    let repo = tmp.path().join("repo");
    fs::create_dir_all(repo.join("packages")).unwrap();
    fs::copy(&lar_path, repo.join("packages/org.example.lib-1.0.0.lar")).unwrap();
    let index = build_index(&repo, &secret).unwrap();
    write_index(&repo, &index).unwrap();
    fs::write(
        repo.join("advisories.toml"),
        r#"
format = 1

[[advisories]]
id = "LAR-2026-BAD"
package_id = "org.example.lib"
severity = "high"
summary = "missing versions and hashes"
"#,
    )
    .unwrap();

    add_source(
        &store,
        "main".into(),
        repo.display().to_string(),
        SourcePolicy::Deps,
        true,
    )
    .unwrap();

    let mut warn = Cursor::new(Vec::new());
    let err = fetch_into_store(
        &store,
        "org.example.lib",
        "1.0.0",
        LookupMode::Deps,
        &mut warn,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("advisory") || err.to_string().contains("advisories"),
        "{err}"
    );
}

#[test]
fn audit_fails_on_high_severity() {
    let tmp = tempdir().unwrap();
    let prefix = tmp.path().join("prefix");
    let store = open_prefix(&prefix);

    let (public, secret, _) = keygen().unwrap();
    trust_add(&store, &public, "").unwrap();

    let pkg_dir = tmp.path().join("pkg");
    let lar_path = pack_lib(&pkg_dir, "org.example.lib", "1.0.0", b"audit");
    store.add(&lar_path).unwrap();

    let repo = tmp.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    // advisories only — no packages needed for audit of store pins
    fs::write(
        repo.join("advisories.toml"),
        r#"
format = 1

[[advisories]]
id = "LAR-2026-0003"
package_id = "org.example.lib"
versions = ["1.0.0"]
severity = "high"
summary = "High severity"
"#,
    )
    .unwrap();
    // minimal signed index so source is valid when present
    fs::copy(&lar_path, repo.join("org.example.lib-1.0.0.lar")).unwrap();
    let index = build_index(&repo, &secret).unwrap();
    write_index(&repo, &index).unwrap();

    add_source(
        &store,
        "vendor".into(),
        repo.display().to_string(),
        SourcePolicy::Both,
        false,
    )
    .unwrap();

    let mut out = Vec::new();
    let findings = audit(&store, AuditScope::Store, &mut out).unwrap();
    assert!(!findings.is_empty());
    assert!(audit_should_fail(&findings));
}

#[test]
fn http_fetch_works() {
    let tmp = tempdir().unwrap();
    let prefix = tmp.path().join("prefix");
    let store = open_prefix(&prefix);

    let (public, secret, _) = keygen().unwrap();
    trust_add(&store, &public, "").unwrap();

    let pkg_dir = tmp.path().join("pkg");
    let lar_path = pack_lib(&pkg_dir, "org.example.lib", "1.0.0", b"http");

    let repo = tmp.path().join("repo");
    fs::create_dir_all(repo.join("packages")).unwrap();
    fs::copy(&lar_path, repo.join("packages/org.example.lib-1.0.0.lar")).unwrap();
    let index = build_index(&repo, &secret).unwrap();
    write_index(&repo, &index).unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let root = repo.clone();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = serve_file(&root, stream);
        }
    });

    let uri = format!("http://{addr}/");
    add_source(&store, "http".into(), uri, SourcePolicy::Deps, true).unwrap();

    let mut warn = Cursor::new(Vec::new());
    let stored = fetch_into_store(
        &store,
        "org.example.lib",
        "1.0.0",
        LookupMode::Deps,
        &mut warn,
    )
    .unwrap();
    assert_eq!(stored.version, "1.0.0");
}

#[test]
fn list_dep_versions_skips_yanked_index_pins() {
    let tmp = tempdir().unwrap();
    let prefix = tmp.path().join("prefix");
    let store = open_prefix(&prefix);

    let (public, secret, _) = keygen().unwrap();
    trust_add(&store, &public, "").unwrap();

    let pkg_old = tmp.path().join("pkg-old");
    let lar_old = pack_lib(&pkg_old, "org.example.lib", "1.0.0", b"old");
    let pkg_new = tmp.path().join("pkg-new");
    let lar_new = pack_lib(&pkg_new, "org.example.lib", "1.1.0", b"new");

    let repo = tmp.path().join("repo");
    fs::create_dir_all(repo.join("packages")).unwrap();
    fs::copy(&lar_old, repo.join("packages/org.example.lib-1.0.0.lar")).unwrap();
    fs::copy(&lar_new, repo.join("packages/org.example.lib-1.1.0.lar")).unwrap();
    let index = build_index(&repo, &secret).unwrap();
    write_index(&repo, &index).unwrap();
    fs::write(
        repo.join("advisories.toml"),
        r#"
format = 1

[[advisories]]
id = "LAR-YANK"
package_id = "org.example.lib"
versions = ["1.0.0"]
severity = "high"
yanked = true
summary = "yanked"
"#,
    )
    .unwrap();

    add_source(
        &store,
        "main".into(),
        repo.display().to_string(),
        SourcePolicy::Deps,
        true,
    )
    .unwrap();

    let versions = list_dep_versions(&store, "org.example.lib").unwrap();
    assert!(versions.contains(&"1.1.0".to_string()), "{versions:?}");
    assert!(
        !versions.contains(&"1.0.0".to_string()),
        "yanked 1.0.0 should be excluded: {versions:?}"
    );
}

fn serve_file(root: &Path, mut stream: std::net::TcpStream) -> std::io::Result<()> {
    use std::io::{Read, Write};
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf)?;
    let req = String::from_utf8_lossy(&buf[..n]);
    let path = req
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let rel = path.trim_start_matches('/');
    let file = if rel.is_empty() {
        root.join("index.toml")
    } else {
        root.join(rel)
    };
    if file.is_file() {
        let body = fs::read(&file)?;
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(header.as_bytes())?;
        stream.write_all(&body)?;
    } else {
        stream.write_all(
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )?;
    }
    Ok(())
}
