//! Downloads checked against a SHA-256 digest.

use std::fs::{self, File};
use std::io::{self, BufRead as _, Read as _, Write as _};
use std::path::Path;
use std::thread;
use std::time::{Duration, SystemTime};

use sha2::{Digest as _, Sha256};
use ureq::http::Response;
use ureq::tls::{RootCerts, TlsConfig};
use ureq::{Agent, Body};

use crate::HostError;

/// The largest file [`fetch`] downloads.
const MAX_FILE: u64 = 4 << 30;

/// How often a failed request is tried again.
const RETRIES: u32 = 3;

/// The wait before the first retry; each further one doubles it.
const BACKOFF: Duration = if cfg!(test) {
    Duration::from_millis(1)
} else {
    Duration::from_secs(1)
};

/// The longest `Retry-After` wait honoured; a longer one ends the retrying.
const MAX_WAIT: Duration = Duration::from_secs(30);

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
    let response = get_ok(agent, url).map_err(fail)?;
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
    let response = get_ok(agent, url).map_err(fail)?;
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

/// A GET that succeeds, or the reason it did not.
fn get_ok(agent: &Agent, url: &str) -> Result<Response<Body>, String> {
    let response = get_retrying(agent, url, &[], &[]).map_err(|e| e.to_string())?;
    if response.status().is_success() {
        Ok(response)
    } else {
        Err(format!("HTTP {}", response.status()))
    }
}

/// A GET that is tried again, up to three times with a doubling wait, after a connection error,
/// a timeout or a status that RFC 9110 and RFC 6585 describe as temporary (see [`temporary`]).
/// A `Retry-After` answer sets the wait; one longer than [`MAX_WAIT`] is not waited for and the
/// response is returned. Any other status is returned as it is for the caller to judge.
pub(crate) fn get_retrying(
    agent: &Agent,
    url: &str,
    headers: &[(&str, &str)],
    query: &[(&str, &str)],
) -> Result<Response<Body>, ureq::Error> {
    let mut wait = BACKOFF;
    let mut attempt = 0;
    loop {
        let mut request = agent.get(url).config().http_status_as_error(false).build();
        for (key, value) in headers {
            request = request.header(*key, *value);
        }
        for (key, value) in query {
            request = request.query(*key, *value);
        }
        let asked = match request.call() {
            Ok(response) => {
                if !temporary(response.status().as_u16()) || attempt == RETRIES {
                    return Ok(response);
                }
                let asked = retry_after(&response, SystemTime::now());
                if asked.is_some_and(|a| a > MAX_WAIT) {
                    return Ok(response);
                }
                asked
            }
            Err(e) => {
                let transient = matches!(
                    e,
                    ureq::Error::Io(_) | ureq::Error::ConnectionFailed | ureq::Error::Timeout(_)
                );
                if !transient || attempt == RETRIES {
                    return Err(e);
                }
                None
            }
        };
        attempt += 1;
        thread::sleep(asked.unwrap_or(wait));
        wait *= 2;
    }
}

/// Whether the status says the same request may work later: 408 (RFC 9110 section 15.5.9),
/// 429 (RFC 6585 section 4), and the gateway and availability errors 502, 503 and 504
/// (RFC 9110 sections 15.6.3 to 15.6.5). 500, 501 and 505 describe a fault that waiting does
/// not fix.
const fn temporary(status: u16) -> bool {
    matches!(status, 408 | 429 | 502 | 503 | 504)
}

