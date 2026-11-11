//! Opening a connection: directly or through a `CONNECT` tunnel, with the address guard applied
//! to the target in both cases, then TLS for `https`.

use std::future::Future;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use http::Uri;
use hyper::rt::{Read, ReadBufCursor, Write};
use hyper_util::client::legacy::connect::{Connected, Connection};
use hyper_util::client::proxy::matcher::{Intercept, Matcher};
use hyper_util::rt::TokioIo;
use rustls::pki_types::ServerName;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio::task::JoinSet;

use crate::{DEADLINE, Reach, allowed};

/// How long an address has to connect before the next one is tried alongside it.
const STAGGER: std::time::Duration = std::time::Duration::from_millis(250);

/// How much of a proxy's answer to `CONNECT` is read before giving up.
const PROXY_HEAD_LIMIT: usize = 16 * 1024;

/// Which destinations go through which proxy, from the proxy variables.
#[derive(Clone)]
pub struct Proxies(Arc<Matcher>);

impl std::fmt::Debug for Proxies {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Proxies").finish_non_exhaustive()
    }
}

impl Proxies {
    /// `ALL_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY` and `NO_PROXY`, in either case, with curl's rules
    /// for `NO_PROXY` (domain suffixes, addresses and CIDR ranges). The system's own proxy
    /// settings are not read, so the variables alone decide, as for curl and kubectl.
    #[must_use]
    pub fn from_env() -> Self {
        Self(Arc::new(Matcher::from_env()))
    }

    /// No proxy: every destination is reached directly.
    #[must_use]
    pub fn none() -> Self {
        Self(Arc::new(Matcher::builder().build()))
    }

    /// `proxy` for every destination except those `no_proxy` lists, in the format of `NO_PROXY`.
    #[must_use]
    pub fn all(proxy: &str, no_proxy: &str) -> Self {
        Self(Arc::new(Matcher::builder().all(proxy).no(no_proxy).build()))
    }
}

/// How a name is turned into addresses.
pub(crate) type Resolver = Arc<
    dyn Fn(String, u16) -> Pin<Box<dyn Future<Output = io::Result<Vec<SocketAddr>>> + Send>>
        + Send
        + Sync,
>;

fn system_resolver() -> Resolver {
    Arc::new(|host, port| {
        Box::pin(async move { Ok(tokio::net::lookup_host((host, port)).await?.collect()) })
    })
}

/// The connector: where `reach` allows, through the proxy the destination calls for, with TLS
/// for `https`.
#[derive(Clone)]
pub struct Connector {
    reach: Reach,
    tls: tokio_rustls::TlsConnector,
    proxies: Proxies,
    resolve: Resolver,
}

/// A connector that connects where `reach` allows, trusts what `tls` trusts, and uses the proxy
/// the environment names.
#[must_use]
pub fn connector(reach: Reach, tls: &Arc<rustls::ClientConfig>) -> Connector {
    connector_with(reach, tls, Proxies::from_env())
}

/// [`connector`] with the proxies given instead of the environment's.
#[must_use]
pub fn connector_with(
    reach: Reach,
    tls: &Arc<rustls::ClientConfig>,
    proxies: Proxies,
) -> Connector {
    Connector {
        reach,
        tls: tokio_rustls::TlsConnector::from(Arc::clone(tls)),
        proxies,
        resolve: system_resolver(),
    }
}

impl std::fmt::Debug for Connector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connector")
            .field("reach", &self.reach)
            .finish_non_exhaustive()
    }
}

impl Connector {
    #[cfg(test)]
    pub(crate) fn resolving(mut self, resolve: Resolver) -> Self {
        self.resolve = resolve;
        self
    }

    /// Where the connection to `host` goes: no proxy for a local address or a host `NO_PROXY`
    /// covers.
    pub(crate) fn route(&self, host: &str, uri: &Uri) -> Option<Intercept> {
        if is_local(host) {
            return None;
        }
        self.proxies.0.intercept(uri)
    }

    /// The addresses of `host` that `reach` allows, in the order the resolver gave them.
    pub(crate) async fn targets(&self, host: &str, port: u16) -> io::Result<Vec<SocketAddr>> {
        if let Ok(ip) = host.parse::<IpAddr>() {
            if !allowed(self.reach, ip) {
                return Err(refused(format!("Zeabur does not connect to {ip}")));
            }
            return Ok(vec![SocketAddr::new(ip, port)]);
        }
        let addresses: Vec<SocketAddr> = (self.resolve)(host.to_owned(), port)
            .await?
            .into_iter()
            .filter(|a| allowed(self.reach, a.ip()))
            .collect();
        if addresses.is_empty() {
            return Err(refused(format!(
                "Zeabur does not connect to the addresses of {host}"
            )));
        }
        Ok(addresses)
    }

    async fn connect(&self, uri: Uri) -> io::Result<Conn> {
        let https = match uri.scheme_str() {
            Some("https") => true,
            Some("http") => false,
            _ => return Err(io::Error::other("only http and https URLs are supported")),
        };
        let host = uri
            .host()
            .map(|h| h.trim_start_matches('[').trim_end_matches(']').to_owned())
            .ok_or_else(|| io::Error::other("the URL has no host"))?;
        let port = uri.port_u16().unwrap_or(if https { 443 } else { 80 });
        let tcp = match self.route(&host, &uri) {
            None => self.dial(&host, port).await?,
            Some(proxy) => self.tunnel(&proxy, &host, port).await?,
        };
        if !https {
            return Ok(Conn::new(tcp));
        }
        let name = ServerName::try_from(host).map_err(io::Error::other)?;
        Ok(Conn::new(self.tls.connect(name, tcp).await?))
    }

