//! Fixed certificates and a loopback HTTPS server, to test which certificates a client trusts.
//!
//! `testdata/tls/generate.sh` made them: two unrelated CAs, [`CA_A`] and [`CA_B`], and a leaf for
//! `localhost` and `127.0.0.1` that CA A signed. The server presents that leaf.

use std::net::SocketAddr;
use std::sync::Arc;

use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

/// The PEM certificate of CA A, which signed the leaf.
pub const CA_A: &str = include_str!("../testdata/tls/ca-a.pem");
/// The PEM certificate of CA B, which signed nothing.
pub const CA_B: &str = include_str!("../testdata/tls/ca-b.pem");
const LEAF: &str = include_str!("../testdata/tls/leaf.pem");
const LEAF_KEY: &str = include_str!("../testdata/tls/leaf.key");

/// Serves `200 OK` with the body `ok` over TLS on a loopback port, to every client that finishes
/// the handshake; a client that refuses the certificate is dropped. The address is the leaf's
/// `127.0.0.1`.
///
/// # Panics
///
/// When the port cannot be bound.
#[expect(clippy::expect_used, reason = "a test cannot go on without its server")]
pub async fn serve() -> SocketAddr {
    let chain = vec![CertificateDer::from_pem_slice(LEAF.as_bytes()).expect("leaf")];
    let key = PrivateKeyDer::from_pem_slice(LEAF_KEY.as_bytes()).expect("leaf key");
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("protocol versions")
    .with_no_client_auth()
    .with_single_cert(chain, key)
    .expect("server config");
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                return;
            };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut stream) = acceptor.accept(socket).await else {
                    return;
                };
                let mut buffer = [0; 1024];
                let _request: std::io::Result<usize> = stream.read(&mut buffer).await;
                let _written: std::io::Result<()> = stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok",
                    )
                    .await;
                let _closed: std::io::Result<()> = stream.shutdown().await;
            });
        }
    });
    address
}
