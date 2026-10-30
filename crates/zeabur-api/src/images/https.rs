//! The registry client: hyper over rustls with aws-lc-rs and the platform's trust roots, the
//! stack kube-rs already ships.
//!
//! In the cluster, Zeabur's pod reaches addresses a person may not, so the server's client
//! refuses loopback, link-local (the cloud metadata service), unspecified, multicast and
//! broadcast addresses, whether a name resolves to them or the URL names them.

use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::http::{self, Request};
use bytes::Bytes;
use http_body_util::{BodyExt as _, Full, Limited};
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::connect::dns::{GaiResolver, Name};
use hyper_util::rt::{TokioExecutor, TokioTimer};
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject as _;
use secrecy::ExposeSecret as _;

use super::registry::{Answer, Get, Transport};

/// How long one exchange may take, connection included.
const DEADLINE: Duration = Duration::from_secs(5);
/// How long a connection may sit unused in the pool.
pub(crate) const IDLE: Duration = Duration::from_secs(30);
/// How many unused connections the pool keeps per host.
pub(crate) const IDLE_PER_HOST: usize = 2;

/// Which addresses the client may connect to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// Any: `zeaburctl ui`, on the person's own machine.
    Any,
    /// Not loopback, link-local, unspecified, multicast or broadcast: the in-cluster server.
    Public,
}

/// A registry client.
#[derive(Clone)]
pub struct Https {
    client: Client<Connector, Full<Bytes>>,
    reach: Reach,
}

impl std::fmt::Debug for Https {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Https")
            .field("reach", &self.reach)
            .finish_non_exhaustive()
    }
}

impl Https {
    /// A client that connects where `reach` allows, trusting the platform's roots and the
    /// certificates of the PEM bundle `extra_ca`.
    ///
    /// # Errors
    ///
    /// When `extra_ca` is not a PEM certificate bundle, or TLS cannot be set up.
    pub fn new(reach: Reach, extra_ca: Option<&[u8]>) -> Result<Self, String> {
        let client = pooled(connector(reach, extra_ca)?, IDLE);
        Ok(Self { client, reach })
    }

    async fn send(&self, get: Get) -> Result<Answer, String> {
        let uri: http::Uri = get.url.parse().map_err(|e| format!("{e}"))?;
        refuse_named(self.reach, &uri)?;
        let mut request = Request::get(uri).header(http::header::USER_AGENT, "zeabur");
        if let Some(accept) = &get.accept {
            request = request.header(http::header::ACCEPT, accept);
        }
        if let Some(authorization) = &get.authorization {
            let mut value = http::HeaderValue::from_str(authorization.expose_secret())
                .map_err(|e| e.to_string())?;
            value.set_sensitive(true);
            request = request.header(http::header::AUTHORIZATION, value);
        }
        let request = request.body(Full::default()).map_err(|e| e.to_string())?;
        let response = self
            .client
            .request(request)
            .await
            .map_err(|e| describe(&e))?;
        let header = |name: http::header::HeaderName| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        let www_authenticate = header(http::header::WWW_AUTHENTICATE);
        let location = header(http::header::LOCATION);
        let content_type = header(http::header::CONTENT_TYPE);
        let status = response.status().as_u16();
        let body = if status == 200 {
            Limited::new(response.into_body(), get.limit)
                .collect()
                .await
                .map_err(|e| e.to_string())?
                .to_bytes()
                .to_vec()
        } else {
            Vec::new()
        };
        Ok(Answer {
            status,
            www_authenticate,
            location,
            content_type,
            body,
        })
    }

    /// Sends `request` and reads at most `limit` bytes of the answer, whatever its status, within
    /// [`DEADLINE`]; the status and the body. For the GitHub client.
    pub(crate) async fn exchange(
        &self,
        request: Request<Full<Bytes>>,
        limit: usize,
    ) -> Result<(u16, Vec<u8>), String> {
        refuse_named(self.reach, request.uri())?;
        let exchange = async {
            let response = self
                .client
                .request(request)
                .await
                .map_err(|e| describe(&e))?;
            let status = response.status().as_u16();
            let body = Limited::new(response.into_body(), limit)
                .collect()
                .await
                .map_err(|e| e.to_string())?
                .to_bytes()
                .to_vec();
            Ok((status, body))
        };
        tokio::time::timeout(DEADLINE, exchange)
            .await
            .map_err(|_elapsed| "the server did not answer in time".to_owned())?
    }
}

/// The HTTPS connector, through the platform's trust roots, to the addresses `reach` allows.
pub(crate) type Connector = HttpsConnector<HttpConnector<Guarded>>;

/// A [`Connector`] that connects where `reach` allows and also trusts `extra_ca`, a PEM bundle.
///
/// # Errors
///
/// When `extra_ca` is not a PEM certificate bundle, or TLS cannot be set up.
pub(crate) fn connector(reach: Reach, extra_ca: Option<&[u8]>) -> Result<Connector, String> {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let extra = match extra_ca {
        Some(pem) => {
            let roots = CertificateDer::pem_slice_iter(pem)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("not a PEM certificate bundle: {e}"))?;
            if roots.is_empty() {
                return Err("not a PEM certificate bundle: no certificate in it".to_owned());
            }
            roots
        }
        None => Vec::new(),
    };
    let verifier =
        rustls_platform_verifier::Verifier::new_with_extra_roots(extra, Arc::clone(&provider))
            .map_err(|e| e.to_string())?;
    let tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    let mut http = HttpConnector::new_with_resolver(Guarded {
        inner: GaiResolver::new(),
        reach,
    });
    http.enforce_http(false);
    http.set_connect_timeout(Some(DEADLINE));
    Ok(hyper_rustls::HttpsConnectorBuilder::new()
        .with_tls_config(tls)
        .https_or_http()
        .enable_http1()
        .wrap_connector(http))
}

