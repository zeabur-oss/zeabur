//! A directory of downloads named by their SHA-256, laid out as an OCI image layout so `oras`,
//! `crane` and `skopeo` can read it.
//!
//! `zeabur-installer bundle` fills it ([`record`]) with every download that is checked against a
//! digest, and with the SHA-256 of each file that has none ([`note`]); [`finish`] writes
//! `index.json`. A later run reads it ([`open`]) before it downloads, and checks each file again.
//! A digest the installer has built in always wins over one recorded here.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use crate::HostError;

static BUNDLE: RwLock<Option<Bundle>> = RwLock::new(None);

const ARTIFACT_TYPE: &str = "application/vnd.zeabur.bundle.file.v1";
const MANIFEST: &str = "application/vnd.oci.image.manifest.v1+json";
const EMPTY: &str = "application/vnd.oci.empty.v1+json";
const TITLE: &str = "org.opencontainers.image.title";
const SOURCE: &str = "com.zeabur.bundle.source-url";
const FILE_DIGEST: &str = "com.zeabur.bundle.sha256";
const PINNED: &str = "com.zeabur.bundle.pinned";

#[derive(Clone)]
struct Bundle {
    dir: PathBuf,
    record: bool,
    entries: BTreeMap<String, Entry>,
}

#[derive(Clone)]
struct Entry {
    digest: String,
    size: u64,
    pinned: bool,
}

fn current() -> Option<Bundle> {
    BUNDLE.read().ok()?.clone()
}

/// Reads downloads from `dir` for the rest of the run, unless the run is recording.
pub fn open(dir: &Path) {
    if let Ok(mut bundle) = BUNDLE.write()
        && !bundle.as_ref().is_some_and(|b| b.record)
    {
        *bundle = Some(Bundle {
            dir: dir.to_owned(),
            record: false,
            entries: BTreeMap::new(),
        });
    }
}

/// Keeps every download that is checked against a digest in `dir` for the rest of the run.
///
/// # Errors
///
/// When the directory cannot be made.
pub fn record(dir: &Path) -> Result<(), HostError> {
    let blobs = dir.join("blobs").join("sha256");
    fs::create_dir_all(&blobs).map_err(HostError::io(&blobs))?;
    let layout = dir.join("oci-layout");
    fs::write(&layout, "{\"imageLayoutVersion\":\"1.0.0\"}\n").map_err(HostError::io(&layout))?;
    if let Ok(mut bundle) = BUNDLE.write() {
        *bundle = Some(Bundle {
            dir: dir.to_owned(),
            record: true,
            entries: BTreeMap::new(),
        });
    }
    Ok(())
}

/// Where a file with `digest` (`sha256:<hex>` or bare hex) is kept.
fn path(bundle: &Bundle, digest: &str) -> Option<PathBuf> {
    let hex = digest.strip_prefix("sha256:").unwrap_or(digest);
    let valid = hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit());
    valid.then(|| bundle.dir.join("blobs").join("sha256").join(hex))
}

/// The file with `digest`, when the bundle is open and holds it. Its content is not checked
/// here.
#[must_use]
pub fn find(digest: &str) -> Option<PathBuf> {
    let bundle = current().filter(|b| !b.record)?;
    path(&bundle, digest).filter(|p| p.is_file())
}

/// The bytes of the file with `digest`, when the bundle is open and holds it. The caller checks
/// them.
#[must_use]
pub fn read(digest: &str) -> Option<Vec<u8>> {
    fs::read(find(digest)?).ok()
}

/// The digest recorded for `url` when the bundle was made, for a file the installer has no digest
/// of its own for. Only when the bundle is open.
#[must_use]
pub fn recorded(url: &str) -> Option<String> {
    let bundle = current().filter(|b| !b.record)?;
    let index = fs::read(bundle.dir.join("index.json")).ok()?;
    let index: Value = serde_json::from_slice(&index).ok()?;
    for item in index.get("manifests")?.as_array()? {
        let notes = item.get("annotations")?;
        if notes.get(SOURCE).and_then(Value::as_str) == Some(url)
            && notes.get(PINNED).and_then(Value::as_str) == Some("false")
        {
            return notes.get(FILE_DIGEST)?.as_str().map(str::to_owned);
        }
    }
    None
}

/// Keeps `bytes` under `digest` when recording.
///
/// # Errors
///
/// When the file cannot be written.
pub fn save(digest: &str, bytes: &[u8]) -> Result<(), HostError> {
    let Some(bundle) = current().filter(|b| b.record) else {
        return Ok(());
    };
    let Some(target) = path(&bundle, digest) else {
        return Ok(());
    };
    fs::write(&target, bytes).map_err(HostError::io(&target))
}

