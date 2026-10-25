//! SSH deploy keys: an Ed25519 key pair in the formats Git hosts and OpenSSH read [ssh-key], and
//! the `known_hosts` lines that vouch for a Git host [sshd-known-hosts].
//!
//! The public key is one `authorized_keys` line (`ssh-ed25519 AAAA… comment`), what GitHub and
//! GitLab take as a deploy key. The private key is OpenSSH's own unencrypted format
//! (`-----BEGIN OPENSSH PRIVATE KEY-----`), which `ssh` and libraries such as russh read.
//!
//! Host keys are read as Zeabur Builder reads them: a plain host pattern (`host` on port 22,
//! `[host]:port` on any port), and only Ed25519 and ECDSA keys.
//!
//! [ssh-key]: https://cvsweb.openbsd.org/src/usr.bin/ssh/PROTOCOL.key?rev=HEAD
//! [sshd-known-hosts]: https://man.openbsd.org/sshd.8#SSH_KNOWN_HOSTS_FILE_FORMAT

use aws_lc_rs::digest::{SHA256, digest};
use aws_lc_rs::signature::{Ed25519KeyPair, KeyPair as _};
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};

use crate::crypto::{RandomError, random};

const KEY_TYPE: &[u8] = b"ssh-ed25519";
const MAGIC: &[u8] = b"openssh-key-v1\0";
const PEM_WIDTH: usize = 70;

/// A new key pair.
#[derive(Clone, PartialEq, Eq)]
pub struct DeployKey {
    /// `ssh-ed25519 AAAA… <comment>`.
    pub public: String,
    /// The private key, PEM.
    pub private: String,
}

impl std::fmt::Debug for DeployKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeployKey")
            .field("public", &self.public)
            .finish_non_exhaustive()
    }
}

fn string(out: &mut Vec<u8>, bytes: &[u8]) {
    let length = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(bytes);
}

fn public_blob(public: &[u8]) -> Vec<u8> {
    let mut blob = Vec::new();
    string(&mut blob, KEY_TYPE);
    string(&mut blob, public);
    blob
}

/// A fresh Ed25519 key pair; `comment` ends the public key's line and is kept in the private key.
///
/// # Errors
///
/// When the system random number generator fails.
pub fn deploy_key(comment: &str) -> Result<DeployKey, RandomError> {
    let seed = random::<32>()?;
    let pair = Ed25519KeyPair::from_seed_unchecked(&seed).map_err(|_rejected| RandomError)?;
    let public = pair.public_key().as_ref().to_vec();
    let check = random::<4>()?;
    let comment = comment.replace(['\n', '\r'], " ");

    let mut secret = Vec::new();
    secret.extend_from_slice(&check);
    secret.extend_from_slice(&check);
    string(&mut secret, KEY_TYPE);
    string(&mut secret, &public);
    let mut both = seed.to_vec();
    both.extend_from_slice(&public);
    string(&mut secret, &both);
    string(&mut secret, comment.as_bytes());
    let mut pad = 1_u8;
    while secret.len() % 8 != 0 {
        secret.push(pad);
        pad = pad.wrapping_add(1);
    }

    let mut body = MAGIC.to_vec();
    string(&mut body, b"none");
    string(&mut body, b"none");
    string(&mut body, b"");
    body.extend_from_slice(&1_u32.to_be_bytes());
    string(&mut body, &public_blob(&public));
    string(&mut body, &secret);

    let encoded = STANDARD.encode(&body);
    let mut private = String::from("-----BEGIN OPENSSH PRIVATE KEY-----\n");
    for line in encoded.as_bytes().chunks(PEM_WIDTH) {
        private.push_str(&String::from_utf8_lossy(line));
        private.push('\n');
    }
    private.push_str("-----END OPENSSH PRIVATE KEY-----\n");
    let line = format!(
        "ssh-ed25519 {} {comment}",
        STANDARD.encode(public_blob(&public))
    );
    Ok(DeployKey {
        public: line.trim_end().to_owned(),
        private,
    })
}

/// The `ssh-<type> AAAA…` public key held in an unencrypted OpenSSH private key, without its
/// comment; `None` for anything else.
#[must_use]
pub fn public_key(private: &str) -> Option<String> {
    let text: String = private
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("-----"))
        .collect();
    let bytes = STANDARD.decode(text).ok()?;
    let mut rest = bytes.strip_prefix(MAGIC)?;
    let (cipher, after) = field(rest)?;
    if cipher != b"none" {
        return None;
    }
    rest = after;
    for _ in 0..2 {
        rest = field(rest)?.1;
    }
    let (count, after) = rest.split_first_chunk::<4>()?;
    if u32::from_be_bytes(*count) != 1 {
        return None;
    }
    let (blob, _) = field(after)?;
    let kind = std::str::from_utf8(field(blob)?.0).ok()?;
    Some(format!("{kind} {}", STANDARD.encode(blob)))
}

