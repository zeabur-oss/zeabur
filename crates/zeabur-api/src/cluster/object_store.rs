//! Downloads from a backup bucket, which Zeabur streams to the browser so that no signed URL,
//! key or bucket address reaches the page.
//!
//! - **Velero** signs a URL for a backup's contents [velero-download]. A location that encrypts
//!   with a customer key (SSE-C) signs the key's headers too, so the request must send them: the
//!   algorithm, the key in base64 and its MD5 digest in base64 [s3-ssec]. Zeabur reads the key from
//!   Velero's Secret as the person.
//! - **K8up** writes an export to the bucket of the volume's backups. Zeabur signs the requests
//!   with the keys of that bucket, read from the backup's Secret as the person, with AWS Signature
//!   Version 4 in the query string [s3-sigv4-query], valid for `SIGNED_FOR` seconds.
//!
//! A URL is fetched only when its host is the location's endpoint, so a changed status cannot
//! send Zeabur's requests elsewhere, and only at addresses the server may reach
//! ([`https::Reach`]).
//!
//! [velero-download]: https://velero.io/docs/v1.17/api-types/downloadrequest/
//! [s3-ssec]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/ServerSideEncryptionCustomerKeys.html
//! [s3-sigv4-query]: https://docs.aws.amazon.com/AmazonS3/latest/API/sigv4-query-string-auth.html

use std::fmt::Write as _;
use std::time::Duration;

use axum::http::{self, HeaderName, HeaderValue, Request, Response};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use bytes::Bytes;
use http_body_util::Empty;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use zeabur_kube::k8s_openapi::jiff::Timestamp;

use crate::images::https::{self, Connector, connector, describe, refuse_named};

/// How long a URL Zeabur signs stays valid.
pub(crate) const SIGNED_FOR: u32 = 60;
/// How long the store may take to start answering.
const FIRST_BYTE: Duration = Duration::from_secs(30);

/// A client for backup buckets.
#[derive(Clone)]
pub struct Fetcher {
    client: Result<Client<Connector, Empty<Bytes>>, String>,
    reach: https::Reach,
}

impl std::fmt::Debug for Fetcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fetcher")
            .field("reach", &self.reach)
            .finish_non_exhaustive()
    }
}

impl Fetcher {
    /// A client that connects where `reach` allows.
    #[must_use]
    pub fn new(reach: https::Reach) -> Self {
        let client = connector(reach, None).map(|c| Client::builder(TokioExecutor::new()).build(c));
        Self { client, reach }
    }

    /// `GET url` with `headers`, answered once the store sends its status line.
    pub(crate) async fn get(
        &self,
        url: &str,
        headers: Vec<(HeaderName, HeaderValue)>,
    ) -> Result<Response<axum::body::Body>, String> {
        let client = self.client.as_ref().map_err(Clone::clone)?;
        let uri: http::Uri = url.parse().map_err(|e| format!("{e}"))?;
        refuse_named(self.reach, &uri)?;
        let mut request = Request::get(uri).header(http::header::USER_AGENT, "zeabur");
        for (name, value) in headers {
            request = request.header(name, value);
        }
        let request = request.body(Empty::new()).map_err(|e| e.to_string())?;
        tokio::time::timeout(FIRST_BYTE, client.request(request))
            .await
            .map_err(|_elapsed| "the store did not answer in time".to_owned())?
            .map(|response| response.map(axum::body::Body::new))
            .map_err(|e| describe(&e))
    }
}

/// The host and port of `url`, lowercase.
pub(crate) fn authority(url: &str) -> Option<String> {
    let uri: http::Uri = url.parse().ok()?;
    let host = uri.host()?.to_ascii_lowercase();
    Some(match uri.port_u16() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    })
}

/// The headers that send the customer key `key` with a request (SSE-C) [s3-ssec].
///
/// [s3-ssec]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/ServerSideEncryptionCustomerKeys.html
pub(crate) fn customer_key_headers(key: &[u8]) -> Vec<(HeaderName, HeaderValue)> {
    let sensitive = |value: String| {
        let mut value = HeaderValue::from_str(&value).unwrap_or(HeaderValue::from_static(""));
        value.set_sensitive(true);
        value
    };
    vec![
        (
            HeaderName::from_static("x-amz-server-side-encryption-customer-algorithm"),
            HeaderValue::from_static("AES256"),
        ),
        (
            HeaderName::from_static("x-amz-server-side-encryption-customer-key"),
            sensitive(STANDARD.encode(key)),
        ),
        (
            HeaderName::from_static("x-amz-server-side-encryption-customer-key-md5"),
            sensitive(STANDARD.encode(md5(key))),
        ),
    ]
}

