//! A small HTTPS client for the issuer: hyper's client over rustls with aws-lc-rs, the stack
//! kube-rs already ships, and the platform's trust roots (the image's CA bundle), plus any
//! [`ExtraRoots`] for an issuer whose certificate a private CA signed.
//!
//! Every exchange has a deadline and a size limit, so an issuer that hangs or answers without
//! end cannot hold a worker or fill memory.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use bytes::Bytes;
use http::{Request, StatusCode};
use http_body_util::{BodyExt as _, Full, Limited};
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject as _;

/// How long one exchange with the issuer may take, connection included.
const DEADLINE: Duration = Duration::from_secs(10);
/// The largest answer read from the issuer; its metadata and token answers are a few KiB.
const MAX_BODY: usize = 64 * 1024;
/// How long a connection may sit unused in the pool.
const IDLE: Duration = Duration::from_secs(30);

/// Certificate authorities trusted for the issuer on top of the platform's roots.
#[derive(Clone, Default)]
pub struct ExtraRoots {
    certs: Vec<CertificateDer<'static>>,
    tls: Arc<OnceLock<Arc<rustls::ClientConfig>>>,
}

impl ExtraRoots {
    /// The certificates in a PEM bundle; anything between them is ignored, as in a CA bundle file.
    ///
    /// # Errors
    ///
    /// When the bundle is not PEM or holds no certificate.
    pub fn from_pem(pem: &[u8]) -> Result<Self, String> {
        let roots = CertificateDer::pem_slice_iter(pem)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("not a PEM certificate bundle: {e}"))?;
        if roots.is_empty() {
            return Err("no certificate in the bundle".to_owned());
        }
        Ok(Self {
            certs: roots,
            tls: Arc::default(),
        })
    }

    /// How many certificates there are.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.certs.len()
    }

    /// Whether there are none.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.certs.is_empty()
    }

    /// The TLS configuration for these roots, built on the first call and shared by every clone.
    fn tls(&self) -> Result<Arc<rustls::ClientConfig>, HttpError> {
        if let Some(tls) = self.tls.get() {
            return Ok(Arc::clone(tls));
        }
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let verifier = rustls_platform_verifier::Verifier::new_with_extra_roots(
            self.certs.clone(),
            Arc::clone(&provider),
        )
        .map_err(|e| HttpError::Tls(e.to_string()))?;
        let tls = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| HttpError::Tls(e.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_no_client_auth();
        Ok(Arc::clone(self.tls.get_or_init(|| Arc::new(tls))))
    }
}

impl std::fmt::Debug for ExtraRoots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ExtraRoots")
            .field(&self.certs.len())
            .finish()
    }
}

/// The client.
#[derive(Clone)]
pub(crate) struct Http {
    client: Client<HttpsConnector<HttpConnector>, Full<Bytes>>,
}

impl std::fmt::Debug for Http {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Http").finish_non_exhaustive()
    }
}

/// Why an exchange failed before an answer was read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum HttpError {
    /// TLS could not be set up.
    #[error("cannot set up TLS: {0}")]
    Tls(String),
    /// No answer in time, or the connection failed.
    #[error("{0}")]
    Transport(String),
}

impl Http {
    /// A client that trusts the platform's roots and `extra`.
    pub(crate) fn new(extra: &ExtraRoots) -> Result<Self, HttpError> {
        Self::with_idle(extra, IDLE)
    }

    fn with_idle(extra: &ExtraRoots, idle: Duration) -> Result<Self, HttpError> {
        // Plain HTTP lets tests use a loopback issuer; `crate::url::BaseUrl` refuses it to any
        // other host before a request is made.
        let mut http = HttpConnector::new();
        http.enforce_http(false);
        let connector = HttpsConnector::from((http, extra.tls()?));
        let client = Client::builder(TokioExecutor::new())
            .pool_timer(TokioTimer::new())
            .pool_idle_timeout(idle)
            .pool_max_idle_per_host(1)
            .build(connector);
        Ok(Self { client })
    }

    /// Sends `request` and reads the answer, within [`DEADLINE`] and [`MAX_BODY`].
    pub(crate) async fn send(
        &self,
        request: Request<Full<Bytes>>,
    ) -> Result<(StatusCode, Bytes), HttpError> {
        let exchange = async {
            let response = self
                .client
                .request(request)
                .await
                .map_err(|e| HttpError::Transport(e.to_string()))?;
            let status = response.status();
            let body = Limited::new(response.into_body(), MAX_BODY)
                .collect()
                .await
                .map_err(|e| HttpError::Transport(e.to_string()))?
                .to_bytes();
            Ok((status, body))
        };
        tokio::time::timeout(DEADLINE, exchange)
            .await
            .map_err(|_elapsed| {
                HttpError::Transport("the issuer did not answer in time".to_owned())
            })?
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    use super::*;

    #[tokio::test]
    async fn a_pooled_connection_is_closed_once_the_idle_timeout_passes() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 1024];
            let _request = socket.read(&mut buffer).await.unwrap();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok")
                .await
                .unwrap();
            tokio::time::timeout(Duration::from_secs(5), socket.read(&mut buffer))
                .await
                .map(|read| read.unwrap())
        });
        let http = Http::with_idle(&ExtraRoots::default(), Duration::from_millis(100)).unwrap();
        let request = Request::get(url).body(Full::new(Bytes::new())).unwrap();
        let (status, _) = http.send(request).await.unwrap();
        assert_eq!(status, StatusCode::OK);
        assert_eq!(server.await.unwrap(), Ok(0), "the server saw the close");
    }

    #[test]
    fn clones_of_the_roots_share_one_tls_configuration() {
        let roots = ExtraRoots::default();
        let clone = roots.clone();
        assert!(Arc::ptr_eq(&roots.tls().unwrap(), &clone.tls().unwrap()));
        assert!(!Arc::ptr_eq(
            &roots.tls().unwrap(),
            &ExtraRoots::default().tls().unwrap()
        ));
    }
}
