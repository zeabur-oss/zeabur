//! Pushing an uploaded folder to the registry as an OCI artifact, the form Zeabur Builder
//! downloads [archive-source].
//!
//! Zeabur streams the request's body to the registry and keeps none of it: the registry takes the
//! empty config blob, the folder's gzip tar as the one layer, and a manifest that names both,
//! written under a tag so that a registry which removes untagged manifests keeps it [oci-push].
//! The token only ever goes to the registry it was issued for: a `Location` that names another
//! origin is refused.
//!
//! # References
//!
//! - [archive-source] Zeabur Builder, "Archive sources," `docs/archive-source.md`.
//! - [oci-push] Open Container Initiative, "Distribution Specification v1.1.1: Pushing blobs and
//!   manifests." <https://github.com/opencontainers/distribution-spec/blob/v1.1.1/spec.md#push>
//!
//! [archive-source]: https://github.com/zeabur-oss/builder/blob/main/docs/archive-source.md
//! [oci-push]: https://github.com/opencontainers/distribution-spec/blob/v1.1.1/spec.md#push

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::http::{self, HeaderMap, HeaderValue, Method, Request, StatusCode, header};
use futures_util::StreamExt as _;
use http_body_util::{BodyExt as _, Limited};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::json;
use sha2::{Digest as _, Sha256};

use crate::images::https::{Connector, Reach, connector, describe, refuse_named};

/// The largest compressed folder, as Zeabur Builder's download allows.
pub(crate) const MAX_LAYER: u64 = 256 * 1024 * 1024;

const ARTIFACT_TYPE: &str = "application/vnd.zeabur.source.v1+tar";
const LAYER_TYPE: &str = "application/vnd.oci.image.layer.v1.tar+gzip";
const EMPTY_TYPE: &str = "application/vnd.oci.empty.v1+json";
const MANIFEST_TYPE: &str = "application/vnd.oci.image.manifest.v1+json";
/// The digest of `{}`, the empty config.
const EMPTY_DIGEST: &str =
    "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a";
const QUICK: Duration = Duration::from_secs(30);
const TRANSFER: Duration = Duration::from_mins(15);
const ANSWER_LIMIT: usize = 64 * 1024;

/// Where and how to push.
pub(crate) struct Target {
    /// `host:port` or a URL; a bare host is reached over HTTPS, except loopback when the server
    /// may reach any address.
    pub(crate) address: String,
    /// `<namespace>/<build>/source`.
    pub(crate) repository: String,
    /// The bearer token of the upload Secret.
    pub(crate) token: SecretString,
}

/// What was pushed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pushed {
    /// The manifest's digest: what the `Build` names.
    pub(crate) digest: String,
    /// The tag it was pushed under.
    pub(crate) tag: String,
}

/// Why the push failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PushError {
    /// The registry refused the token.
    Refused,
    /// The body was not as long as `Content-Length` said, or longer.
    Length,
    /// No answer, or none in time.
    Unreachable(String),
    /// An answer Zeabur cannot use.
    Unexpected(String),
}

/// The registry client of uploads.
#[derive(Clone)]
pub struct Pusher {
    client: Result<Client<Connector, Body>, String>,
    reach: Reach,
}

impl std::fmt::Debug for Pusher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pusher")
            .field("reach", &self.reach)
            .finish_non_exhaustive()
    }
}

/// One request to the registry.
struct Call<'a> {
    method: Method,
    url: &'a str,
    content_type: Option<&'a str>,
    body: Body,
    length: u64,
    deadline: Duration,
}

impl<'a> Call<'a> {
    fn new(method: Method, url: &'a str) -> Self {
        Self {
            method,
            url,
            content_type: None,
            body: Body::empty(),
            length: 0,
            deadline: QUICK,
        }
    }

    fn blob(mut self, body: Body, length: u64) -> Self {
        self.content_type = Some("application/octet-stream");
        self.body = body;
        self.length = length;
        self
    }
}

struct Answer {
    status: StatusCode,
    headers: HeaderMap,
}

impl Pusher {
    /// A client that connects where `reach` allows and trusts what `tls` trusts.
    pub(crate) fn new(reach: Reach, tls: &Result<Arc<rustls::ClientConfig>, String>) -> Self {
        let client = tls
            .as_ref()
            .map(|tls| {
                Client::builder(TokioExecutor::new())
                    .pool_max_idle_per_host(0)
                    .build(connector(reach, tls))
            })
            .map_err(Clone::clone);
        Self { client, reach }
    }