/// The body at `url`, fetched with `headers`, or a problem naming `place` and what was asked
/// for (`asked`: "for the exported file").
pub(crate) async fn fetched(
    fetcher: &Fetcher,
    url: &str,
    headers: Vec<(HeaderName, HeaderValue)>,
    place: &str,
    asked: &str,
) -> Result<axum::body::Body, crate::error::ApiError> {
    let unavailable = |detail: String| {
        crate::error::ApiError::from(zeabur_core::problem::Problem::new(
            zeabur_core::problem::Kind::Unavailable,
            detail,
        ))
    };
    let answer = fetcher
        .get(url, headers)
        .await
        .map_err(|e| unavailable(format!("{place} could not be reached: {e}")))?;
    if !answer.status().is_success() {
        return Err(unavailable(format!(
            "{place} answered {} {asked}.",
            answer.status()
        )));
    }
    Ok(answer.into_body())
}

/// The MD5 digest of `data` (RFC 1321). S3 takes it as the customer key's checksum; it protects
/// nothing.
#[must_use]
pub(crate) fn md5(data: &[u8]) -> [u8; 16] {
    const SHIFTS: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let table: [u32; 64] = std::array::from_fn(|i| {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "RFC 1321 defines T[i] as the integer part of 2^32 * |sin(i + 1)|"
        )]
        let value = ((i as f64 + 1.0).sin().abs() * 4_294_967_296.0) as u32;
        value
    });
    let mut message = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bits.to_le_bytes());
    let mut state: [u32; 4] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476];
    for block in message.as_chunks::<64>().0 {
        let words: Vec<u32> = block
            .as_chunks::<4>()
            .0
            .iter()
            .map(|w| u32::from_le_bytes(*w))
            .collect();
        let [mut first, mut second, mut third, mut fourth] = state;
        for (round, (shift, constant)) in SHIFTS.iter().zip(table).enumerate() {
            let (mix, index) = match round / 16 {
                0 => ((second & third) | (!second & fourth), round),
                1 => ((fourth & second) | (!fourth & third), (5 * round + 1) % 16),
                2 => (second ^ third ^ fourth, (3 * round + 5) % 16),
                _ => (third ^ (second | !fourth), (7 * round) % 16),
            };
            let word = words.get(index).copied().unwrap_or_default();
            let rotated = first
                .wrapping_add(mix)
                .wrapping_add(constant)
                .wrapping_add(word)
                .rotate_left(*shift);
            (first, second, third, fourth) = (fourth, second.wrapping_add(rotated), second, third);
        }
        for (word, add) in state.iter_mut().zip([first, second, third, fourth]) {
            *word = word.wrapping_add(add);
        }
    }
    let mut out = [0_u8; 16];
    for (chunk, word) in out.as_chunks_mut::<4>().0.iter_mut().zip(state) {
        *chunk = word.to_le_bytes();
    }
    out
}

/// Keys of a bucket.
pub(crate) struct Keys<'a> {
    /// The access key ID.
    pub(crate) access: &'a str,
    /// The secret access key.
    pub(crate) secret: &'a str,
    /// The region; `us-east-1` when empty.
    pub(crate) region: &'a str,
}

fn encode(text: &str, slash: bool) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(byte));
            }
            b'/' if !slash => out.push('/'),
            _ => {
                write!(out, "%{byte:02X}").unwrap_or_default();
            }
        }
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, b| {
        write!(out, "{b:02x}").unwrap_or_default();
        out
    })
}

/// A `GET` URL of `path` (`/<bucket>/<key>`, path-style) at `endpoint` with `query`, signed with
/// `keys` at `now` for [`SIGNED_FOR`] seconds [s3-sigv4-query].
///
/// [s3-sigv4-query]: https://docs.aws.amazon.com/AmazonS3/latest/API/sigv4-query-string-auth.html
pub(crate) fn presigned_get(
    endpoint: &str,
    path: &str,
    query: &[(&str, &str)],
    keys: &Keys<'_>,
    now: Timestamp,
) -> Option<String> {
    presigned(endpoint, path, query, keys, now, SIGNED_FOR)
}

