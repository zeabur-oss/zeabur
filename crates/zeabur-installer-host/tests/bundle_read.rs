//! A run that opens a bundle takes its downloads from it and checks them again.
//!
//! The bundle is process-wide, so this file holds the only test that opens one.

#[cfg(test)]
mod tests {
    use std::fs;

    use sha2::{Digest as _, Sha256};
    use zeabur_installer_host::{HostError, bundle, chart, download};

    fn digest(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    fn keep(blobs: &std::path::Path, bytes: &[u8]) -> String {
        let digest = digest(bytes);
        fs::write(blobs.join(format!("sha256-{digest}")), bytes).expect("write the blob");
        digest
    }

    #[test]
    fn downloads_come_from_the_bundle_and_are_checked() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let blobs = dir.path().join("bundle/blobs");
        fs::create_dir_all(&blobs).expect("blobs directory");

        let file = b"a pinned release file";
        let file_digest = keep(&blobs, file);
        let text = b"kind: ConfigMap\n";
        let text_digest = keep(&blobs, text);
        let archive = b"a chart archive";
        let archive_digest = keep(&blobs, archive);
        let altered = digest(b"what the name promises");
        fs::write(blobs.join(format!("sha256-{altered}")), b"something else")
            .expect("write the altered blob");

        bundle::open(&dir.path().join("bundle"));
        // Nothing listens here, so any download that is attempted fails.
        let url = "http://127.0.0.1:9/unreachable";
        let agent = download::agent();

        let dest = dir.path().join("out/file");
        let got = download::fetch(&agent, url, &dest, Some(&file_digest)).expect("from the bundle");
        assert_eq!(got, file_digest);
        assert_eq!(fs::read(&dest).expect("copied file"), file);

        let got = download::fetch_text(&agent, url, Some(&format!("sha256:{text_digest}")))
            .expect("text from the bundle");
        assert_eq!(got.as_bytes(), text);

        let got = chart::fetch(
            &agent,
            Some("http://127.0.0.1:9/charts"),
            "demo",
            "1.0.0",
            &format!("sha256:{archive_digest}"),
        )
        .expect("chart from the bundle");
        assert_eq!(got, archive);

        let refused = download::fetch(&agent, url, &dir.path().join("out/altered"), Some(&altered))
            .expect_err("a changed file is refused");
        assert!(refused.to_string().contains(&altered), "{refused}");
        assert!(!dir.path().join("out/altered").exists());

        let missing = digest(b"not in the bundle");
        let error = download::fetch(&agent, url, &dir.path().join("out/missing"), Some(&missing))
            .expect_err("a file the bundle lacks is downloaded");
        assert!(matches!(error, HostError::Download { .. }), "{error}");
    }
}