    fn base(&self, address: &str) -> String {
        if address.starts_with("http://") || address.starts_with("https://") {
            return address.trim_end_matches('/').to_owned();
        }
        let bare = address.rsplit_once(':').map_or(address, |(h, _)| h);
        if self.reach == Reach::Any && matches!(bare, "localhost" | "127.0.0.1") {
            format!("http://{address}")
        } else {
            format!("https://{address}")
        }
    }

    async fn call(
        &self,
        base: &str,
        token: &SecretString,
        call: Call<'_>,
    ) -> Result<Answer, PushError> {
        let Call {
            method,
            url,
            content_type,
            body,
            length,
            deadline,
        } = call;
        let client = self
            .client
            .as_ref()
            .map_err(|e| PushError::Unreachable(e.clone()))?;
        let url = within(base, url)?;
        let uri: http::Uri = url
            .parse()
            .map_err(|e: http::uri::InvalidUri| PushError::Unexpected(e.to_string()))?;
        refuse_named(self.reach, &uri).map_err(PushError::Unreachable)?;
        let mut authorization = HeaderValue::from_str(&format!("Bearer {}", token.expose_secret()))
            .map_err(|e| PushError::Unexpected(e.to_string()))?;
        authorization.set_sensitive(true);
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::USER_AGENT, "zeabur")
            .header(header::AUTHORIZATION, authorization)
            .header(header::CONTENT_LENGTH, length);
        if let Some(content_type) = content_type {
            request = request.header(header::CONTENT_TYPE, content_type);
        }
        let request = request
            .body(body)
            .map_err(|e| PushError::Unexpected(e.to_string()))?;
        let exchange = async {
            let response = client
                .request(request)
                .await
                .map_err(|e| PushError::Unreachable(describe(&e)))?;
            let (parts, body) = response.into_parts();
            Limited::new(body, ANSWER_LIMIT)
                .collect()
                .await
                .map_err(|e| PushError::Unexpected(e.to_string()))?;
            Ok(Answer {
                status: parts.status,
                headers: parts.headers,
            })
        };
        tokio::time::timeout(deadline, exchange)
            .await
            .map_err(|_elapsed| {
                PushError::Unreachable("the registry did not answer in time".to_owned())
            })?
    }

    /// Opens an upload session and returns where to continue it.
    async fn start(&self, base: &str, target: &Target) -> Result<String, PushError> {
        let url = format!("{base}/v2/{}/blobs/uploads/", target.repository);
        let answer = self
            .call(base, &target.token, Call::new(Method::POST, &url))
            .await?;
        expect(&answer, &[StatusCode::ACCEPTED])?;
        location(&answer, base)
    }

    async fn finish(
        &self,
        base: &str,
        target: &Target,
        location: &str,
        digest: &str,
    ) -> Result<(), PushError> {
        let sep = if location.contains('?') { '&' } else { '?' };
        let url = format!("{location}{sep}digest={digest}");
        let answer = self
            .call(
                base,
                &target.token,
                Call::new(Method::PUT, &url).blob(Body::empty(), 0),
            )
            .await?;
        expect(&answer, &[StatusCode::CREATED])
    }

    /// Pushes `body`, `length` bytes of gzip tar, under `tag` as a Zeabur source artifact.
    pub(crate) async fn push(
        &self,
        target: &Target,
        tag: &str,
        body: Body,
        length: u64,
    ) -> Result<Pushed, PushError> {
        let base = self.base(&target.address);
        let empty = self.start(&base, target).await?;
        let sep = if empty.contains('?') { '&' } else { '?' };
        let url = format!("{empty}{sep}digest={EMPTY_DIGEST}");
        let answer = self
            .call(
                &base,
                &target.token,
                Call::new(Method::PUT, &url).blob(Body::from("{}"), 2),
            )
            .await?;
        expect(&answer, &[StatusCode::CREATED])?;

        let upload = self.start(&base, target).await?;
        let hash = Arc::new(Mutex::new(Sha256::new()));
        let seen = Arc::new(AtomicU64::new(0));
        let counted = {
            let (hash, seen) = (Arc::clone(&hash), Arc::clone(&seen));
            body.into_data_stream().map(move |chunk| {
                let chunk = chunk?;
                let total =
                    seen.fetch_add(chunk.len() as u64, Ordering::Relaxed) + chunk.len() as u64;
                if total > length {
                    return Err(axum::Error::new(std::io::Error::other(
                        "longer than declared",
                    )));
                }
                if let Ok(mut hash) = hash.lock() {
                    hash.update(&chunk);
                }
                Ok(chunk)
            })
        };
        let answer = self
            .call(
                &base,
                &target.token,
                Call {
                    deadline: TRANSFER,
                    ..Call::new(Method::PATCH, &upload).blob(Body::from_stream(counted), length)
                },
            )
            .await
            .map_err(|error| {
                if seen.load(Ordering::Relaxed) == length {
                    error
                } else {
                    PushError::Length
                }
            })?;
        expect(&answer, &[StatusCode::ACCEPTED, StatusCode::CREATED])?;
        if seen.load(Ordering::Relaxed) != length {
            return Err(PushError::Length);
        }
        let layer = hash
            .lock()
            .map(|hash| format!("sha256:{}", hex::encode(hash.clone().finalize())))
            .map_err(|_poisoned| PushError::Unexpected("the digest was lost".to_owned()))?;
        let next = location(&answer, &base)?;
        self.finish(&base, target, &next, &layer).await?;

        let manifest = serde_json::to_vec(&json!({
            "schemaVersion": 2,
            "mediaType": MANIFEST_TYPE,
            "artifactType": ARTIFACT_TYPE,
            "config": {"mediaType": EMPTY_TYPE, "digest": EMPTY_DIGEST, "size": 2},
            "layers": [{"mediaType": LAYER_TYPE, "digest": layer, "size": length}],
        }))
        .map_err(|e| PushError::Unexpected(e.to_string()))?;
        let digest = format!("sha256:{}", hex::encode(Sha256::digest(&manifest)));
        let url = format!("{base}/v2/{}/manifests/{tag}", target.repository);
        let size = manifest.len() as u64;
        let answer = self
            .call(
                &base,
                &target.token,
                Call {
                    content_type: Some(MANIFEST_TYPE),
                    ..Call::new(Method::PUT, &url).blob(Body::from(manifest), size)
                },
            )
            .await?;
        expect(&answer, &[StatusCode::CREATED])?;
        if let Some(said) = answer
            .headers
            .get("docker-content-digest")
            .and_then(|v| v.to_str().ok())
            && said != digest
        {
            return Err(PushError::Unexpected(format!(
                "the registry stored the manifest as {said}, not {digest}"
            )));
        }
        Ok(Pushed {
            digest,
            tag: tag.to_owned(),
        })
    }
}

