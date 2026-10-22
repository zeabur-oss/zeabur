//! Where the issuer lets people manage how they sign in: the `manage` URL of its
//! `/.well-known/passkey-endpoints` [passkey-endpoints] (§3), at the issuer's origin.
//!
//! Zeabur holds no credentials, so its account page only links there. The document is read with
//! a short deadline, so an issuer that hangs does not hold the page, and kept for a while: an
//! answer (a URL, or none) for [`KEEP`], a failure for [`RETRY`].
//!
//! [passkey-endpoints]: https://www.w3.org/TR/passkey-endpoints-1/

use std::time::{Duration, Instant};

use bytes::Bytes;
use http::{Request, StatusCode, header};
use http_body_util::Full;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::client::{AuthError, ExtraRoots};
use crate::http::Http;
use crate::url::{BaseUrl, page_url};

/// How long one read may take.
const DEADLINE: Duration = Duration::from_secs(3);
/// How long an answer is kept.
pub const KEEP: Duration = Duration::from_secs(600);
/// How long a failed read is kept before the issuer is asked again.
pub const RETRY: Duration = Duration::from_secs(60);

#[derive(Deserialize)]
struct Endpoints {
    #[serde(default)]
    manage: Option<String>,
}

/// The issuer's passkey endpoints, read on demand.
pub struct PasskeyEndpoints {
    url: String,
    http: Http,
    deadline: Duration,
    kept: Mutex<Option<(Instant, Duration, Option<String>)>>,
}

impl std::fmt::Debug for PasskeyEndpoints {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasskeyEndpoints")
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

impl PasskeyEndpoints {
    /// The endpoints of `issuer`, trusting `extra` besides the platform's roots.
    ///
    /// # Errors
    ///
    /// When TLS cannot be set up.
    pub fn new(issuer: &BaseUrl, extra: ExtraRoots) -> Result<Self, AuthError> {
        Self::with_deadline(issuer, extra, DEADLINE)
    }

    fn with_deadline(
        issuer: &BaseUrl,
        extra: ExtraRoots,
        deadline: Duration,
    ) -> Result<Self, AuthError> {
        Ok(Self {
            url: format!("{}/.well-known/passkey-endpoints", issuer.origin()),
            http: Http::new(extra).map_err(|e| AuthError::Invalid(e.to_string()))?,
            deadline,
            kept: Mutex::new(None),
        })
    }

    /// The `manage` URL, when the issuer names a secure one.
    pub async fn manage(&self) -> Option<String> {
        let mut kept = self.kept.lock().await;
        if let Some((at, keep, answer)) = kept.as_ref()
            && at.elapsed() < *keep
        {
            return answer.clone();
        }
        let (keep, answer) = match tokio::time::timeout(self.deadline, self.read()).await {
            Ok(Ok(answer)) => (KEEP, answer),
            Ok(Err(error)) => {
                tracing::debug!(url = %self.url, %error, "cannot read the passkey endpoints");
                (RETRY, None)
            }
            Err(_elapsed) => (RETRY, None),
        };
        *kept = Some((Instant::now(), keep, answer.clone()));
        answer
    }

    async fn read(&self) -> Result<Option<String>, AuthError> {
        let request = Request::get(&self.url)
            .header(header::ACCEPT, "application/json")
            .body(Full::new(Bytes::new()))
            .map_err(|e| AuthError::Invalid(e.to_string()))?;
        let (status, body) = self
            .http
            .send(request)
            .await
            .map_err(|e| AuthError::Unreachable(e.to_string()))?;
        if status == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if status != StatusCode::OK {
            return Err(AuthError::Unreachable(format!(
                "{} answered {status}",
                self.url
            )));
        }
        let endpoints: Endpoints = serde_json::from_slice(&body)
            .map_err(|e| AuthError::Invalid(format!("{}: {e}", self.url)))?;
        Ok(endpoints.manage.and_then(|m| page_url(&m).ok()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::Router;
    use axum::routing::get;

    use super::*;

    async fn issuer(
        answer: &'static str,
        status: StatusCode,
        delay: Duration,
    ) -> (BaseUrl, Arc<AtomicUsize>) {
        let reads = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&reads);
        let app = Router::new().route(
            "/.well-known/passkey-endpoints",
            get(move || {
                counted.fetch_add(1, Ordering::SeqCst);
                async move {
                    tokio::time::sleep(delay).await;
                    (status, answer)
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/oidc", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        (BaseUrl::parse(&url).unwrap(), reads)
    }

    fn endpoints(issuer: &BaseUrl) -> PasskeyEndpoints {
        PasskeyEndpoints::with_deadline(issuer, ExtraRoots::default(), Duration::from_millis(500))
            .unwrap()
    }

    #[tokio::test]
    async fn the_manage_url_is_read_at_the_issuers_origin_once() {
        let (url, reads) = issuer(
            r#"{"enroll": "https://id.example.com/enroll", "manage": "https://id.example.com/account?tab=passkeys"}"#,
            StatusCode::OK,
            Duration::ZERO,
        )
        .await;
        let endpoints = endpoints(&url);
        for _ in 0..2 {
            assert_eq!(
                endpoints.manage().await.as_deref(),
                Some("https://id.example.com/account?tab=passkeys")
            );
        }
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn no_document_an_insecure_url_or_a_slow_issuer_name_no_page() {
        let (url, _) = issuer("", StatusCode::NOT_FOUND, Duration::ZERO).await;
        assert_eq!(endpoints(&url).manage().await, None);
        let (url, _) = issuer(
            r#"{"manage": "http://id.example.com/account"}"#,
            StatusCode::OK,
            Duration::ZERO,
        )
        .await;
        assert_eq!(endpoints(&url).manage().await, None);
        let (url, _) = issuer(
            r#"{"enroll": "https://id.example.com/enroll"}"#,
            StatusCode::OK,
            Duration::ZERO,
        )
        .await;
        assert_eq!(endpoints(&url).manage().await, None);
        let (url, _) = issuer(
            r#"{"manage": "https://id.example.com/account"}"#,
            StatusCode::OK,
            Duration::from_secs(5),
        )
        .await;
        assert_eq!(endpoints(&url).manage().await, None);
    }
}
