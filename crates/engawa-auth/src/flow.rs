//! A sign-in in progress, between sending the browser to the issuer and its return.
//!
//! [`Pending`] holds the `state` value, the PKCE verifier and what the sign-in is for, sealed
//! with AES-256-GCM under a key made when the process starts, and travels in a cookie that
//! lasts ten minutes. Nothing is stored on the server for a visitor who has not signed in, so
//! starting sign-ins cannot fill memory; Genkan keeps its own unauthenticated flows the same way.
//!
//! - **`state`** binds the answer to this browser: the callback is accepted only when the `state`
//!   it carries equals the one in the cookie (compared in constant time), which stops a forged
//!   answer from signing the person in as someone else (login CSRF, [rfc6749] §10.12, [rfc9700]
//!   §4.7.1).
//! - **PKCE S256** [rfc7636] binds the code to this server's request, so a code stolen on the way
//!   back cannot be redeemed elsewhere (code injection, [rfc9700] §4.5.3.1); RFC 9700 recommends it
//!   for confidential clients too (§2.1.1), and Genkan requires it of every client.
//! - **Where to go afterwards** is a path on this server only ([`return_path`]): a sign-in link
//!   cannot send the person to another site (the client as an open redirector, [rfc9700] §4.11.1).
//!
//! [rfc6749]: https://www.rfc-editor.org/rfc/rfc6749
//! [rfc7636]: https://www.rfc-editor.org/rfc/rfc7636
//! [rfc9700]: https://www.rfc-editor.org/rfc/rfc9700

use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use engawa_core::crypto::{RandomError, equal, open, random, random_token, seal, sha256};
use serde::{Deserialize, Serialize};

/// How long a sign-in may take, in seconds.
pub const LIFETIME_SECONDS: i64 = 600;
/// The longest return path kept; anything longer goes to `/`.
const MAX_RETURN: usize = 2048;
/// The associated data of a sealed flow, so no other sealed value opens as one.
const AAD: &[u8] = b"engawa-flow-v1";

/// What a sign-in is for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "for", rename_all = "camelCase")]
pub enum Purpose {
    /// Signing in; afterwards the browser goes to `return_to`, a path on this server.
    SignIn {
        /// Where to go afterwards.
        return_to: String,
    },
    /// Asking for administrator rights, in a separate window, for a person already signed in.
    Elevate,
}

impl Purpose {
    /// The scopes to ask for: `elevate` adds the person's elevated groups to a short-lived
    /// token (Genkan's `docs/managing-users.md`, "Admin rights").
    #[must_use]
    pub const fn scope(&self) -> &'static str {
        match self {
            Self::SignIn { .. } => "openid",
            Self::Elevate => "openid elevate",
        }
    }
}

/// The key flows are sealed with, made when the process starts: a sign-in in progress does not
/// survive a restart, and needs not.
#[derive(Clone)]
pub struct FlowKey([u8; 32]);

impl fmt::Debug for FlowKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FlowKey(…)")
    }
}

impl FlowKey {
    /// A fresh key.
    ///
    /// # Errors
    ///
    /// When the system random number generator fails.
    pub fn random() -> Result<Self, RandomError> {
        Ok(Self(random::<32>()?))
    }
}

/// A sign-in in progress.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending {
    state: String,
    verifier: String,
    purpose: Purpose,
    expires_at: i64,
}

impl fmt::Debug for Pending {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pending")
            .field("purpose", &self.purpose)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

impl Pending {
    /// A new sign-in for `purpose`, at `now` (seconds since the epoch).
    ///
    /// # Errors
    ///
    /// When the system random number generator fails.
    pub fn start(purpose: Purpose, now: i64) -> Result<Self, RandomError> {
        Ok(Self {
            // 256 bits each; RFC 7636 §7.1 asks for at least 256 bits of entropy in the verifier,
            // which is 43 characters of base64url (§4.1).
            state: random_token::<32>()?,
            verifier: URL_SAFE_NO_PAD.encode(random::<32>()?),
            purpose,
            expires_at: now.saturating_add(LIFETIME_SECONDS),
        })
    }

    /// The `state` value to send.
    #[must_use]
    pub fn state(&self) -> &str {
        &self.state
    }

    /// The PKCE verifier, sent with the code.
    #[must_use]
    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    /// The PKCE S256 challenge: `BASE64URL(SHA256(verifier))` ([rfc7636] §4.2).
    ///
    /// [rfc7636]: https://www.rfc-editor.org/rfc/rfc7636
    #[must_use]
    pub fn challenge(&self) -> String {
        URL_SAFE_NO_PAD.encode(sha256(self.verifier.as_bytes()))
    }

    /// What the sign-in is for.
    #[must_use]
    pub const fn purpose(&self) -> &Purpose {
        &self.purpose
    }