    /// A direct connection to the first checked address that answers.
    async fn dial(&self, host: &str, port: u16) -> io::Result<TcpStream> {
        first(self.targets(host, port).await?).await
    }

    /// A `CONNECT` tunnel through `proxy`. The proxy hop is not checked, since an administrator
    /// chose it. For [`Reach::Public`] the tunnel is asked for the checked address of the target
    /// and not for its name, so the proxy cannot resolve the name to somewhere else.
    async fn tunnel(&self, proxy: &Intercept, host: &str, port: u16) -> io::Result<TcpStream> {
        let uri = proxy.uri();
        if uri.scheme_str() != Some("http") {
            return Err(io::Error::other(
                "only an http:// proxy is supported, which tunnels with CONNECT",
            ));
        }
        let proxy_host = uri
            .host()
            .map(|h| h.trim_start_matches('[').trim_end_matches(']'))
            .ok_or_else(|| io::Error::other("the proxy URL has no host"))?;
        let proxy_port = uri.port_u16().unwrap_or(80);
        let hop = match proxy_host.parse::<IpAddr>() {
            Ok(ip) => vec![SocketAddr::new(ip, proxy_port)],
            Err(_) => (self.resolve)(proxy_host.to_owned(), proxy_port).await?,
        };
        let authority = match self.reach {
            Reach::Public => self
                .targets(host, port)
                .await?
                .first()
                .map(ToString::to_string)
                .ok_or_else(|| io::Error::other("no address to tunnel to"))?,
            Reach::Any => host.parse::<IpAddr>().map_or_else(
                |_| format!("{host}:{port}"),
                |ip| SocketAddr::new(ip, port).to_string(),
            ),
        };
        let mut stream = first(hop).await?;
        let mut head = format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n");
        if let Some(authorization) = proxy.basic_auth().and_then(|v| v.to_str().ok()) {
            head.push_str("Proxy-Authorization: ");
            head.push_str(authorization);
            head.push_str("\r\n");
        }
        head.push_str("\r\n");
        stream.write_all(head.as_bytes()).await?;
        let status = read_status(&mut stream).await?;
        if !(200..300).contains(&status) {
            return Err(io::Error::other(format!(
                "the proxy refused the tunnel to {authority} with status {status}"
            )));
        }
        Ok(stream)
    }
}

/// The status of a proxy's answer to `CONNECT`. The head is read byte by byte, so nothing the
/// target sends after it is taken.
async fn read_status(stream: &mut TcpStream) -> io::Result<u16> {
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() >= PROXY_HEAD_LIMIT {
            return Err(io::Error::other("the proxy's answer is too long"));
        }
        head.push(stream.read_u8().await?);
    }
    String::from_utf8_lossy(&head)
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| io::Error::other("the proxy did not answer with an HTTP status"))
}

/// The first connection to succeed. The next address is tried when the one before it fails or
/// has not answered within [`STAGGER`], and the earlier attempts keep going, so an address that
/// silently drops packets does not hold up the others.
async fn first(addresses: Vec<SocketAddr>) -> io::Result<TcpStream> {
    let mut remaining = addresses.into_iter();
    let mut attempts = JoinSet::new();
    let mut last = None;
    loop {
        if let Some(address) = remaining.next() {
            attempts.spawn(TcpStream::connect(address));
        } else if attempts.is_empty() {
            break;
        }
        let Ok(finished) = tokio::time::timeout(STAGGER, attempts.join_next()).await else {
            continue;
        };
        match finished {
            Some(Ok(Ok(stream))) => return Ok(stream),
            Some(Ok(Err(error))) => last = Some(error),
            Some(Err(error)) => last = Some(io::Error::other(error)),
            None => {}
        }
    }
    Err(last.unwrap_or_else(|| io::Error::other("no address to connect to")))
}

fn refused(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}

/// Whether `host` names this machine or its link: a name under `localhost`, or a loopback,
/// unspecified or link-local address. Such a destination is never sent to a proxy.
pub(crate) fn is_local(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") {
        return true;
    }
    let Ok(ip) = host.parse::<IpAddr>() else {
        return false;
    };
    let ip = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
        IpAddr::V4(_) => ip,
    };
    match ip {
        IpAddr::V4(ip) => ip.is_loopback() || ip.is_unspecified() || ip.is_link_local(),
        IpAddr::V6(ip) => ip.is_loopback() || ip.is_unspecified() || ip.is_unicast_link_local(),
    }
}

impl tower::Service<Uri> for Connector {
    type Response = Conn;
    type Error = io::Error;
    type Future = Pin<Box<dyn Future<Output = io::Result<Conn>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        let this = self.clone();
        Box::pin(async move {
            tokio::time::timeout(DEADLINE, this.connect(uri))
                .await
                .map_err(|_elapsed| {
                    io::Error::new(io::ErrorKind::TimedOut, "connecting timed out")
                })?
        })
    }
}

trait Io: AsyncRead + AsyncWrite + Send + Unpin {}

impl<T: AsyncRead + AsyncWrite + Send + Unpin> Io for T {}

/// An open connection, in TLS or not.
pub struct Conn(TokioIo<Box<dyn Io>>);

impl Conn {
    fn new(io: impl AsyncRead + AsyncWrite + Send + Unpin + 'static) -> Self {
        Self(TokioIo::new(Box::new(io)))
    }
}

impl Read for Conn {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_read(cx, buf)
    }
}

impl Write for Conn {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.0).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }
}

impl Connection for Conn {
    fn connected(&self) -> Connected {
        Connected::new()
    }
}

impl std::fmt::Debug for Conn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Conn").finish_non_exhaustive()
    }
}