/// The host key types Zeabur Builder accepts.
const HOST_KEY_TYPES: [&str; 4] = [
    "ssh-ed25519",
    "ecdsa-sha2-nistp256",
    "ecdsa-sha2-nistp384",
    "ecdsa-sha2-nistp521",
];
/// The most `known_hosts` text taken.
pub const KNOWN_HOSTS_MAX: usize = 16 * 1024;

/// A host key `known_hosts` lists for a host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKey {
    /// The line to write: the host pattern, the key type and the key.
    pub line: String,
    /// The key type, such as `ssh-ed25519`.
    pub algorithm: String,
    /// `SHA256:` and the key's digest in unpadded base64, as `ssh-keygen -l` prints it.
    pub fingerprint: String,
}

/// Why `known_hosts` vouches for no key of the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnownHostsError {
    /// Longer than [`KNOWN_HOSTS_MAX`].
    TooLong,
    /// The host names are hashed (`ssh-keyscan -H`), so they cannot be matched.
    Hashed,
    /// The host is listed only with keys of other types, such as RSA.
    Unsupported,
    /// No line names the host on the port.
    NotListed,
}

/// The host pattern `known_hosts` names `host` on `port` with.
#[must_use]
pub fn host_pattern(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_owned()
    } else {
        format!("[{host}]:{port}")
    }
}

/// The Ed25519 and ECDSA keys `known_hosts` lists for `host` on `port`, each as one line naming
/// the host as [`host_pattern`] does. Comments, markers (`@cert-authority`, `@revoked`) and other
/// hosts are left out.
///
/// # Errors
///
/// When no key of a type Zeabur Builder takes is listed for the host.
pub fn host_keys(
    known_hosts: &str,
    host: &str,
    port: u16,
) -> Result<Vec<HostKey>, KnownHostsError> {
    if known_hosts.len() > KNOWN_HOSTS_MAX {
        return Err(KnownHostsError::TooLong);
    }
    let pattern = host_pattern(host, port);
    let names = |patterns: &str| {
        let mut hit = false;
        for one in patterns.split(',') {
            let (negated, one) = one.strip_prefix('!').map_or((false, one), |n| (true, n));
            let matches = one.eq_ignore_ascii_case(&pattern)
                || (port == 22 && one.eq_ignore_ascii_case(&format!("[{host}]:22")));
            if matches && negated {
                return false;
            }
            hit |= matches;
        }
        hit
    };
    let (mut keys, mut hashed, mut other_types) = (Vec::new(), false, false);
    for line in known_hosts.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') || line.starts_with('@') {
            continue;
        }
        let mut fields = line.split_ascii_whitespace();
        let (Some(patterns), Some(algorithm), Some(key)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        hashed |= patterns.starts_with("|1|");
        if !names(patterns) {
            continue;
        }
        let Some(blob) = STANDARD
            .decode(key)
            .ok()
            .filter(|blob| field(blob).is_some_and(|(kind, _)| kind == algorithm.as_bytes()))
        else {
            other_types = true;
            continue;
        };
        if !HOST_KEY_TYPES.contains(&algorithm) {
            other_types = true;
            continue;
        }
        let fingerprint = format!(
            "SHA256:{}",
            STANDARD_NO_PAD.encode(digest(&SHA256, &blob).as_ref())
        );
        let line = format!("{pattern} {algorithm} {key}");
        if !keys.iter().any(|k: &HostKey| k.line == line) {
            keys.push(HostKey {
                line,
                algorithm: algorithm.to_owned(),
                fingerprint,
            });
        }
    }
    match (keys.is_empty(), other_types, hashed) {
        (false, ..) => Ok(keys),
        (true, true, _) => Err(KnownHostsError::Unsupported),
        (true, false, true) => Err(KnownHostsError::Hashed),
        (true, false, false) => Err(KnownHostsError::NotListed),
    }
}

