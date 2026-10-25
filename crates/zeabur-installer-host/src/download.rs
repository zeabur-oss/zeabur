//! Downloads checked against a SHA-256 digest.

use std::fs::{self, File};
use std::io::{self, BufRead as _, Read as _, Write as _};
use std::path::Path;
use std::time::Duration;

use sha2::{Digest as _, Sha256};
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig};

use crate::HostError;

/// The largest file [`fetch`] downloads.
const MAX_FILE: u64 = 4 << 30;

/// An HTTPS client trusting the system's CA store.
#[must_use]
pub fn agent() -> Agent {
    // Fails only when a provider is already installed, which is as good.
    drop(rustls::crypto::aws_lc_rs::default_provider().install_default());
    Agent::config_builder()
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_recv_response(Some(Duration::from_mins(1)))
        .timeout_recv_body(Some(Duration::from_hours(1)))
        .user_agent(concat!("zeabur-installer/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// Downloads `url` to `dest` unless a file with the expected digest is already there. Returns the
/// digest. With no expected digest the file is always downloaded; check it with [`sha256_file`]
/// against a list.
///
/// # Errors
///
/// When the download fails, is larger than 4 GiB or the digest differs; a wrong file is removed.
pub fn fetch(
    agent: &Agent,
    url: &str,
    dest: &Path,
    sha256: Option<&str>,
) -> Result<String, HostError> {
    if let Some(expected) = sha256
        && dest.exists()
        && sha256_file(dest)? == expected
    {
        return Ok(expected.to_owned());
    }
    let fail = |reason: String| HostError::Download {
        url: url.to_owned(),
        reason,
    };
    let dir = dest.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).map_err(HostError::io(dir))?;
    let partial = dest.with_extension("partial");
    let response = agent.get(url).call().map_err(|e| fail(e.to_string()))?;
    let mut reader = response
        .into_body()
        .into_with_config()
        .limit(MAX_FILE)
        .reader();
    let mut file = File::create(&partial).map_err(HostError::io(&partial))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1 << 16];
    loop {
        let n = reader.read(&mut buffer).map_err(|e| fail(e.to_string()))?;
        let Some(chunk) = buffer.get(..n) else { break };
        if chunk.is_empty() {
            break;
        }
        hasher.update(chunk);
        file.write_all(chunk).map_err(HostError::io(&partial))?;
    }
    file.sync_all().map_err(HostError::io(&partial))?;
    let actual = hex::encode(hasher.finalize());
    if let Some(expected) = sha256
        && actual != expected
    {
        drop(fs::remove_file(&partial));
        return Err(HostError::Digest {
            file: file_name(dest),
            expected: expected.to_owned(),
            actual,
        });
    }
    fs::rename(&partial, dest).map_err(HostError::io(dest))?;
    Ok(actual)
}

/// Downloads a text document, such as a manifest, of at most 64 MiB, and checks it against
/// `sha256` (`sha256:<hex>`) when one is given.
///
/// # Errors
///
/// When the download fails or the digest differs.
pub fn fetch_text(agent: &Agent, url: &str, sha256: Option<&str>) -> Result<String, HostError> {
    let fail = |reason: String| HostError::Download {
        url: url.to_owned(),
        reason,
    };
    let response = agent.get(url).call().map_err(|e| fail(e.to_string()))?;
    let text = response
        .into_body()
        .with_config()
        .limit(64 << 20)
        .read_to_string()
        .map_err(|e| fail(e.to_string()))?;
    if let Some(expected) = sha256 {
        check(url, expected, text.as_bytes())?;
    }
    Ok(text)
}

/// Checks `bytes` against `expected`, `sha256:<hex>` or bare hex.
///
/// # Errors
///
/// When the digest differs.
pub fn check(what: &str, expected: &str, bytes: &[u8]) -> Result<(), HostError> {
    let expected = expected.strip_prefix("sha256:").unwrap_or(expected);
    let actual = hex::encode(Sha256::digest(bytes));
    if actual == expected {
        return Ok(());
    }
    Err(HostError::Digest {
        file: what.to_owned(),
        expected: expected.to_owned(),
        actual,
    })
}

/// The SHA-256 of a file.
///
/// # Errors
///
/// When it cannot be read.
pub fn sha256_file(path: &Path) -> Result<String, HostError> {
    let mut file = io::BufReader::new(File::open(path).map_err(HostError::io(path))?);
    let mut hasher = Sha256::new();
    loop {
        let chunk = file.fill_buf().map_err(HostError::io(path))?;
        if chunk.is_empty() {
            break;
        }
        hasher.update(chunk);
        let n = chunk.len();
        file.consume(n);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// The digest of `file` in a `sha256sum` list.
#[must_use]
pub fn digest_in_list<'a>(list: &'a str, file: &str) -> Option<&'a str> {
    list.lines().find_map(|line| {
        let (digest, name) = line.split_once(char::is_whitespace)?;
        (name.trim_start().trim_start_matches('*') == file).then_some(digest)
    })
}

/// Checks a downloaded file against a `sha256sum` list, removing it when it differs.
///
/// # Errors
///
/// When the list does not name it or the digest differs.
pub fn check_against_list(list: &str, path: &Path, actual: &str) -> Result<(), HostError> {
    let file = file_name(path);
    let expected = digest_in_list(list, &file).ok_or_else(|| HostError::Parse {
        what: "the release's checksum list".to_owned(),
        reason: format!("no {file}"),
    })?;
    if expected == actual {
        return Ok(());
    }
    drop(fs::remove_file(path));
    Err(HostError::Digest {
        file,
        expected: expected.to_owned(),
        actual: actual.to_owned(),
    })
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_lists() {
        let list = "aa  rke2.linux-arm64.tar.gz\nbb *rke2-images-core.linux-arm64.tar.zst\n";
        assert_eq!(digest_in_list(list, "rke2.linux-arm64.tar.gz"), Some("aa"));
        assert_eq!(
            digest_in_list(list, "rke2-images-core.linux-arm64.tar.zst"),
            Some("bb")
        );
        assert_eq!(digest_in_list(list, "rke2.linux-amd64.tar.gz"), None);
    }

    #[test]
    fn file_digest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
