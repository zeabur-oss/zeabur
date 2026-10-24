//! Join tokens for new workers: Kubernetes bootstrap tokens, as `rke2 token create` makes them.
//!
//! The token is a Secret in `kube-system`, so anyone allowed to create Secrets there (Zeabur, for
//! a signed-in admin) can make one through the API, and kube-controller-manager deletes it when it
//! expires. RKE2's supervisor accepts it on the agent routes only, so it cannot join a server.

use std::fs;
use std::io::Read as _;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use zeabur_installer_plan::plan::MANAGER;

use crate::HostError;

/// The group RKE2's supervisor lets join as an agent (k3s `NodeBootstrapTokenAuthGroup`).
pub const AUTH_GROUP: &str = "system:bootstrappers:rke2:default-node-token";
/// How long a token joins workers unless told otherwise.
pub const TTL: Duration = Duration::from_hours(1);
/// The longest lifetime `parse_ttl` accepts.
pub const MAX_TTL: Duration = Duration::from_hours(24);

/// A bootstrap token, `<id>.<secret>`. `Debug` leaves the secret out.
#[derive(Clone, PartialEq, Eq)]
pub struct BootstrapToken {
    /// Six lowercase letters or digits.
    pub id: String,
    /// Sixteen lowercase letters or digits.
    pub secret: String,
}

impl std::fmt::Debug for BootstrapToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BootstrapToken")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl BootstrapToken {
    /// A new random token.
    ///
    /// # Errors
    ///
    /// When `/dev/urandom` cannot be read.
    pub fn generate() -> Result<Self, HostError> {
        Ok(Self {
            id: random_chars(6)?,
            secret: random_chars(16)?,
        })
    }

    /// The token as RKE2 reads it, pinned to the cluster CA: `K10<ca-hash>::<id>.<secret>`.
    #[must_use]
    pub fn joining(&self, ca_hash: &str) -> String {
        format!("K10{ca_hash}::{}.{}", self.id, self.secret)
    }

    /// The Secret holding it, expiring at `expires` (RFC 3339).
    #[must_use]
    pub fn secret_object(&self, description: &str, expires: &str) -> Value {
        json!({
            "apiVersion": "v1",
            "kind": "Secret",
            "type": "bootstrap.kubernetes.io/token",
            "metadata": {
                "name": format!("bootstrap-token-{}", self.id),
                "namespace": "kube-system",
                "labels": {"app.kubernetes.io/managed-by": MANAGER},
            },
            "stringData": {
                "token-id": self.id,
                "token-secret": self.secret,
                "description": description,
                "expiration": expires,
                "usage-bootstrap-authentication": "true",
                "usage-bootstrap-signing": "true",
                "auth-extra-groups": AUTH_GROUP,
            },
        })
    }
}

/// A lifetime such as `30m` or `1h`, from one minute to a day.
#[must_use]
pub fn parse_ttl(text: &str) -> Option<Duration> {
    let text = text.trim();
    let (number, unit) = text.split_at(text.len().checked_sub(1)?);
    let n: u64 = number.parse().ok()?;
    let ttl = match unit {
        "m" => Duration::from_mins(n),
        "h" => Duration::from_hours(n),
        _ => return None,
    };
    (Duration::from_mins(1)..=MAX_TTL)
        .contains(&ttl)
        .then_some(ttl)
}

/// `n` characters of `[a-z0-9]`, without modulo bias.
fn random_chars(n: usize) -> Result<String, HostError> {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut file = fs::File::open("/dev/urandom").map_err(HostError::io("/dev/urandom"))?;
    let mut out = String::with_capacity(n);
    let mut byte = [0_u8; 1];
    while out.len() < n {
        file.read_exact(&mut byte)
            .map_err(HostError::io("/dev/urandom"))?;
        // 252 = 7 × 36: bytes above it would favour the first letters.
        if let Some(&c) = byte
            .first()
            .filter(|&&b| b < 252)
            .and_then(|&b| ALPHABET.get(usize::from(b % 36)))
        {
            out.push(char::from(c));
        }
    }
    Ok(out)
}

/// The CA hash of a join token, as k3s computes it for a CA file of one certificate: the SHA-256
/// of the PEM bytes exactly as stored. `None` for a bundle, whose hash is that of its root's DER.
#[must_use]
pub fn ca_hash(pem: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(pem).ok()?;
    if text.matches("-----BEGIN CERTIFICATE-----").count() != 1 {
        return None;
    }
    Some(hex::encode(Sha256::digest(pem)))
}

/// The CA hash a server token carries, `K10<hash>::server:<secret>`.
#[must_use]
pub fn hash_of_token(token: &str) -> Option<&str> {
    let (hash, _) = token.trim().strip_prefix("K10")?.split_once("::")?;
    (hash.len() == 64).then_some(hash)
}

/// `now + ttl` in RFC 3339, UTC, to the second.
#[must_use]
pub fn expires(ttl: Duration) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    rfc3339((now + ttl).as_secs())
}

/// Seconds since the epoch in RFC 3339, UTC.
#[must_use]
pub fn rfc3339(secs: u64) -> String {
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (y, m, d) = civil(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Days since 1970-01-01 to (year, month, day); Howard Hinnant's `civil_from_days`.
const fn civil(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_fit_the_bootstrap_pattern() {
        let t = BootstrapToken::generate().unwrap();
        let joining = t.joining(&"a".repeat(64));
        let credentials = joining.split_once("::").unwrap().1;
        assert!(zeabur_installer_plan::config::is_bootstrap_token(
            credentials
        ));
        assert!(!format!("{t:?}").contains(&t.secret));
        let secret = t.secret_object("zeabur-installer node add", "2026-10-24T08:00:00Z");
        assert_eq!(
            secret["metadata"]["name"],
            format!("bootstrap-token-{}", t.id)
        );
        assert_eq!(secret["type"], "bootstrap.kubernetes.io/token");
        assert_eq!(secret["stringData"]["auth-extra-groups"], AUTH_GROUP);
        assert_eq!(
            secret["stringData"]["usage-bootstrap-authentication"],
            "true"
        );
    }

    #[test]
    fn ca_hashes() {
        let pem = b"-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n";
        assert_eq!(
            ca_hash(pem).unwrap(),
            hex::encode(Sha256::digest(pem.as_slice()))
        );
        let bundle = [pem.as_slice(), pem.as_slice()].concat();
        assert_eq!(ca_hash(&bundle), None);
        let hash = "0123456789abcdef".repeat(4);
        assert_eq!(
            hash_of_token(&format!("K10{hash}::server:x\n")),
            Some(hash.as_str())
        );
        assert_eq!(hash_of_token("abcdef.0123456789abcdef"), None);
    }

    #[test]
    fn lifetimes() {
        assert_eq!(parse_ttl("30m"), Some(Duration::from_mins(30)));
        assert_eq!(parse_ttl("1h"), Some(Duration::from_hours(1)));
        assert_eq!(parse_ttl("24h"), Some(MAX_TTL));
        for bad in ["", "m", "0m", "25h", "90", "1d", "-1h", "1.5h"] {
            assert_eq!(parse_ttl(bad), None, "{bad}");
        }
    }

    #[test]
    fn dates() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339(1_792_713_600 + 3_661), "2026-10-23T01:01:01Z");
    }
}