fn expect(answer: &Answer, accepted: &[StatusCode]) -> Result<(), PushError> {
    if accepted.contains(&answer.status) {
        Ok(())
    } else if matches!(
        answer.status,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
    ) {
        Err(PushError::Refused)
    } else {
        Err(PushError::Unexpected(format!(
            "the registry answered {}",
            answer.status
        )))
    }
}

/// The `Location` of the answer, as an absolute URL on `base`'s origin.
fn location(answer: &Answer, base: &str) -> Result<String, PushError> {
    let value = answer
        .headers
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| PushError::Unexpected("the registry gave no upload location".to_owned()))?;
    within(base, value)
}

/// `url` as an absolute URL, which must be on `base`'s origin: the token goes nowhere else.
fn within(base: &str, url: &str) -> Result<String, PushError> {
    if url.starts_with('/') && !url.starts_with("//") {
        return Ok(format!("{base}{url}"));
    }
    match url.strip_prefix(base) {
        Some(rest) if rest.is_empty() || rest.starts_with('/') || rest.starts_with('?') => {
            Ok(url.to_owned())
        }
        _ => Err(PushError::Unexpected(
            "the registry pointed the upload to another address".to_owned(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_location_on_another_origin_never_gets_the_token() {
        let base = "https://zot.zot.svc:5000";
        assert_eq!(
            within(base, "/v2/a/b/blobs/uploads/1?_state=x").as_deref(),
            Ok("https://zot.zot.svc:5000/v2/a/b/blobs/uploads/1?_state=x")
        );
        assert!(within(base, "https://zot.zot.svc:5000/v2/a").is_ok());
        for other in [
            "https://evil.example/v2/a",
            "https://zot.zot.svc:50001/v2/a",
            "https://zot.zot.svc:5000.evil.example/v2/a",
            "http://zot.zot.svc:5000/v2/a",
            "//evil.example/v2/a",
        ] {
            assert!(within(base, other).is_err(), "{other}");
        }
    }
}