/// Keeps the file at `source` under `digest` when recording, as a hard link when it can.
///
/// # Errors
///
/// When the file cannot be kept.
pub fn save_file(digest: &str, source: &Path) -> Result<(), HostError> {
    let Some(bundle) = current().filter(|b| b.record) else {
        return Ok(());
    };
    let Some(target) = path(&bundle, digest) else {
        return Ok(());
    };
    if target.exists() {
        return Ok(());
    }
    if fs::hard_link(source, &target).is_err() {
        fs::copy(source, &target).map_err(HostError::io(&target))?;
    }
    Ok(())
}

/// Lists the file kept for `url` in `index.json`. `pinned` is false when the installer had no
/// digest of its own and this is the SHA-256 of what was downloaded. Only when recording.
pub fn note(url: &str, digest: &str, size: u64, pinned: bool) {
    if let Ok(mut bundle) = BUNDLE.write()
        && let Some(bundle) = bundle.as_mut().filter(|b| b.record)
    {
        bundle.entries.insert(
            url.to_owned(),
            Entry {
                digest: digest.strip_prefix("sha256:").unwrap_or(digest).to_owned(),
                size,
                pinned,
            },
        );
    }
}

/// The URLs noted without a digest of the installer's own, which the bundle vouches for only as
/// "what was there when it was made".
#[must_use]
pub fn recorded_urls() -> Vec<String> {
    let Some(bundle) = current().filter(|b| b.record) else {
        return Vec::new();
    };
    let mut urls = Vec::new();
    for (url, entry) in &bundle.entries {
        if !entry.pinned {
            urls.push(url.clone());
        }
    }
    urls
}

fn keep_json(bundle: &Bundle, value: &Value) -> Result<(String, u64), HostError> {
    let bytes = serde_json::to_vec(value).map_err(|e| HostError::Parse {
        what: "bundle index".to_owned(),
        reason: e.to_string(),
    })?;
    let digest = hex::encode(Sha256::digest(&bytes));
    let target = path(bundle, &digest).ok_or_else(|| HostError::Parse {
        what: "bundle index".to_owned(),
        reason: "bad digest".to_owned(),
    })?;
    fs::write(&target, &bytes).map_err(HostError::io(&target))?;
    Ok((digest, bytes.len() as u64))
}

/// Writes `index.json`: one manifest per noted file, each with the file as its only layer.
///
/// # Errors
///
/// When a file cannot be written.
pub fn finish() -> Result<(), HostError> {
    let Some(bundle) = current().filter(|b| b.record) else {
        return Ok(());
    };
    let (empty, empty_size) = keep_json(&bundle, &json!({}))?;
    let mut manifests = Vec::new();
    for (url, entry) in &bundle.entries {
        let title = url.rsplit('/').next().unwrap_or(url);
        let manifest = json!({
            "schemaVersion": 2,
            "mediaType": MANIFEST,
            "artifactType": ARTIFACT_TYPE,
            "config": {"mediaType": EMPTY, "digest": format!("sha256:{empty}"), "size": empty_size},
            "layers": [{
                "mediaType": "application/octet-stream",
                "digest": format!("sha256:{}", entry.digest),
                "size": entry.size,
                "annotations": {TITLE: title},
            }],
        });
        let (digest, size) = keep_json(&bundle, &manifest)?;
        manifests.push(json!({
            "mediaType": MANIFEST,
            "digest": format!("sha256:{digest}"),
            "size": size,
            "artifactType": ARTIFACT_TYPE,
            "annotations": {
                TITLE: title,
                SOURCE: url,
                FILE_DIGEST: entry.digest,
                PINNED: entry.pinned.to_string(),
            },
        }));
    }
    let index = json!({"schemaVersion": 2, "mediaType": "application/vnd.oci.image.index.v1+json", "manifests": manifests});
    let target = bundle.dir.join("index.json");
    let bytes = serde_json::to_vec_pretty(&index).map_err(|e| HostError::Parse {
        what: "index.json".to_owned(),
        reason: e.to_string(),
    })?;
    fs::write(&target, bytes).map_err(HostError::io(&target))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_with_or_without_the_prefix_name_the_same_file() {
        let bundle = Bundle {
            dir: PathBuf::from("/b"),
            record: false,
            entries: BTreeMap::new(),
        };
        let hex = "a".repeat(64);
        assert_eq!(
            path(&bundle, &hex),
            Some(PathBuf::from(format!("/b/blobs/sha256/{hex}")))
        );
        assert_eq!(path(&bundle, &hex), path(&bundle, &format!("sha256:{hex}")));
    }

    #[test]
    fn a_digest_that_is_not_hex_names_no_file() {
        let bundle = Bundle {
            dir: PathBuf::from("/b"),
            record: false,
            entries: BTreeMap::new(),
        };
        assert_eq!(path(&bundle, "../../etc/passwd"), None);
        assert_eq!(path(&bundle, &"g".repeat(64)), None);
        assert_eq!(path(&bundle, "abc"), None);
    }
}
