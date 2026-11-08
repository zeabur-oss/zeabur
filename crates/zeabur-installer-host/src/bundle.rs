//! A directory of downloads named by their SHA-256, so an install can run where the sources are
//! out of reach.
//!
//! `zeabur-installer bundle` fills it ([`record`]) with every download that is checked against a
//! digest. A later run reads it ([`open`]) before it downloads, and checks each file again.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::HostError;

static BUNDLE: RwLock<Option<Bundle>> = RwLock::new(None);

#[derive(Clone)]
struct Bundle {
    dir: PathBuf,
    record: bool,
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
        });
    }
}

/// Keeps every download that is checked against a digest in `dir` for the rest of the run.
///
/// # Errors
///
/// When the directory cannot be made.
pub fn record(dir: &Path) -> Result<(), HostError> {
    let blobs = dir.join("blobs");
    fs::create_dir_all(&blobs).map_err(HostError::io(&blobs))?;
    if let Ok(mut bundle) = BUNDLE.write() {
        *bundle = Some(Bundle {
            dir: dir.to_owned(),
            record: true,
        });
    }
    Ok(())
}

/// Where a file with `digest` (`sha256:<hex>` or bare hex) is kept.
fn path(bundle: &Bundle, digest: &str) -> Option<PathBuf> {
    let hex = digest.strip_prefix("sha256:").unwrap_or(digest);
    let valid = hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit());
    valid.then(|| bundle.dir.join("blobs").join(format!("sha256-{hex}")))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_with_or_without_the_prefix_name_the_same_file() {
        let bundle = Bundle {
            dir: PathBuf::from("/b"),
            record: false,
        };
        let hex = "a".repeat(64);
        assert_eq!(
            path(&bundle, &hex),
            Some(PathBuf::from(format!("/b/blobs/sha256-{hex}")))
        );
        assert_eq!(path(&bundle, &hex), path(&bundle, &format!("sha256:{hex}")));
    }

    #[test]
    fn a_digest_that_is_not_hex_names_no_file() {
        let bundle = Bundle {
            dir: PathBuf::from("/b"),
            record: false,
        };
        assert_eq!(path(&bundle, "../../etc/passwd"), None);
        assert_eq!(path(&bundle, &"g".repeat(64)), None);
        assert_eq!(path(&bundle, "abc"), None);
    }
}
