use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use url::Url;

use crate::advisories::{empty_advisories, parse_advisories, AdvisoriesFile};
use crate::index::{parse_index, PackageIndex};
use crate::Error;
use crate::Result;

/// Resolved base for a source uri.
#[derive(Debug, Clone)]
pub enum SourceBase {
    Dir(PathBuf),
    Http(Url),
}

pub fn parse_uri(uri: &str) -> Result<SourceBase> {
    if let Ok(url) = Url::parse(uri) {
        match url.scheme() {
            "file" => {
                let path = url
                    .to_file_path()
                    .map_err(|_| Error::UnsupportedUri(uri.into()))?;
                Ok(SourceBase::Dir(path))
            }
            "http" | "https" => Ok(SourceBase::Http(url)),
            _ => Err(Error::UnsupportedUri(uri.into())),
        }
    } else {
        // Treat as filesystem path.
        let path = PathBuf::from(uri);
        if path.as_os_str().is_empty() {
            return Err(Error::UnsupportedUri(uri.into()));
        }
        Ok(SourceBase::Dir(path))
    }
}

pub fn read_index(base: &SourceBase) -> Result<PackageIndex> {
    let text = read_text(base, "index.toml")?;
    parse_index(&text)
}

pub fn read_advisories(base: &SourceBase) -> Result<AdvisoriesFile> {
    match read_text(base, "advisories.toml") {
        Ok(text) => {
            let file = parse_advisories(&text)?;
            file.require_signature_fields()?;
            Ok(file)
        }
        Err(err) if is_missing_advisories(&err) => Ok(empty_advisories()),
        Err(err) => Err(err),
    }
}

/// True only when `advisories.toml` is absent (local NotFound or HTTP 404).
fn is_missing_advisories(err: &Error) -> bool {
    match err {
        Error::Io { source, .. } => source.kind() == std::io::ErrorKind::NotFound,
        Error::Http { message, .. } => {
            // `HTTP {status}` uses StatusCode Display, e.g. "HTTP 404 Not Found".
            message.contains("404")
        }
        _ => false,
    }
}

/// Fetch a relative blob into a new temp file; caller deletes it.
pub fn fetch_blob(base: &SourceBase, relative: &str) -> Result<PathBuf> {
    crate::index::validate_relative_file(relative)?;
    let bytes = read_bytes(base, relative)?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!("lar-fetch-{nanos}.lar"));
    let mut file = fs::File::create(&tmp).map_err(|source| Error::Io {
        path: tmp.clone(),
        source,
    })?;
    file.write_all(&bytes).map_err(|source| Error::Io {
        path: tmp.clone(),
        source,
    })?;
    Ok(tmp)
}

fn read_text(base: &SourceBase, relative: &str) -> Result<String> {
    let bytes = read_bytes(base, relative)?;
    String::from_utf8(bytes).map_err(|err| Error::Other(format!("invalid utf-8: {err}")))
}

fn read_bytes(base: &SourceBase, relative: &str) -> Result<Vec<u8>> {
    crate::index::validate_relative_file(relative)?;
    match base {
        SourceBase::Dir(dir) => {
            let path = dir.join(relative);
            fs::read(&path).map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })
        }
        SourceBase::Http(url) => {
            let joined = url
                .join(relative)
                .map_err(|err| Error::Other(format!("join url: {err}")))?;
            // Same-origin relative join only; reject if host/scheme changed unexpectedly.
            if joined.scheme() != url.scheme() || joined.host_str() != url.host_str() {
                return Err(Error::Other(format!(
                    "refusing cross-origin fetch for {relative}"
                )));
            }
            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|err| Error::Http {
                    url: joined.to_string(),
                    message: err.to_string(),
                })?;
            let response = client
                .get(joined.clone())
                .send()
                .map_err(|err| Error::Http {
                    url: joined.to_string(),
                    message: err.to_string(),
                })?;
            let status = response.status();
            if !status.is_success() {
                return Err(Error::Http {
                    url: joined.to_string(),
                    message: format!("HTTP {status}"),
                });
            }
            response
                .bytes()
                .map(|b| b.to_vec())
                .map_err(|err| Error::Http {
                    url: joined.to_string(),
                    message: err.to_string(),
                })
        }
    }
}
