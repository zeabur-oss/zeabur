//! The one outbound HTTP client: hyper over rustls with aws-lc-rs and the platform's trust
//! roots, the stack kube-rs already ships, with the proxy variables and the address guard.
//!
//! # Where it may connect
//!
//! In the cluster, Zeabur's pod reaches addresses a person may not, so the server's client
//! ([`Reach::Public`]) refuses loopback, link-local (the cloud metadata service), unspecified,
//! multicast and broadcast addresses, whether a name resolves to them or the URL names them.
//! Addresses on private networks stay reachable, since registries and stores live there.
//!
//! # Through a proxy
//!
//! The client reads `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY` and `NO_PROXY` (either case) and
//! tunnels with `CONNECT` ([`Proxies`]). Local addresses are never sent to a proxy. The proxy hop
//! is not guarded, since an administrator chose it, but the target is: it is checked before a
//! direct connection or a `CONNECT`, and the tunnel is opened to the address that was checked, so
//! a proxy that resolves the name differently cannot lead the connection elsewhere. TLS and the
//! HTTP `Host` still use the name.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::{self, Request, Response};
use http_body_util::{BodyExt as _, Full, Limited};
use hyper::body::Incoming;
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject as _;

mod connect;

pub use connect::{Connector, Proxies, connector, connector_with};

/// How long one exchange may take, connection included.
pub const DEADLINE: Duration = Duration::from_secs(5);
/// How long a connection may sit unused in the pool.
pub const IDLE: Duration = Duration::from_secs(30);
/// How many unused connections the pool keeps per host.
pub const IDLE_PER_HOST: usize = 2;

/// Which addresses the client may connect to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// Any: `zeaburctl ui`, on the person's own machine.
    Any,
    /// Not loopback, link-local, unspecified, multicast or broadcast: the in-cluster server.
    Public,
}

/// An HTTPS client.
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
    /// A client that connects where `reach` allows, trusting what `tls` trusts, through the
    /// proxy the environment names.
    #[must_use]
    pub fn new(reach: Reach, tls: &Arc<rustls::ClientConfig>) -> Self {
        Self::with_proxies(reach, tls, Proxies::from_env())
    }

    /// [`Self::new`] with the proxies given instead of the environment's.
    #[must_use]
    pub fn with_proxies(reach: Reach, tls: &Arc<rustls::ClientConfig>, proxies: Proxies) -> Self {
        let client = pooled(connector_with(reach, tls, proxies), IDLE);
        Self { client, reach }
    }

    /// Which addresses this client may connect to.
    #[must_use]
    pub const fn reach(&self) -> Reach {
        self.reach
    }

    /// Sends `request` and returns the answer once its head arrives. The caller reads the body
    /// and bounds the time.
    ///
    /// # Errors
    ///
    /// The URL names an address the client may not reach, or the exchange fails; the message
    /// names the innermost cause.
    pub async fn request(
        &self,
        request: Request<Full<Bytes>>,
    ) -> Result<Response<Incoming>, String> {
        refuse_named(self.reach, request.uri())?;
        self.client.request(request).await.map_err(|e| describe(&e))
    }

    /// Sends `request` and reads at most `limit` bytes of the answer, whatever its status, within
    /// [`DEADLINE`]; the status and the body.
    ///
    /// # Errors
    ///
    /// As [`Self::request`], and when the answer is too long or late.
    pub async fn exchange(
        &self,
        request: Request<Full<Bytes>>,
        limit: usize,
    ) -> Result<(u16, Vec<u8>), String> {
        let (status, _, body) = self.exchange_with_headers(request, limit).await?;
        Ok((status, body))
    }

    /// [`Self::exchange`], with the answer's headers.
    ///
    /// # Errors
    ///
    /// As [`Self::exchange`].
    pub async fn exchange_with_headers(
        &self,
        request: Request<Full<Bytes>>,
        limit: usize,
    ) -> Result<(u16, http::HeaderMap, Vec<u8>), String> {
        let exchange = async {
            let response = self.request(request).await?;
            let status = response.status().as_u16();
            let headers = response.headers().clone();
            let body = Limited::new(response.into_body(), limit)
                .collect()
                .await
                .map_err(|e| e.to_string())?
                .to_bytes()
                .to_vec();
            Ok((status, headers, body))
        };
        tokio::time::timeout(DEADLINE, exchange)
            .await
            .map_err(|_elapsed| "the server did not answer in time".to_owned())?
    }
}

/// What a trust policy's clients share: its TLS configuration, or why there is none.
pub type Tls = Result<Arc<rustls::ClientConfig>, String>;

/// The TLS configuration for the platform's trust roots and the certificates of the PEM bundle
/// `extra_ca`; one per trust policy, shared by its clients.
///
/// # Errors
///
/// When `extra_ca` is not a PEM certificate bundle, or TLS cannot be set up.
pub fn tls(extra_ca: Option<&[u8]>) -> Tls {
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
    Ok(Arc::new(
        rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_no_client_auth(),
    ))
}

/// A client over `connector` that closes a connection left unused for `idle`.
#[must_use]
pub fn pooled<B>(connector: Connector, idle: Duration) -> Client<Connector, B>
where
    B: http_body::Body + Send,
    B::Data: Send,
{
    Client::builder(TokioExecutor::new())
        .pool_timer(TokioTimer::new())
        .pool_idle_timeout(idle)
        .pool_max_idle_per_host(IDLE_PER_HOST)
        .build(connector)
}

/// Refuses a URL that names an address `reach` does not allow.
///
/// # Errors
///
/// The message names the address.
pub fn refuse_named(reach: Reach, uri: &http::Uri) -> Result<(), String> {
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
#[must_use]
pub fn describe(error: &(dyn std::error::Error + 'static)) -> String {
    let mut source = error;
    while let Some(next) = source.source() {
        source = next;
    }
    source.to_string()
}

/// Whether `reach` allows connecting to `ip`.
#[must_use]
pub const fn allowed(reach: Reach, ip: IpAddr) -> bool {
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

#[cfg(test)]
mod tests;
