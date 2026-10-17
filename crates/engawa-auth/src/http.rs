//! A small HTTPS client for the issuer: hyper's client over rustls with aws-lc-rs, the stack
//! kube-rs already ships, and the platform's trust roots (the image's CA bundle), plus any
//! [`ExtraRoots`] for an issuer whose certificate a private CA signed.
//!
//! Every exchange has a deadline and a size limit, so an issuer that hangs or answers without
//! end cannot hold a worker or fill memory.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::{Request, StatusCode};
use http_body_util::{BodyExt as _, Full, Limited};
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject as _;

/// How long one exchange with the issuer may take, connection included.
const DEADLINE: Duration = Duration::from_secs(10);
/// The largest answer read from the issuer; its metadata and token answers are a few KiB.
const MAX_BODY: usize = 64 * 1024;

/// Certificate authorities trusted for the issuer on top of the platform's roots.
#[derive(Clone, Default)]
pub struct ExtraRoots(Vec<CertificateDer<'static>>);

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
        Ok(Self(roots))
    }

    /// How many certificates there are.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are none.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Debug for ExtraRoots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ExtraRoots").field(&self.0.len()).finish()
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
    pub(crate) fn new(extra: ExtraRoots) -> Result<Self, HttpError> {
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let verifier = rustls_platform_verifier::Verifier::new_with_extra_roots(
            extra.0,
            Arc::clone(&provider),
        )
        .map_err(|e| HttpError::Tls(e.to_string()))?;
        let tls = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| HttpError::Tls(e.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_no_client_auth();
        // `https_or_http` lets tests use a loopback issuer over plain HTTP; `crate::url::BaseUrl`
        // refuses plain HTTP to any other host before a request is made.
        let connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(tls)
            .https_or_http()
            .enable_http1()
            .build();
        let client = Client::builder(TokioExecutor::new())
            .pool_idle_timeout(Duration::from_secs(30))
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