/// One length-prefixed string and what follows it.
fn field(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    let (length, rest) = bytes.split_first_chunk::<4>()?;
    rest.split_at_checked(usize::try_from(u32::from_be_bytes(*length)).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(private: &str) -> Vec<u8> {
        let text: String = private
            .lines()
            .filter(|l| !l.starts_with("-----"))
            .collect();
        STANDARD.decode(text).unwrap()
    }

    #[test]
    fn the_public_key_is_one_authorized_keys_line() {
        let key = deploy_key("zeabur shop/web").unwrap();
        let mut parts = key.public.splitn(3, ' ');
        assert_eq!(parts.next(), Some("ssh-ed25519"));
        let blob = STANDARD.decode(parts.next().unwrap()).unwrap();
        assert_eq!(blob.len(), 4 + 11 + 4 + 32);
        assert_eq!(parts.next(), Some("zeabur shop/web"));
        assert_ne!(key, deploy_key("zeabur shop/web").unwrap());
    }

    #[test]
    fn the_private_key_holds_the_same_public_key_and_checks() {
        let key = deploy_key("c").unwrap();
        assert!(
            key.private
                .starts_with("-----BEGIN OPENSSH PRIVATE KEY-----\n")
        );
        assert!(key.private.ends_with("-----END OPENSSH PRIVATE KEY-----\n"));
        assert!(key.private.lines().all(|l| l.len() <= PEM_WIDTH + 5));
        let bytes = body(&key.private);
        assert!(bytes.starts_with(MAGIC));
        let blob = STANDARD
            .decode(key.public.split(' ').nth(1).unwrap())
            .unwrap();
        let at = bytes
            .windows(blob.len())
            .position(|w| w == blob.as_slice())
            .unwrap();
        assert!(at > MAGIC.len());
        let secret_start = at + blob.len() + 4;
        let secret = &bytes[secret_start..];
        assert_eq!(secret.len() % 8, 0);
        assert_eq!(secret[0..4], secret[4..8]);
        let seed = &secret[8 + 4 + 11 + 4 + 32 + 4..][..32];
        let pair = Ed25519KeyPair::from_seed_unchecked(seed).unwrap();
        assert_eq!(pair.public_key().as_ref(), &blob[blob.len() - 32..]);
    }

    #[test]
    fn the_public_key_comes_back_from_the_private_key() {
        let key = deploy_key("zeabur shop").unwrap();
        let public = public_key(&key.private).unwrap();
        assert_eq!(format!("{public} zeabur shop"), key.public);
        assert_eq!(public_key("not a key"), None);
        assert_eq!(
            public_key(&key.private.replace("b3BlbnNzaC1rZXktdjE", "AAAA")),
            None
        );
    }

    /// GitHub's published Ed25519 host key and its fingerprint (`https://api.github.com/meta`).
    const GITHUB: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";

    #[test]
    fn host_keys_keep_the_hosts_ed25519_and_ecdsa_lines_with_their_fingerprints() {
        let text = format!(
            "# git.example.com:2222 SSH-2.0-OpenSSH_9.6\n\
             [git.example.com]:2222 ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQ\n\
             [git.example.com]:2222 {GITHUB}\n\
             [git.example.com]:2222 {GITHUB}\n\
             other.example.com {GITHUB}\n"
        );
        let keys = host_keys(&text, "git.example.com", 2222).unwrap();
        assert_eq!(
            keys,
            [HostKey {
                line: format!("[git.example.com]:2222 {GITHUB}"),
                algorithm: "ssh-ed25519".to_owned(),
                fingerprint: "SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU".to_owned(),
            }]
        );
        let plain = host_keys(
            &format!("Git.Example.com,10.0.0.1 {GITHUB}"),
            "git.example.com",
            22,
        )
        .unwrap();
        assert_eq!(plain[0].line, format!("git.example.com {GITHUB}"));
    }

    #[test]
    fn host_keys_say_why_nothing_vouches_for_the_host() {
        let rsa = "git.example.com ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQ";
        assert_eq!(
            host_keys(rsa, "git.example.com", 22),
            Err(KnownHostsError::Unsupported)
        );
        let hashed = format!("|1|aGFzaA==|c2FsdA== {GITHUB}");
        assert_eq!(
            host_keys(&hashed, "git.example.com", 22),
            Err(KnownHostsError::Hashed)
        );
        let elsewhere = format!("git.example.com {GITHUB}");
        assert_eq!(
            host_keys(&elsewhere, "git.example.com", 2222),
            Err(KnownHostsError::NotListed)
        );
        let negated = format!("*.example.com,!git.example.com {GITHUB}");
        assert_eq!(
            host_keys(&negated, "git.example.com", 22),
            Err(KnownHostsError::NotListed)
        );
        let mislabelled = format!(
            "git.example.com ecdsa-sha2-nistp256 {}",
            GITHUB.split(' ').nth(1).unwrap()
        );
        assert_eq!(
            host_keys(&mislabelled, "git.example.com", 22),
            Err(KnownHostsError::Unsupported)
        );
        assert_eq!(
            host_keys(&" ".repeat(KNOWN_HOSTS_MAX + 1), "git.example.com", 22),
            Err(KnownHostsError::TooLong)
        );
    }
}
