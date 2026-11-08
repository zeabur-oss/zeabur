//! A run that records keeps what it downloads under the file's digest.
//!
//! The bundle is process-wide, so this file holds the only test that records.

#[cfg(test)]
mod tests {
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::{fs, thread};

    use sha2::{Digest as _, Sha256};
    use zeabur_installer_host::{bundle, download};

    fn serve(body: &'static [u8]) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
        let address = listener.local_addr().expect("its address");
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut request = [0_u8; 1024];
                drop(stream.read(&mut request));
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                drop(stream.write_all(head.as_bytes()));
                drop(stream.write_all(body));
            }
        });
        format!("http://{address}/file")
    }

    #[test]
    fn a_download_checked_against_a_digest_is_kept() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let body: &'static [u8] = b"the bytes a release serves";
        let digest = hex::encode(Sha256::digest(body));
        let url = serve(body);
        bundle::record(&dir.path().join("bundle")).expect("record into a directory");
        let agent = download::agent();

        let dest = dir.path().join("downloads/file");
        download::fetch(&agent, &url, &dest, Some(&digest)).expect("download");
        let kept = dir.path().join(format!("bundle/blobs/sha256-{digest}"));
        assert_eq!(fs::read(&kept).expect("kept file"), body);

        // A download with no digest to check it against is not kept.
        let other = dir.path().join("downloads/other");
        download::fetch(&agent, &url, &other, None).expect("download without a digest");
        let blobs = dir.path().join("bundle/blobs");
        assert_eq!(fs::read_dir(blobs).expect("blobs").count(), 1);

        // A file already on the host is kept too, so a second run still fills the bundle.
        fs::remove_file(&kept).expect("remove the kept file");
        download::fetch(
            &agent,
            "http://127.0.0.1:9/unreachable",
            &dest,
            Some(&digest),
        )
        .expect("the file on the host");
        assert_eq!(fs::read(&kept).expect("kept again"), body);

        let text = download::fetch_text(&agent, &url, Some(&digest)).expect("text");
        assert_eq!(text.as_bytes(), body);
    }
}