    /// Whether `state`, from the callback, is this sign-in's; in constant time.
    #[must_use]
    pub fn matches(&self, state: &str) -> bool {
        equal(state.as_bytes(), self.state.as_bytes())
    }

    /// The flow, sealed for its cookie.
    ///
    /// # Errors
    ///
    /// When the system random number generator fails.
    pub fn seal(&self, key: &FlowKey) -> Result<String, RandomError> {
        let json = serde_json::to_vec(self).map_err(|_serialize| RandomError)?;
        Ok(URL_SAFE_NO_PAD.encode(seal(&key.0, AAD, &json)?))
    }

    /// The flow in `sealed`, unless it was altered, sealed under another key, or has expired at
    /// `now`.
    #[must_use]
    pub fn open(key: &FlowKey, sealed: &str, now: i64) -> Option<Self> {
        let bytes = URL_SAFE_NO_PAD.decode(sealed).ok()?;
        let json = open(&key.0, AAD, &bytes)?;
        let pending: Self = serde_json::from_slice(&json).ok()?;
        (now < pending.expires_at).then_some(pending)
    }
}

/// `raw` when it is a path on this server, otherwise `/`.
///
/// A return path is followed after sign-in, so it must not lead to another site: it starts with
/// one `/` and no second `/` or `\` (browsers read `//host` and `/\host` as another host), and
/// holds only visible ASCII (no spaces, controls or line breaks; browsers send the rest
/// percent-encoded).
#[must_use]
pub fn return_path(raw: Option<&str>) -> String {
    let Some(raw) = raw else {
        return "/".to_owned();
    };
    let mut bytes = raw.bytes();
    let starts_well = bytes.next() == Some(b'/') && !matches!(bytes.next(), Some(b'/' | b'\\'));
    let visible = raw
        .bytes()
        .all(|byte| byte.is_ascii_graphic() && byte != b'\\');
    if starts_well && visible && raw.len() <= MAX_RETURN {
        raw.to_owned()
    } else {
        "/".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn the_challenge_is_s256_of_the_verifier() {
        let pending = Pending::start(Purpose::Elevate, NOW).unwrap();
        assert_eq!(pending.verifier().len(), 43);
        assert_eq!(
            pending.challenge(),
            URL_SAFE_NO_PAD.encode(sha256(pending.verifier().as_bytes()))
        );
        // RFC 7636 Appendix B.
        let example = Pending {
            verifier: "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".to_owned(),
            ..pending
        };
        assert_eq!(
            example.challenge(),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn a_sealed_flow_opens_only_unaltered_under_its_key_and_before_it_expires() {
        let key = FlowKey::random().unwrap();
        let pending = Pending::start(
            Purpose::SignIn {
                return_to: "/projects/shop".to_owned(),
            },
            NOW,
        )
        .unwrap();
        let sealed = pending.seal(&key).unwrap();
        assert!(!sealed.contains(pending.verifier()));
        assert_eq!(Pending::open(&key, &sealed, NOW + 1), Some(pending));
        assert_eq!(Pending::open(&key, &sealed, NOW + LIFETIME_SECONDS), None);
        assert_eq!(
            Pending::open(&FlowKey::random().unwrap(), &sealed, NOW),
            None
        );
        let mut altered = sealed.into_bytes();
        if let Some(byte) = altered.get_mut(20) {
            *byte = if *byte == b'A' { b'B' } else { b'A' };
        }
        assert_eq!(
            Pending::open(&key, &String::from_utf8(altered).unwrap(), NOW),
            None
        );
    }

    #[test]
    fn state_matches_only_itself() {
        let pending = Pending::start(Purpose::Elevate, NOW).unwrap();
        let state = pending.state().to_owned();
        assert!(pending.matches(&state));
        assert!(!pending.matches(""));
        assert!(!pending.matches(state.get(..63).unwrap()));
        assert!(!pending.matches(&Pending::start(Purpose::Elevate, NOW).unwrap().state));
    }

    #[test]
    fn only_paths_on_this_server_are_returned_to() {
        for good in ["/", "/projects/shop?tab=logs", "/settings#about", "/a%20b"] {
            assert_eq!(return_path(Some(good)), good);
        }
        for bad in [
            "https://attacker.example/",
            "//attacker.example/",
            "/\\attacker.example/",
            "\\\\attacker.example",
            "javascript:alert(1)",
            "projects",
            "/a b",
            "/a\r\nSet-Cookie: x=y",
            "/\u{3000}",
            "",
        ] {
            assert_eq!(return_path(Some(bad)), "/", "{bad:?}");
        }
        assert_eq!(return_path(None), "/");
        assert_eq!(return_path(Some(&format!("/{}", "a".repeat(3000)))), "/");
    }
}
