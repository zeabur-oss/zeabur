//! Zeabur's client for Zeabur Proxy (`crates/zeabur-proxy`): HTTPS that trusts only the proxy's
//! CA (the chart's, or the one `--proxy-ca-file` names), with the person's token as the bearer
//! token, a deadline and a size limit.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::http::{Request, Uri, header};
use bytes::Bytes;
use http_body_util::{BodyExt as _, Empty, Limited};
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject as _;
use zeabur_api::services::proxy::{ProxyAnswer, ZeaburProxy};
use zeabur_kube::UserToken;

/// Longer than the proxy's own 30 s for its backend, so its answer arrives first.
const DEADLINE: Duration = Duration::from_secs(35);
/// The proxy's own limit (16 MiB) and room for its problem.
const MAX_BODY: usize = (16 << 20) + 4096;

/// The client.
#[derive(Clone)]
pub(crate) struct Proxy {
    base: String,
    client: Client<HttpsConnector<HttpConnector>, Empty<Bytes>>,
}

impl std::fmt::Debug for Proxy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Proxy")
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

impl Proxy {
    /// A client for the proxy at `url` (`https://host[:port]`), trusting the CAs in `ca_pem`.
    pub(crate) fn new(url: &str, ca_pem: &[u8]) -> Result<Self, String> {
        let uri: Uri = url.parse().map_err(|e| format!("{url:?}: {e}"))?;
        let plain = uri.scheme_str() == Some("https")
            && uri.host().is_some()
            && uri.path_and_query().is_none_or(|p| p.as_str() == "/");
        if !plain {
            return Err(format!("{url:?} is not https://host[:port]"));
        }
        let mut roots = rustls::RootCertStore::empty();
        for cert in CertificateDer::pem_slice_iter(ca_pem) {
            let cert = cert.map_err(|e| format!("not a PEM certificate bundle: {e}"))?;
            roots.add(cert).map_err(|e| e.to_string())?;
        }
        if roots.is_empty() {
            return Err("no certificate in the CA file".to_owned());
        }
        let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_root_certificates(roots)
        .with_no_client_auth();
        let mut http = HttpConnector::new();
        http.enforce_http(false);
        http.set_connect_timeout(Some(Duration::from_secs(5)));
        let connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(tls)
            .https_only()
            .enable_http1()
            .wrap_connector(http);
        let client = Client::builder(TokioExecutor::new())
            .pool_timer(TokioTimer::new())
            .pool_idle_timeout(Duration::from_secs(30))
            .build(connector);
        Ok(Self {
            base: url.trim_end_matches('/').to_owned(),
            client,
        })
    }

    async fn send(&self, token: &UserToken, path: &str) -> Result<ProxyAnswer, String> {
        let request = Request::get(format!("{}{path}", self.base))
            .header(header::AUTHORIZATION, token.authorization().clone())
            .body(Empty::new())
            .map_err(|e| e.to_string())?;
        let exchange = async {
            let response = self
                .client
                .request(request)
                .await
                .map_err(|e| e.to_string())?;
            let status = response.status().as_u16();
            let body = Limited::new(response.into_body(), MAX_BODY)
                .collect()
                .await
                .map_err(|e| e.to_string())?
                .to_bytes();
            Ok(ProxyAnswer {
                status,
                body: body.to_vec(),
            })
        };
        tokio::time::timeout(DEADLINE, exchange)
            .await
            .map_err(|_elapsed| "no answer in time".to_owned())?
    }
}

impl ZeaburProxy for Proxy {
    fn get<'a>(
        &'a self,
        token: &'a UserToken,
        path_and_query: String,
    ) -> Pin<Box<dyn Future<Output = Result<ProxyAnswer, String>> + Send + 'a>> {
        Box::pin(async move { self.send(token, &path_and_query).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_https_origin_and_a_ca_make_a_client() {
        assert!(
            Proxy::new("http://proxy.zeabur.svc", b"")
                .unwrap_err()
                .contains("https://host")
        );
        assert!(
            Proxy::new("https://proxy.zeabur.svc/logs", b"")
                .unwrap_err()
                .contains("https://host")
        );
        assert!(Proxy::new("https://proxy.zeabur.svc", b"").is_err());
        assert!(Proxy::new("https://proxy.zeabur.svc", b"not pem").is_err());
    }
}
