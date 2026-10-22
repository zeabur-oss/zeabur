//! The TLS listener's certificate, read again when its files change, so a renewed certificate
//! (cert-manager, or a Secret replaced by hand) is served without a restart.

use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::SystemTime;

use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;

/// Serves the certificate and key in two PEM files.
pub struct Reloading {
    cert: PathBuf,
    key: PathBuf,
    current: Mutex<(Option<SystemTime>, Arc<CertifiedKey>)>,
}

impl fmt::Debug for Reloading {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Reloading")
            .field("cert", &self.cert)
            .finish_non_exhaustive()
    }
}

impl Reloading {
    /// Reads the files once.
    ///
    /// # Errors
    ///
    /// When either cannot be read or parsed.
    pub fn new(cert: PathBuf, key: PathBuf) -> Result<Self, String> {
        let loaded = load(&cert, &key)?;
        Ok(Self {
            current: Mutex::new((modified(&cert), Arc::new(loaded))),
            cert,
            key,
        })
    }

    /// A server configuration with this certificate, HTTP/1.1 only.
    #[must_use]
    pub fn config(self) -> rustls::ServerConfig {
        let mut config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(self));
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        config
    }
}

fn modified(path: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn load(cert: &PathBuf, key: &PathBuf) -> Result<CertifiedKey, String> {
    let chain = CertificateDer::pem_file_iter(cert)
        .and_then(Iterator::collect::<Result<Vec<_>, _>>)
        .map_err(|e| format!("cannot read {}: {e}", cert.display()))?;
    let key = PrivateKeyDer::from_pem_file(key)
        .map_err(|e| format!("cannot read {}: {e}", key.display()))?;
    let provider = rustls::crypto::aws_lc_rs::default_provider();
    CertifiedKey::from_der(chain, key, &provider).map_err(|e| e.to_string())
}

impl ResolvesServerCert for Reloading {
    fn resolve(&self, _: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        let mut current = self.current.lock().unwrap_or_else(PoisonError::into_inner);
        let now = modified(&self.cert);
        if now != current.0 {
            match load(&self.cert, &self.key) {
                Ok(loaded) => *current = (now, Arc::new(loaded)),
                Err(error) => {
                    current.0 = now;
                    tracing::warn!(%error, "keeping the previous certificate");
                }
            }
        }
        Some(Arc::clone(&current.1))
    }
}
