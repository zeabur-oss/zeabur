//! The cluster's CA and one certificate for both host names, made with openssl and kept in
//! `target/dev-cluster/tls` until `down`.

use std::fs;
use std::path::Path;

use crate::process::{Cmd, random_hex};

/// Days the CA and the certificate are valid.
const DAYS: &str = "30";

/// Makes `ca.crt`, `ca.key`, `tls.crt` and `tls.key` in `dir` for `hosts`, unless `tls.crt` is
/// there.
pub(crate) fn ensure(dir: &Path, hosts: &[&str]) -> Result<(), String> {
    if dir.join("tls.crt").is_file() {
        return Ok(());
    }
    make(dir, hosts).map_err(|e| format!("making the cluster's certificates: {e}"))
}

fn make(dir: &Path, hosts: &[&str]) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    let ec = [
        "-newkey",
        "ec",
        "-pkeyopt",
        "ec_paramgen_curve:P-256",
        "-nodes",
    ];
    openssl(dir)
        .args(["req", "-x509"])
        .args(ec)
        .args(["-days", DAYS, "-subj", "/CN=Zeabur dev CA"])
        .args(["-keyout", "ca.key", "-out", "ca.crt"])
        .args(["-addext", "basicConstraints=critical,CA:TRUE"])
        .args(["-addext", "keyUsage=critical,keyCertSign"])
        .probe()?;
    openssl(dir)
        .args(["req", "-new"])
        .args(ec)
        .args([
            "-subj",
            "/CN=zeabur dev",
            "-keyout",
            "tls.key",
            "-out",
            "tls.csr",
        ])
        .probe()?;
    let ext = dir.join("ext.cnf");
    fs::write(&ext, extensions(hosts)).map_err(|e| format!("writing {}: {e}", ext.display()))?;
    openssl(dir)
        .args([
            "x509", "-req", "-in", "tls.csr", "-CA", "ca.crt", "-CAkey", "ca.key",
        ])
        .args(["-days", DAYS, "-extfile", "ext.cnf", "-out", "tls.crt"])
        .arg("-set_serial")
        .arg(format!("0x{}", random_hex(16)?))
        .probe()
        .map(drop)
}

fn openssl(dir: &Path) -> Cmd {
    Cmd::new("openssl").current_dir(dir)
}

/// The server certificate's extensions.
fn extensions(hosts: &[&str]) -> String {
    let names: Vec<String> = hosts.iter().map(|host| format!("DNS:{host}")).collect();
    format!(
        "subjectAltName={}\nextendedKeyUsage=serverAuth\nbasicConstraints=critical,CA:FALSE\n",
        names.join(",")
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::ensure;
    use crate::process::Cmd;

    const HOSTS: &[&str] = &["identity.localhost", "zeabur.localhost"];

    fn verify(dir: &std::path::Path, host: &str) -> Result<String, String> {
        Cmd::new("openssl")
            .current_dir(dir)
            .args(["verify", "-CAfile", "ca.crt", "-purpose", "sslserver"])
            .args(["-verify_hostname", host, "tls.crt"])
            .probe()
    }

    #[test]
    fn the_certificate_is_trusted_for_both_hosts_through_the_ca() {
        let dir = TempDir::new().unwrap();
        ensure(dir.path(), HOSTS).unwrap();
        for host in HOSTS {
            verify(dir.path(), host).unwrap();
        }
        assert!(verify(dir.path(), "other.localhost").is_err());
    }

    #[test]
    fn a_second_run_keeps_the_ca_and_certificate() {
        let dir = TempDir::new().unwrap();
        ensure(dir.path(), HOSTS).unwrap();
        let ca = fs::read(dir.path().join("ca.crt")).unwrap();
        let cert = fs::read(dir.path().join("tls.crt")).unwrap();
        ensure(dir.path(), HOSTS).unwrap();
        assert_eq!(fs::read(dir.path().join("ca.crt")).unwrap(), ca);
        assert_eq!(fs::read(dir.path().join("tls.crt")).unwrap(), cert);
    }
}
