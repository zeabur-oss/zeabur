//! SSH deploy keys: an Ed25519 key pair in the formats Git hosts and OpenSSH read [ssh-key].
//!
//! The public key is one `authorized_keys` line (`ssh-ed25519 AAAA… comment`), what GitHub and
//! GitLab take as a deploy key. The private key is OpenSSH's own unencrypted format
//! (`-----BEGIN OPENSSH PRIVATE KEY-----`), which `ssh` and libraries such as russh read.
//!
//! [ssh-key]: https://cvsweb.openbsd.org/src/usr.bin/ssh/PROTOCOL.key?rev=HEAD

use aws_lc_rs::signature::{Ed25519KeyPair, KeyPair as _};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

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
}
