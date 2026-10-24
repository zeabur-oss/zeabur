//! Unpacking the RKE2 tarball.

use std::fs::File;
use std::path::Path;

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;

use crate::HostError;

/// Unpacks a `.tar.gz` into `dest`, keeping modes. Entries leaving `dest` are refused by `tar`.
///
/// # Errors
///
/// When the archive cannot be read or a file cannot be written.
pub fn unpack_tar_gz(archive: &Path, dest: &Path) -> Result<(), HostError> {
    let file = File::open(archive).map_err(HostError::io(archive))?;
    let mut tar = tar::Archive::new(GzDecoder::new(file));
    tar.set_preserve_permissions(true);
    tar.set_overwrite(true);
    tar.unpack(dest).map_err(HostError::io(archive))
}

/// Packs a chart directory as `helm package` would lay it out: one top-level directory named after
/// the chart. A `HelmChart`'s `chartContent` takes the result, base64-encoded.
///
/// # Errors
///
/// When the directory cannot be read.
pub fn pack_chart(dir: &Path, name: &str) -> Result<Vec<u8>, HostError> {
    let mut tar = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    tar.follow_symlinks(false);
    tar.append_dir_all(name, dir).map_err(HostError::io(dir))?;
    tar.into_inner()
        .and_then(GzEncoder::finish)
        .map_err(HostError::io(dir))
}

#[cfg(test)]
mod tests {
    use std::io::Read as _;

    use super::*;

    #[test]
    fn charts_pack_under_their_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Chart.yaml"), "name: zeabur\n").unwrap();
        let bytes = pack_chart(dir.path(), "zeabur").unwrap();
        let mut archive = tar::Archive::new(GzDecoder::new(bytes.as_slice()));
        let mut names = Vec::new();
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let mut s = String::new();
            entry.read_to_string(&mut s).unwrap();
            names.push(entry.path().unwrap().display().to_string());
        }
        assert!(names.contains(&"zeabur/Chart.yaml".to_owned()), "{names:?}");
    }
}