/// A client over `connector` that closes a connection left unused for `idle`.
pub(crate) fn pooled<B>(connector: Connector, idle: Duration) -> Client<Connector, B>
where
    B: axum::body::HttpBody + Send,
    B::Data: Send,
{
    Client::builder(TokioExecutor::new())
        .pool_timer(TokioTimer::new())
        .pool_idle_timeout(idle)
        .pool_max_idle_per_host(IDLE_PER_HOST)
        .build(connector)
}

/// Refuses a URL that names an address `reach` does not allow.
pub(crate) fn refuse_named(reach: Reach, uri: &http::Uri) -> Result<(), String> {
    match uri
        .host()
        .map(|h| h.trim_start_matches('[').trim_end_matches(']'))
        .and_then(|h| h.parse::<IpAddr>().ok())
    {
        Some(ip) if !allowed(reach, ip) => Err(format!("Zeabur does not connect to {ip}")),
        _ => Ok(()),
    }
}

/// The innermost cause, which names what failed (DNS, TLS, a refused address).
pub(crate) fn describe(error: &(dyn std::error::Error + 'static)) -> String {
    let mut source = error;
    while let Some(next) = source.source() {
        source = next;
    }
    source.to_string()
}

impl Transport for Https {
    fn get<'a>(
        &'a self,
        get: Get,
    ) -> Pin<Box<dyn Future<Output = Result<Answer, String>> + Send + 'a>> {
        Box::pin(async move {
            tokio::time::timeout(DEADLINE, self.send(get))
                .await
                .map_err(|_elapsed| "the registry did not answer in time".to_owned())?
        })
    }
}

pub(crate) const fn allowed(reach: Reach, ip: IpAddr) -> bool {
    match reach {
        Reach::Any => true,
        Reach::Public => {
            let ip = match ip {
                IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
                    Some(v4) => IpAddr::V4(v4),
                    None => IpAddr::V6(v6),
                },
                IpAddr::V4(_) => ip,
            };
            match ip {
                IpAddr::V4(v4) => {
                    !(v4.is_loopback()
                        || v4.is_link_local()
                        || v4.is_unspecified()
                        || v4.is_multicast()
                        || v4.is_broadcast())
                }
                IpAddr::V6(v6) => {
                    !(v6.is_loopback()
                        || v6.is_unicast_link_local()
                        || v6.is_unspecified()
                        || v6.is_multicast())
                }
            }
        }
    }
}

/// The system's resolver, keeping only the addresses `reach` allows.
#[derive(Clone)]
pub(crate) struct Guarded {
    inner: GaiResolver,
    reach: Reach,
}

impl tower::Service<Name> for Guarded {
    type Response = std::vec::IntoIter<SocketAddr>;
    type Error = std::io::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, name: Name) -> Self::Future {
        let reach = self.reach;
        let host = name.as_str().to_owned();
        let lookup = self.inner.call(name);
        Box::pin(async move {
            let addresses: Vec<SocketAddr> =
                lookup.await?.filter(|a| allowed(reach, a.ip())).collect();
            if addresses.is_empty() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("Zeabur does not connect to the addresses of {host}"),
                ));
            }
            Ok(addresses.into_iter())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_never_reaches_its_own_host_or_the_metadata_service() {
        for refused in [
            "127.0.0.1",
            "169.254.169.254",
            "0.0.0.0",
            "::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "224.0.0.1",
            "255.255.255.255",
        ] {
            let ip: IpAddr = refused.parse().unwrap();
            assert!(!allowed(Reach::Public, ip), "{refused}");
            assert!(allowed(Reach::Any, ip), "{refused}");
        }
        // Registries inside the cluster are on private addresses.
        for kept in ["10.43.0.10", "192.168.1.5", "104.16.0.1", "2606:4700::1"] {
            assert!(allowed(Reach::Public, kept.parse().unwrap()), "{kept}");
        }
    }

    #[test]
    fn an_extra_ca_must_be_a_pem_certificate_bundle() {
        let error = Https::new(Reach::Public, Some(b"not a certificate")).unwrap_err();
        assert!(error.contains("not a PEM certificate bundle"), "{error}");
        assert!(Https::new(Reach::Public, Some(b"")).is_err());
        assert!(Https::new(Reach::Public, None).is_ok());
    }

    #[tokio::test]
    async fn a_url_that_names_a_refused_address_is_not_sent() {
        let client = Https::new(Reach::Public, None).unwrap();
        let answer = client
            .get(Get {
                url: "http://169.254.169.254/latest/meta-data/".to_owned(),
                accept: None,
                authorization: None,
                limit: 1024,
            })
            .await;
        assert_eq!(
            answer.err().as_deref(),
            Some("Zeabur does not connect to 169.254.169.254")
        );
    }

    #[tokio::test]
    async fn a_pooled_connection_is_closed_once_the_idle_timeout_passes() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

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
        let client = pooled(
            connector(Reach::Any, None).unwrap(),
            Duration::from_millis(100),
        );
        let request = Request::get(url).body(Full::<Bytes>::default()).unwrap();
        let response = client.request(request).await.unwrap();
        response.into_body().collect().await.unwrap();
        assert_eq!(server.await.unwrap(), Ok(0), "the server saw the close");
    }
}
