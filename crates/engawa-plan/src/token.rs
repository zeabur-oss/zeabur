//! The plan token: what makes an apply carry out exactly the plan a person confirmed.
//!
//! A token is `v2.<sealed>`: the claims as JSON, encrypted and authenticated with AES-256-GCM
//! [sp800-38d] under a key only the server holds, as an AEAD [rfc5116], then base64url without
//! padding. The claims bind the person (user name and groups), the project, the request as made,
//! the `resourceVersion` of every object it writes, when it was made (the restart annotation's
//! value) and when it expires. Nothing is stored on the server: apply opens the token, checks the
//! expiry, reads the project again, refuses when any of those objects changed, and computes the
//! same writes from the claims.
//!
//! The claims are encrypted, not only signed, because the request may hold a Secret value the
//! person typed: the token travels through the browser, its history and logs, and must not carry
//! that value in a readable form. The version prefix is the associated data, so a token of
//! another format never opens as this one.
//!
//! [rfc5116]: https://www.rfc-editor.org/rfc/rfc5116
//! [sp800-38d]: https://doi.org/10.6028/NIST.SP.800-38D

use std::collections::BTreeMap;
use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use engawa_core::crypto::{RandomError, open, random, seal};
use serde::{Deserialize, Serialize};

use crate::change::PlanRequest;

/// How long a plan may be applied after it was made, in seconds.
pub const LIFETIME_SECONDS: i64 = 600;

const PREFIX: &str = "v2.";

/// The key plan tokens are sealed with (AES-256-GCM, 32 bytes).
#[derive(Clone)]
pub struct PlanKey([u8; 32]);

impl fmt::Debug for PlanKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PlanKey(…)")
    }
}

impl PlanKey {
    /// A fresh random key. Local mode makes one per start, so tokens die with the process.
    ///
    /// # Errors
    ///
    /// When the system random number generator fails.
    pub fn random() -> Result<Self, RandomError> {
        Ok(Self(random::<32>()?))
    }

    /// A key from stored bytes (the in-cluster server keeps one in a Secret).
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

/// What a token binds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Claims {
    /// The person's user name.
    pub person: String,
    /// The person's groups.
    pub groups: Vec<String>,
    /// The project (namespace).
    pub project: String,
    /// The request as made.
    pub request: PlanRequest,
    /// `resourceVersion` of each object the plan writes, by `kind/name`.
    pub versions: BTreeMap<String, String>,
    /// When the plan was made, RFC 3339.
    pub made_at: String,
    /// When it expires, in seconds since the Unix epoch.
    pub expires_at: i64,
}

/// Why a token is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TokenError {
    /// It is not a token Engawa made.
    #[error("the plan token is malformed")]
    Malformed,
    /// It does not open: it was altered, or made under another key.
    #[error("the plan token was altered or is not from this server")]
    Altered,
    /// It is past its expiry.
    #[error("the plan expired")]
    Expired,
}

/// Seals `claims` into a token.
///
/// # Errors
///
/// When the system random number generator fails (each token has a fresh nonce).
pub fn sign(key: &PlanKey, claims: &Claims) -> Result<String, RandomError> {
    let json = serde_json::to_vec(claims).unwrap_or_default();
    let sealed = seal(&key.0, PREFIX.as_bytes(), &json)?;
    Ok(format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(sealed)))
}

/// The claims of `token`, if it opens under `key` and has not expired at `now` (seconds since
/// the Unix epoch).
///
/// # Errors
///
/// [`TokenError`].
pub fn verify(key: &PlanKey, token: &str, now: i64) -> Result<Claims, TokenError> {
    let rest = token.strip_prefix(PREFIX).ok_or(TokenError::Malformed)?;
    let sealed = URL_SAFE_NO_PAD
        .decode(rest)
        .map_err(|_e| TokenError::Malformed)?;
    let json = open(&key.0, PREFIX.as_bytes(), &sealed).ok_or(TokenError::Altered)?;
    let claims: Claims = serde_json::from_slice(&json).map_err(|_e| TokenError::Malformed)?;
    if now >= claims.expires_at {
        return Err(TokenError::Expired);
    }
    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change::{Change, DataEdit, DataKind};

    fn claims() -> Claims {
        Claims {
            person: "alice".to_owned(),
            groups: vec!["dev".to_owned()],
            project: "shop".to_owned(),
            request: PlanRequest {
                changes: Vec::new(),
                restart_after: false,
            },
            versions: BTreeMap::from([("Deployment/web".to_owned(), "41".to_owned())]),
            made_at: "2026-10-15T00:00:00Z".to_owned(),
            expires_at: 1_000,
        }
    }

    #[test]
    fn a_token_verifies_until_it_expires() {
        let key = PlanKey::random().unwrap();
        let token = sign(&key, &claims()).unwrap();
        assert!(token.starts_with(PREFIX));
        assert_eq!(verify(&key, &token, 999).unwrap(), claims());
        assert_eq!(verify(&key, &token, 1_000), Err(TokenError::Expired));
    }

    #[test]
    fn a_tampered_or_foreign_token_is_refused() {
        let key = PlanKey::random().unwrap();
        let token = sign(&key, &claims()).unwrap();
        // A token from another key (another process).
        let elsewhere = PlanKey::random().unwrap();
        assert_eq!(verify(&elsewhere, &token, 0), Err(TokenError::Altered));
        // Garbage, a wrong prefix, or a truncated token.
        assert_eq!(verify(&key, "v2.abc", 0), Err(TokenError::Altered));
        assert_eq!(verify(&key, "v2.!!", 0), Err(TokenError::Malformed));
        assert_eq!(verify(&key, "nope", 0), Err(TokenError::Malformed));
        let v1 = format!("v1.{}", token.strip_prefix(PREFIX).unwrap());
        assert_eq!(verify(&key, &v1, 0), Err(TokenError::Malformed));
        // Every single changed byte of the sealed part is caught.
        let sealed = URL_SAFE_NO_PAD
            .decode(token.strip_prefix(PREFIX).unwrap())
            .unwrap();
        for i in 0..sealed.len() {
            let mut altered = sealed.clone();
            if let Some(byte) = altered.get_mut(i) {
                *byte ^= 0x01;
            }
            let forged = format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(&altered));
            assert_eq!(
                verify(&key, &forged, 0),
                Err(TokenError::Altered),
                "byte {i}"
            );
        }
    }

    #[test]
    fn a_secret_value_never_appears_in_the_token() {
        let secret = "s3cr3t-p4ssw0rd-value";
        let mut with_secret = claims();
        with_secret.request = PlanRequest {
            changes: vec![Change::Data {
                kind: DataKind::Secret,
                name: "db".to_owned(),
                changes: vec![DataEdit {
                    op: "set".to_owned(),
                    key: "PASSWORD".to_owned(),
                    value: Some(secret.to_owned()),
                }],
            }],
            restart_after: false,
        };
        let key = PlanKey::random().unwrap();
        let token = sign(&key, &with_secret).unwrap();
        let bytes = URL_SAFE_NO_PAD
            .decode(token.strip_prefix(PREFIX).unwrap())
            .unwrap();
        let needle = secret.as_bytes();
        assert!(!bytes.windows(needle.len()).any(|w| w == needle));
        assert!(!token.contains(secret));
        // Nor in its base64 or in the other readable claims (person, project).
        assert!(!bytes.windows(5).any(|w| w == b"alice"));
        assert_eq!(verify(&key, &token, 0).unwrap(), with_secret);
    }
}