fn presigned(
    endpoint: &str,
    path: &str,
    query: &[(&str, &str)],
    keys: &Keys<'_>,
    now: Timestamp,
    seconds: u32,
) -> Option<String> {
    let endpoint = endpoint.trim_end_matches('/');
    let host = authority(endpoint)?;
    let region = if keys.region.is_empty() {
        "us-east-1"
    } else {
        keys.region
    };
    let time = now.strftime("%Y%m%dT%H%M%SZ").to_string();
    let date = time.get(..8).unwrap_or_default();
    let scope = format!("{date}/{region}/s3/aws4_request");
    let credential = format!("{}/{scope}", keys.access);
    let expires = seconds.to_string();
    let mut pairs: Vec<(String, String)> = query
        .iter()
        .map(|(k, v)| (encode(k, true), encode(v, true)))
        .collect();
    for (k, v) in [
        ("X-Amz-Algorithm", "AWS4-HMAC-SHA256"),
        ("X-Amz-Credential", credential.as_str()),
        ("X-Amz-Date", time.as_str()),
        ("X-Amz-Expires", expires.as_str()),
        ("X-Amz-SignedHeaders", "host"),
    ] {
        pairs.push((encode(k, true), encode(v, true)));
    }
    pairs.sort();
    let canonical_query = pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    let canonical_path = encode(path, false);
    let canonical =
        format!("GET\n{canonical_path}\n{canonical_query}\nhost:{host}\n\nhost\nUNSIGNED-PAYLOAD");
    let to_sign = format!(
        "AWS4-HMAC-SHA256\n{time}\n{scope}\n{}",
        hex(&zeabur_core::crypto::sha256(canonical.as_bytes()))
    );
    let mut key = zeabur_core::crypto::hmac_sha256(
        format!("AWS4{}", keys.secret).as_bytes(),
        date.as_bytes(),
    );
    for part in [region, "s3", "aws4_request"] {
        key = zeabur_core::crypto::hmac_sha256(&key, part.as_bytes());
    }
    let signature = hex(&zeabur_core::crypto::hmac_sha256(&key, to_sign.as_bytes()));
    Some(format!(
        "{endpoint}{canonical_path}?{canonical_query}&X-Amz-Signature={signature}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_matches_the_rfc_1321_test_suite() {
        for (text, digest) in [
            ("", "d41d8cd98f00b204e9800998ecf8427e"),
            ("abc", "900150983cd24fb0d6963f7d28e17f72"),
            (
                "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
                "57edf4a22be3c955ac49da2e2107b67a",
            ),
        ] {
            assert_eq!(hex(&md5(text.as_bytes())), digest, "{text}");
        }
    }

    #[test]
    fn a_presigned_url_matches_the_aws_documentation_example() {
        // The example of "Authenticating Requests: Using Query Parameters (AWS Signature Version
        // 4)".
        let keys = Keys {
            access: "AKIAIOSFODNN7EXAMPLE",
            secret: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            region: "us-east-1",
        };
        let now: Timestamp = "2013-05-24T00:00:00Z".parse().unwrap();
        let url = presigned(
            "https://examplebucket.s3.amazonaws.com",
            "/test.txt",
            &[],
            &keys,
            now,
            86_400,
        )
        .unwrap();
        assert!(
            url.ends_with(
                "X-Amz-Signature=aeeed9bbccd4d02ee5c0109b86d86835f995330da4c265957d157751f604d404"
            ),
            "{url}"
        );
        assert!(!url.contains("wJalrXUtnFEMI"), "{url}");
        let short = presigned_get("https://s3.example.com", "/b/k", &[], &keys, now).unwrap();
        assert!(short.contains("X-Amz-Expires=60"), "{short}");
    }

    #[test]
    fn only_the_endpoints_host_and_port_are_compared() {
        assert_eq!(
            authority("http://SeaweedFS.storage:8333/b/k?x=1").as_deref(),
            Some("seaweedfs.storage:8333")
        );
        assert_eq!(
            authority("https://r2.example.com/b").as_deref(),
            Some("r2.example.com")
        );
    }
}
