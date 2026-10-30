//! Unpacking the RKE2 tarball.

use std::fs::File;
use std::io::Read as _;
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

/// The YAML and JSON files in the `crds/` directories of a packaged chart and its subcharts, as
/// one multi-document YAML in path order; empty when there are none. Helm installs these once
/// and never updates them.
///
/// # Errors
///
/// When the archive cannot be read.
pub fn chart_crds(archive: &[u8]) -> Result<Vec<u8>, HostError> {
    let io = |e: std::io::Error| HostError::Parse {
        what: "chart archive".to_owned(),
        reason: e.to_string(),
    };
    let mut files = Vec::new();
    let mut tar = tar::Archive::new(GzDecoder::new(archive));
    for entry in tar.entries().map_err(io)? {
        let mut entry = entry.map_err(io)?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().map_err(io)?.to_string_lossy().into_owned();
        if !is_crd_file(&path) {
            continue;
        }
        let mut text = String::new();
        entry.read_to_string(&mut text).map_err(io)?;
        files.push((path, text));
    }
    files.sort();
    Ok(files
        .into_iter()
        .map(|(_, text)| text)
        .collect::<Vec<_>>()
        .join("\n---\n")
        .into_bytes())
}

/// `<chart>/(charts/<subchart>/)*crds/<file>.yaml|yml|json`.
fn is_crd_file(path: &str) -> bool {
    let mut parts: Vec<&str> = path.split('/').collect();
    let Some(file) = parts.pop() else {
        return false;
    };
    if !["yaml", "yml", "json"]
        .iter()
        .any(|e| file.ends_with(&format!(".{e}")))
    {
        return false;
    }
    let mut rest = parts.iter().skip(1);
    loop {
        match rest.next() {
            Some(&"charts") if rest.next().is_some() => {}
            Some(&"crds") => return true,
            _ => return false,
        }
    }
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn crds_come_from_the_chart_and_its_subcharts_only() {
        let dir = tempfile::tempdir().unwrap();
        for (path, text) in [
            ("Chart.yaml", "name: c\n"),
            ("crds/a.yaml", "kind: A\n"),
            ("crds/notes.txt", "not a manifest\n"),
            ("templates/crds/t.yaml", "kind: Templated\n"),
            ("charts/sub/crds/b.yaml", "kind: B\n"),
        ] {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let bytes = pack_chart(dir.path(), "c").unwrap();
        let crds = String::from_utf8(chart_crds(&bytes).unwrap()).unwrap();
        assert_eq!(crds, "kind: B\n\n---\nkind: A\n");
        let bare = tempfile::tempdir().unwrap();
        std::fs::write(bare.path().join("Chart.yaml"), "name: c\n").unwrap();
        let bytes = pack_chart(bare.path(), "c").unwrap();
        assert!(chart_crds(&bytes).unwrap().is_empty());
    }
}
