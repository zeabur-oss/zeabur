//! Certificates made for this test run and a loopback HTTPS server, to test which certificates a
//! client trusts.
//!
//! Two unrelated CAs, [`ca_a`] and [`ca_b`], and a leaf for `localhost` and `127.0.0.1` that CA A
//! signed. Each is valid from an hour ago for a day, so none can expire under a checked-in test.
//! The server presents the leaf.

use std::net::SocketAddr;
use std::sync::{Arc, LazyLock};

use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use time::{Duration, OffsetDateTime};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

struct Certificates {
    ca_a: String,
    ca_b: String,
    leaf: CertificateDer<'static>,
    leaf_key: Vec<u8>,
}

#[expect(
    clippy::expect_used,
    reason = "a test cannot go on without its certificates"
)]
static CERTIFICATES: LazyLock<Certificates> = LazyLock::new(|| {
    let now = OffsetDateTime::now_utc();
    let valid = |params: &mut CertificateParams| {
        params.not_before = now - Duration::HOUR;
        params.not_after = now + Duration::DAY;
    };
    let ca = |name: &str| {
        let mut params = CertificateParams::default();
        valid(&mut params);
        params.distinguished_name.push(DnType::CommonName, name);
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let key = KeyPair::generate().expect("CA key");
        let certificate = params.self_signed(&key).expect("CA certificate");
        (certificate.pem(), Issuer::new(params, key))
    };
    let (ca_a, issuer) = ca("Zeabur test CA A");
    let (ca_b, _) = ca("Zeabur test CA B");

    let mut params = CertificateParams::new(["localhost".to_owned(), "127.0.0.1".to_owned()])
        .expect("leaf names");
    valid(&mut params);
    params
        .distinguished_name
        .push(DnType::CommonName, "localhost");
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let key = KeyPair::generate().expect("leaf key");
    let leaf = params.signed_by(&key, &issuer).expect("leaf certificate");
    Certificates {
        ca_a,
        ca_b,
        leaf: leaf.der().clone(),
        leaf_key: key.serialize_der(),
    }
});

/// The PEM certificate of CA A, which signed the leaf.
#[must_use]
pub fn ca_a() -> &'static str {
    &CERTIFICATES.ca_a
}

/// The PEM certificate of CA B, which signed nothing.
#[must_use]
pub fn ca_b() -> &'static str {
    &CERTIFICATES.ca_b
}

/// Serves `200 OK` with the body `ok` over TLS on a loopback port, to every client that finishes
/// the handshake; a client that refuses the certificate is dropped. The address is the leaf's
/// `127.0.0.1`.
///
/// # Panics
///
/// When the port cannot be bound.
#[expect(clippy::expect_used, reason = "a test cannot go on without its server")]
pub async fn serve() -> SocketAddr {
    let chain = vec![CERTIFICATES.leaf.clone()];
    let key = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(CERTIFICATES.leaf_key.clone()));
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
