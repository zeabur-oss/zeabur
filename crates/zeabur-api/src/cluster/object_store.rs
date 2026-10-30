//! Downloads of a Velero backup, which Zeabur streams to the browser so that no signed URL, key or
//! bucket address reaches the page.
//!
//! Velero signs a URL for a backup's contents [velero-download]. A location that encrypts with a
//! customer key (SSE-C) signs the key's headers too, so the request must send them: the algorithm,
//! the key in base64 and its MD5 digest in base64 [s3-ssec]. Zeabur reads the key from Velero's
//! Secret as the person.
//!
//! A URL is fetched only when its host is the location's endpoint, so a changed status cannot
//! send Zeabur's requests elsewhere, and only at addresses the server may reach
//! ([`https::Reach`]).
//!
//! [velero-download]: https://velero.io/docs/v1.17/api-types/downloadrequest/
//! [s3-ssec]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/ServerSideEncryptionCustomerKeys.html

use std::time::Duration;

use axum::http::{self, HeaderName, HeaderValue, Request, Response};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use bytes::Bytes;
use http_body_util::Empty;
use hyper_util::client::legacy::Client;

use crate::images::https::{self, Connector, IDLE, Tls, connector, describe, pooled, refuse_named};

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
    /// A client that connects where `reach` allows and trusts what `tls` trusts.
    #[must_use]
    pub fn new(reach: https::Reach, tls: &Tls) -> Self {
        let client = tls
            .as_ref()
            .map(|tls| pooled(connector(reach, tls), IDLE))
            .map_err(Clone::clone);
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
/// for (`asked`: "for the backup's contents").
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

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

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
            let hex = md5(text.as_bytes())
                .iter()
                .fold(String::new(), |mut out, b| {
                    write!(out, "{b:02x}").unwrap_or_default();
                    out
                });
            assert_eq!(hex, digest, "{text}");
        }
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

    /// The extra CA reaches the store's client, and it does not open an address the reach
    /// refuses.
    #[tokio::test]
    async fn the_store_trusts_the_extra_ca_and_still_refuses_link_local_addresses() {
        let broken = Fetcher::new(
            https::Reach::Public,
            &https::tls(Some(b"not a certificate")),
        );
        let error = broken.get("https://s3.example.com/b/k", Vec::new()).await;
        assert!(
            error.as_ref().is_err_and(|e| e.contains("PEM")),
            "{:?}",
            error.map(|r| r.status())
        );
        let store = Fetcher::new(https::Reach::Public, &https::tls(None));
        let refused = store.get("https://169.254.169.254/b/k", Vec::new()).await;
        assert!(
            refused
                .as_ref()
                .is_err_and(|e| e.contains("169.254.169.254")),
            "{:?}",
            refused.map(|r| r.status())
        );
    }

    async fn fetch_over_tls(extra_ca: Option<&str>) -> Result<u16, String> {
        let address = zeabur_test_support::tls::serve().await;
        let store = Fetcher::new(https::Reach::Any, &https::tls(extra_ca.map(str::as_bytes)));
        store
            .get(&format!("https://{address}/b/k"), Vec::new())
            .await
            .map(|response| response.status().as_u16())
    }

    #[tokio::test]
    async fn the_store_trusts_only_the_trusted_ca_that_signed_the_certificate() {
        use zeabur_test_support::tls::{CA_A, CA_B};

        assert_eq!(fetch_over_tls(Some(CA_A)).await, Ok(200));
        assert!(fetch_over_tls(Some(CA_B)).await.is_err());
        assert!(fetch_over_tls(None).await.is_err());
    }
}