/// The wait a `Retry-After` header asks for, as `delay-seconds` or an `HTTP-date`
/// (RFC 9110 section 10.2.3). A date in the past means no wait.
fn retry_after(response: &Response<Body>, now: SystemTime) -> Option<Duration> {
    let value = response.headers().get("retry-after")?.to_str().ok()?.trim();
    if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
        return value.parse().ok().map(Duration::from_secs);
    }
    let date = httpdate::parse_http_date(value).ok()?;
    Some(date.duration_since(now).unwrap_or_default())
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
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// Serves one status per request, the last one for all later requests. Returns the URL and
    /// the number of requests seen.
    fn serve(statuses: &'static [u16], body: &'static str) -> (String, Arc<AtomicUsize>) {
        serve_with(statuses, "", body)
    }

    /// Like [`serve`], with `extra` header lines (each ending in CRLF) on every non-200 reply.
    fn serve_with(
        statuses: &'static [u16],
        extra: &'static str,
        body: &'static str,
    ) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/file", listener.local_addr().unwrap());
        let seen = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&seen);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut request = [0_u8; 4096];
                drop(stream.read(&mut request));
                let n = count.fetch_add(1, Ordering::SeqCst);
                let status = statuses
                    .get(n)
                    .or_else(|| statuses.last())
                    .copied()
                    .unwrap_or(500);
                let body = if status == 200 { body } else { "" };
                let extra = if status == 200 { "" } else { extra };
                let reply = format!(
                    "HTTP/1.1 {status} X\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                drop(stream.write_all(reply.as_bytes()));
            }
        });
        (url, seen)
    }

    const ABC: &str = "abc";
    const ABC_SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn retries_server_errors_until_success() {
        let (url, seen) = serve(&[503, 503, 200], ABC);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("f");
        let digest = fetch(&agent(), &url, &dest, Some(ABC_SHA)).unwrap();
        assert_eq!(digest, ABC_SHA);
        assert_eq!(fs::read_to_string(&dest).unwrap(), ABC);
        assert_eq!(seen.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn text_is_retried_too() {
        let (url, _) = serve(&[429, 504, 200], ABC);
        assert_eq!(fetch_text(&agent(), &url, None).unwrap(), ABC);
    }

    #[test]
    fn client_errors_fail_at_once() {
        let (url, seen) = serve(&[404], "");
        let dir = tempfile::tempdir().unwrap();
        let err = fetch(&agent(), &url, &dir.path().join("f"), None).unwrap_err();
        assert!(err.to_string().contains("404"), "{err}");
        assert_eq!(seen.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn gives_up_after_the_limit() {
        let (url, seen) = serve(&[503], "");
        let err = fetch_text(&agent(), &url, None).unwrap_err();
        assert!(err.to_string().contains("503"), "{err}");
        assert_eq!(seen.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn only_temporary_statuses_are_retried() {
        let (url, seen) = serve(&[408, 502, 200], ABC);
        assert_eq!(fetch_text(&agent(), &url, None).unwrap(), ABC);
        assert_eq!(seen.load(Ordering::SeqCst), 3);
        let fixed: [&'static [u16]; 3] = [&[500, 200], &[501, 200], &[505, 200]];
        for statuses in fixed {
            let (url, seen) = serve(statuses, ABC);
            assert!(fetch_text(&agent(), &url, None).is_err(), "{statuses:?}");
            assert_eq!(seen.load(Ordering::SeqCst), 1, "{statuses:?}");
        }
    }

    #[test]
    fn retry_after_is_honoured_as_seconds_or_a_date() {
        let (url, seen) = serve_with(&[503, 200], "Retry-After: 0\r\n", ABC);
        assert_eq!(fetch_text(&agent(), &url, None).unwrap(), ABC);
        assert_eq!(seen.load(Ordering::SeqCst), 2);
        let (url, seen) = serve_with(
            &[429, 200],
            "Retry-After: Fri, 31 Dec 1999 23:59:59 GMT\r\n",
            ABC,
        );
        assert_eq!(fetch_text(&agent(), &url, None).unwrap(), ABC);
        assert_eq!(seen.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_retry_after_beyond_the_cap_is_not_waited_for() {
        let (url, seen) = serve_with(&[503, 200], "Retry-After: 3600\r\n", ABC);
        let err = fetch_text(&agent(), &url, None).unwrap_err();
        assert!(err.to_string().contains("503"), "{err}");
        assert_eq!(seen.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn digest_mismatch_is_not_retried() {
        let (url, seen) = serve(&[200], ABC);
        let dir = tempfile::tempdir().unwrap();
        let wrong = "0".repeat(64);
        let err = fetch(&agent(), &url, &dir.path().join("f"), Some(&wrong)).unwrap_err();
        assert!(matches!(err, HostError::Digest { .. }), "{err}");
        assert_eq!(seen.load(Ordering::SeqCst), 1);
    }

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
