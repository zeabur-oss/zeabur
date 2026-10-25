//! Sessions: who is signed in, held in this process's memory.
//!
//! - **The browser holds an opaque identifier**, 256 random bits in the `__Host-zeabur` cookie
//!   ([`crate::cookie`]); the tokens never leave the process. OWASP asks for at least 64 bits of
//!   entropy in a session identifier and for its content to be meaningless [owasp-session].
//! - **The store keeps a hash of the identifier**, not the identifier: a lookup hashes what the
//!   browser presented (SHA-256) and finds the session by that hash, then compares the hashes in
//!   constant time. How long a lookup takes depends only on a hash of the cookie, which tells an
//!   attacker nothing about a valid one, and a copy of the process's memory holds no usable cookie.
//! - **Renewal before expiry, one at a time.** A request within [`Limits::refresh_margin`] of the
//!   access token's expiry renews it with the refresh token first. Zeabur Identity rotates refresh
//!   tokens and revokes the whole family when a rotated one is used again ([rfc9700] §4.14.2), so
//!   two concurrent renewals of one session would end it: each session has its own async lock, and
//!   a request that waited on it finds the token already renewed. An issuer that refuses
//!   (`invalid_grant`: revoked, expired, user disabled) ends the session; an issuer that cannot be
//!   reached does not, while the current token lasts.
//! - **Idle and absolute limits** ([`Limits`]): a session unused for `idle`, or older than
//!   `absolute`, is over, whatever its tokens say [owasp-session]. Expired sessions are swept
//!   ([`Sessions::sweep`]).
//! - **Administrator rights** are a second, short-lived access token in the same session, never
//!   renewed (Zeabur Identity issues elevated tokens without a refresh token). It is dropped when
//!   it expires, when the person drops it, and with the session.
//!
//! **One replica.** Sessions live in one process, so the server runs as a single replica; a
//! restart signs everyone out, which with passkeys costs one touch. Sharing sessions between
//! replicas would mean storing refresh tokens somewhere every replica reads: Kubernetes objects
//! are Zeabur's only state, so that would be a Secret in etcd, where refresh tokens for every
//! person would rest in a place readable by whoever can read Secrets in Zeabur's namespace,
//! and would outlive the process. Keeping them in memory keeps them as short-lived as the
//! process.
//!
//! # References
//!
//! - [owasp-session] OWASP Foundation, "Session Management Cheat Sheet," OWASP Cheat Sheet Series. <https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html>
//! - [rfc9700] T. Lodderstedt, J. Bradley, A. Labunets and D. Fett, "Best Current Practice for
//!   OAuth 2.0 Security," RFC 9700, IETF, Jan. 2025. <https://www.rfc-editor.org/rfc/rfc9700>
//!
//! [owasp-session]: https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html
//! [rfc9700]: https://www.rfc-editor.org/rfc/rfc9700

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use secrecy::SecretString;
use zeabur_core::crypto::{RandomError, equal, random_token, sha256};

use crate::client::{AuthError, OAuthClient, TokenSet};

/// The time now, in seconds since the epoch.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// Makes what a request needs from an access token (in the server, a client for kube-apiserver
/// bound to it). The token is not kept otherwise.
pub type Bind<T> = Arc<dyn Fn(&SecretString) -> Result<T, String> + Send + Sync>;

/// How long sessions last, in seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// A session not used for this long is over.
    pub idle: i64,
    /// A session this old is over, used or not.
    pub absolute: i64,
    /// An access token this close to its expiry is renewed before use.
    pub refresh_margin: i64,
}

impl Default for Limits {
    /// Thirty minutes idle and eight hours in all, OWASP's ranges for an application of this
    /// weight [owasp-session] ("Session Expiration"), and renewal a minute ahead of expiry.
    ///
    /// [owasp-session]: https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html#session-expiration
    fn default() -> Self {
        Self {
            idle: 30 * 60,
            absolute: 8 * 60 * 60,
            refresh_margin: 60,
        }
    }
}

/// A credential and when it expires.
#[derive(Debug, Clone)]
pub struct Bound<T> {
    /// What [`Bind`] made of the token.
    pub value: T,
    /// When the token expires, in seconds since the epoch.
    pub expires_at: i64,
}

/// What a request may use of a session.
#[derive(Debug, Clone)]
pub struct Credentials<T> {
    /// The person's user name, as the cluster gave it at sign-in.
    pub username: String,
    /// The everyday credential.
    pub base: Bound<T>,
    /// The administrator credential, while it lasts.
    pub elevated: Option<Bound<T>>,
    /// Whether asking for administrator rights would succeed, by the last ID token.
    pub can_elevate: bool,
}

/// The outcome of looking a session up.
#[derive(Debug, Clone)]
pub enum Lookup<T> {
    /// Signed in.
    Active(Credentials<T>),
    /// No such session, or it is over.
    SignedOut,
    /// The access token expired and the issuer could not be reached to renew it.
    IssuerUnavailable,
}

/// Why a session could not be started or changed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SessionError {
    /// The random number generator failed.
    #[error(transparent)]
    Random(#[from] RandomError),
    /// [`Bind`] refused the token.
    #[error("cannot use the token: {0}")]
    Bind(String),
    /// No such session.
    #[error("the session is over")]
    SignedOut,
}

struct State<T> {
    username: String,
    last_seen: i64,
    base: Bound<T>,
    refresh: Option<SecretString>,
    elevated: Option<Bound<T>>,
    can_elevate: bool,
}

struct Entry<T> {
    hash: [u8; 32],
    created: i64,
    state: Mutex<State<T>>,
    /// Held while the session's token is renewed.
    renewing: tokio::sync::Mutex<()>,
}

impl<T> Entry<T> {
    fn state(&self) -> MutexGuard<'_, State<T>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The sessions of this process.
pub struct Sessions<T> {
    entries: Mutex<HashMap<[u8; 32], Arc<Entry<T>>>>,
    oauth: Arc<OAuthClient>,
    bind: Bind<T>,
    limits: Limits,
    clock: Clock,
}

impl<T> fmt::Debug for Sessions<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sessions")
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

impl<T: Clone + Send + Sync + 'static> Sessions<T> {
    /// An empty store, renewing tokens through `oauth` and binding them with `bind`.
    #[must_use]
    pub fn new(oauth: Arc<OAuthClient>, bind: Bind<T>, limits: Limits, clock: Clock) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            oauth,
            bind,
            limits,
            clock,
        }
    }

    fn entries(&self) -> MutexGuard<'_, HashMap<[u8; 32], Arc<Entry<T>>>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn now(&self) -> i64 {
        (self.clock)()
    }

    fn bound(&self, token: &SecretString, expires_in: i64) -> Result<Bound<T>, SessionError> {
        Ok(Bound {
            value: (self.bind)(token).map_err(SessionError::Bind)?,
            expires_at: self.now().saturating_add(expires_in),
        })
    }

    /// Starts a session for `username` with `tokens`; returns the identifier for the cookie.
    ///
    /// # Errors
    ///
    /// When the token cannot be bound or the random number generator fails.
    pub fn start(&self, username: String, tokens: TokenSet) -> Result<String, SessionError> {
        let base = self.bound(&tokens.access_token, tokens.expires_in)?;
        let id = random_token::<32>()?;
        let hash = sha256(id.as_bytes());
        let now = self.now();
        let entry = Arc::new(Entry {
            hash,
            created: now,
            state: Mutex::new(State {
                username,
                last_seen: now,
                base,
                refresh: tokens.refresh_token,
                elevated: None,
                can_elevate: tokens.can_elevate,
            }),
            renewing: tokio::sync::Mutex::new(()),
        });
        self.entries().insert(hash, entry);
        Ok(id)
    }

    fn find(&self, id: &str) -> Option<Arc<Entry<T>>> {
        let hash = sha256(id.as_bytes());
        let entry = self.entries().get(&hash).cloned()?;
        equal(&entry.hash, &hash).then_some(entry)
    }

    fn over(&self, entry: &Entry<T>, now: i64) -> bool {
        let last_seen = entry.state().last_seen;
        now >= entry.created.saturating_add(self.limits.absolute)
            || now >= last_seen.saturating_add(self.limits.idle)
    }

    fn end(&self, entry: &Entry<T>) {
        self.entries().remove(&entry.hash);
    }

    fn credentials(entry: &Entry<T>, now: i64) -> Credentials<T> {
        let mut state = entry.state();
        state.last_seen = now;
        if state
            .elevated
            .as_ref()
            .is_some_and(|elevated| now >= elevated.expires_at)
        {
            state.elevated = None;
        }
        Credentials {
            username: state.username.clone(),
            base: state.base.clone(),
            elevated: state.elevated.clone(),
            can_elevate: state.can_elevate,
        }
    }

    /// The session `id` names, its token renewed when it is about to expire.
    pub async fn current(&self, id: &str) -> Lookup<T> {
        let Some(entry) = self.find(id) else {
            return Lookup::SignedOut;
        };
        let now = self.now();
        if self.over(&entry, now) {
            self.end(&entry);
            return Lookup::SignedOut;
        }
        let fresh = |entry: &Entry<T>, now: i64| {
            now < entry
                .state()
                .base
                .expires_at
                .saturating_sub(self.limits.refresh_margin)
        };
        if fresh(&entry, now) {
            return Lookup::Active(Self::credentials(&entry, now));
        }
        let _renewing = entry.renewing.lock().await;
        let now = self.now();
        // Another request renewed it while this one waited.
        if fresh(&entry, now) {
            return Lookup::Active(Self::credentials(&entry, now));
        }
        let (refresh, expires_at) = {
            let state = entry.state();
            (state.refresh.clone(), state.base.expires_at)
        };
        let still_valid = now < expires_at;
        let Some(refresh) = refresh else {
            return if still_valid {
                Lookup::Active(Self::credentials(&entry, now))
            } else {
                self.end(&entry);
                Lookup::SignedOut
            };
        };
        match self.oauth.refresh(&refresh).await {
            Ok(tokens) => match self.bound(&tokens.access_token, tokens.expires_in) {
                Ok(base) => {
                    let mut state = entry.state();
                    state.base = base;
                    state.can_elevate = tokens.can_elevate;
                    // RFC 6749 §6: the issuer may keep the refresh token; Zeabur Identity rotates
                    // it.
                    if let Some(next) = tokens.refresh_token {
                        state.refresh = Some(next);
                    }
                    drop(state);
                    Lookup::Active(Self::credentials(&entry, self.now()))
                }
                Err(error) => {
                    tracing::warn!(event = "session.renew", %error, "cannot use the renewed token");
                    self.end(&entry);
                    Lookup::SignedOut
                }
            },
            Err(error @ AuthError::Refused { .. }) => {
                tracing::info!(event = "session.renew", %error, "the issuer ended the session");
                self.end(&entry);
                Lookup::SignedOut
            }
            Err(error @ (AuthError::Unreachable(_) | AuthError::Invalid(_))) => {
                tracing::warn!(event = "session.renew", %error, "cannot renew the token");
                if still_valid {
                    Lookup::Active(Self::credentials(&entry, now))
                } else {
                    Lookup::IssuerUnavailable
                }
            }
        }
    }

    /// Adds administrator rights from `tokens` to the session `id`.
    ///
    /// # Errors
    ///
    /// When the session is over or the token cannot be bound.
    pub fn elevate(&self, id: &str, tokens: &TokenSet) -> Result<i64, SessionError> {
        let entry = self.find(id).ok_or(SessionError::SignedOut)?;
        let elevated = self.bound(&tokens.access_token, tokens.expires_in)?;
        let until = elevated.expires_at;
        entry.state().elevated = Some(elevated);
        Ok(until)
    }

    /// Drops the administrator rights of the session `id`, if it has any.
    pub fn drop_elevation(&self, id: &str) {
        if let Some(entry) = self.find(id) {
            entry.state().elevated = None;
        }
    }

    /// Ends the session `id`; returns its refresh token, to revoke.
    pub fn end_session(&self, id: &str) -> Option<SecretString> {
        let entry = self.find(id)?;
        self.end(&entry);
        entry.state().refresh.take()
    }

    /// Removes every session that is over; returns how many remain.
    pub fn sweep(&self) -> usize {
        let now = self.now();
        let mut entries = self.entries();
        entries.retain(|_, entry| !self.over(entry, now));
        entries.len()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicI64, Ordering};

    use secrecy::ExposeSecret as _;

    use super::*;
    use crate::client::{ClientConfig, ExtraRoots};
    use crate::flow::{Pending, Purpose};
    use crate::testing::{CLIENT_ID, CLIENT_SECRET, FakeIssuer};
    use crate::url::BaseUrl;

    const START: i64 = 1_800_000_000;

    struct Rig {
        issuer: FakeIssuer,
        oauth: Arc<OAuthClient>,
        sessions: Arc<Sessions<String>>,
        now: Arc<AtomicI64>,
    }

    impl Rig {
        async fn new(limits: Limits) -> Self {
            let issuer = FakeIssuer::start().await;
            let oauth = Arc::new(
                OAuthClient::new(ClientConfig {
                    issuer: issuer.url(),
                    public_url: BaseUrl::parse_root("http://127.0.0.1:8080").unwrap(),
                    client_id: CLIENT_ID.to_owned(),
                    client_secret: SecretString::from(CLIENT_SECRET.to_owned()),
                    issuer_ca: ExtraRoots::default(),
                })
                .unwrap(),
            );
            let now = Arc::new(AtomicI64::new(START));
            let clock = Arc::clone(&now);
            let sessions = Arc::new(Sessions::new(
                Arc::clone(&oauth),
                Arc::new(|token: &SecretString| Ok(token.expose_secret().to_owned())),
                limits,
                Arc::new(move || clock.load(Ordering::SeqCst)),
            ));
            Self {
                issuer,
                oauth,
                sessions,
                now,
            }
        }

        fn advance(&self, seconds: i64) {
            self.now.fetch_add(seconds, Ordering::SeqCst);
        }

        /// `user` signs in with a passkey; tokens from the issuer.
        async fn tokens(&self, user: &str, purpose: Purpose) -> TokenSet {
            let pending = Pending::start(purpose, START).unwrap();
            let url = self.oauth.authorization_url(&pending).await.unwrap();
            let answer = self.issuer.approve(&url, user);
            let code = form_urlencoded::parse(answer.as_bytes())
                .find(|(name, _)| name == "code")
                .map(|(_, value)| value.into_owned())
                .unwrap();
            self.oauth
                .exchange(&code, pending.verifier())
                .await
                .unwrap()
        }

        async fn sign_in(&self, user: &str) -> String {
            let tokens = self
                .tokens(
                    user,
                    Purpose::SignIn {
                        return_to: "/".to_owned(),
                    },
                )
                .await;
            self.sessions.start(user.to_owned(), tokens).unwrap()
        }

        async fn base(&self, id: &str) -> Option<String> {
            match self.sessions.current(id).await {
                Lookup::Active(credentials) => Some(credentials.base.value),
                Lookup::SignedOut | Lookup::IssuerUnavailable => None,
            }
        }
    }

    #[tokio::test]
    async fn a_session_holds_the_token_and_only_its_identifier_opens_it() {
        let rig = Rig::new(Limits::default()).await;
        let id = rig.sign_in("alice").await;
        assert_eq!(id.len(), 64);
        let Lookup::Active(credentials) = rig.sessions.current(&id).await else {
            panic!("signed out")
        };
        assert_eq!(credentials.username, "alice");
        assert!(credentials.base.value.starts_with("alice:"));
        assert_eq!(credentials.base.expires_at, START + 600);
        assert!(credentials.elevated.is_none());

        let mut guess = id.clone().into_bytes();
        guess[0] = if guess[0] == b'a' { b'b' } else { b'a' };
        let guess = String::from_utf8(guess).unwrap();
        assert!(matches!(
            rig.sessions.current(&guess).await,
            Lookup::SignedOut
        ));
        assert!(matches!(rig.sessions.current("").await, Lookup::SignedOut));
        // The store keeps no identifier, only its hash.
        assert!(!format!("{:?}", rig.sessions).contains(&id));
    }

    #[tokio::test]
    async fn the_token_is_renewed_shortly_before_it_expires() {
        let rig = Rig::new(Limits::default()).await;
        let id = rig.sign_in("alice").await;
        let first = rig.base(&id).await.unwrap();
        rig.advance(600 - 61);
        assert_eq!(rig.base(&id).await.unwrap(), first);
        assert_eq!(rig.issuer.with(|r| r.refreshes), 0);
        rig.advance(2);
        let renewed = rig.base(&id).await.unwrap();
        assert_ne!(renewed, first);
        assert!(renewed.starts_with("alice:"));
        assert_eq!(rig.issuer.with(|r| r.refreshes), 1);
        // The rotated refresh token is the one used next.
        rig.advance(600);
        assert!(rig.base(&id).await.is_some());
        assert_eq!(rig.issuer.with(|r| r.refreshes), 2);
    }

    /// Zeabur Identity revokes a refresh-token family when a rotated token is used again, so
    /// concurrent requests that all find the token about to expire must renew it once between
    /// them.
    #[tokio::test]
    async fn sixty_four_concurrent_requests_renew_the_token_once() {
        let rig = Rig::new(Limits::default()).await;
        let id = rig.sign_in("alice").await;
        rig.advance(590);
        let barrier = Arc::new(tokio::sync::Barrier::new(64));
        let tasks: Vec<_> = (0..64)
            .map(|_| {
                let sessions = Arc::clone(&rig.sessions);
                let barrier = Arc::clone(&barrier);
                let id = id.clone();
                tokio::spawn(async move {
                    barrier.wait().await;
                    match sessions.current(&id).await {
                        Lookup::Active(credentials) => Some(credentials.base.value),
                        Lookup::SignedOut | Lookup::IssuerUnavailable => None,
                    }
                })
            })
            .collect();
        let mut tokens = Vec::new();
        for task in tasks {
            tokens.push(task.await.unwrap().expect("still signed in"));
        }
        assert_eq!(rig.issuer.with(|r| r.refreshes), 1);
        tokens.dedup();
        assert_eq!(tokens.len(), 1);
    }

    #[tokio::test]
    async fn a_refused_renewal_ends_the_session() {
        let rig = Rig::new(Limits::default()).await;
        // A session whose refresh token the issuer has revoked since (an administrator removed
        // the person's grant, for example).
        let id = rig.sign_in("alice").await;
        let refresh = rig.sessions.end_session(&id).unwrap();
        rig.oauth.revoke(&refresh).await.unwrap();
        let tokens = TokenSet {
            access_token: SecretString::from("alice:1".to_owned()),
            expires_in: 600,
            refresh_token: Some(refresh),
            can_elevate: false,
        };
        let id = rig.sessions.start("alice".to_owned(), tokens).unwrap();
        assert!(rig.base(&id).await.is_some());
        rig.advance(590);
        assert!(matches!(rig.sessions.current(&id).await, Lookup::SignedOut));
        // And it stays over.
        assert!(matches!(rig.sessions.current(&id).await, Lookup::SignedOut));
        assert_eq!(rig.issuer.with(|r| r.refreshes), 0);
    }

    #[tokio::test]
    async fn an_unreachable_issuer_keeps_the_session_while_its_token_lasts() {
        let rig = Rig::new(Limits::default()).await;
        let id = rig.sign_in("alice").await;
        rig.issuer.with(|r| r.down = true);
        rig.advance(590);
        assert!(rig.base(&id).await.is_some());
        rig.advance(20);
        assert!(matches!(
            rig.sessions.current(&id).await,
            Lookup::IssuerUnavailable
        ));
        rig.issuer.with(|r| r.down = false);
        assert!(rig.base(&id).await.is_some());
    }

    #[tokio::test]
    async fn idle_and_absolute_limits_end_sessions() {
        let limits = Limits {
            idle: 100,
            absolute: 250,
            refresh_margin: 60,
        };
        let rig = Rig::new(limits).await;
        let idle = rig.sign_in("alice").await;
        let busy = rig.sign_in("bob").await;
        for _ in 0..2 {
            rig.advance(90);
            assert!(rig.base(&busy).await.is_some());
        }
        assert!(matches!(
            rig.sessions.current(&idle).await,
            Lookup::SignedOut
        ));
        rig.advance(90);
        // Used every 90 seconds, but older than 250.
        assert!(matches!(
            rig.sessions.current(&busy).await,
            Lookup::SignedOut
        ));
    }

    #[tokio::test]
    async fn expired_sessions_are_swept() {
        let rig = Rig::new(Limits::default()).await;
        let _first = rig.sign_in("alice").await;
        rig.advance(20 * 60);
        let second = rig.sign_in("bob").await;
        assert_eq!(rig.sessions.sweep(), 2);
        rig.advance(15 * 60);
        assert_eq!(rig.sessions.sweep(), 1);
        assert!(rig.base(&second).await.is_some());
    }

    #[tokio::test]
    async fn the_id_token_says_who_can_get_administrator_rights_at_each_renewal() {
        let rig = Rig::new(Limits::default()).await;
        rig.issuer
            .with(|r| r.cannot_elevate.insert("carol".to_owned()));
        let alice = rig.sign_in("alice").await;
        let carol = rig.sign_in("carol").await;
        let can_elevate = |id: String| {
            let sessions = Arc::clone(&rig.sessions);
            async move {
                match sessions.current(&id).await {
                    Lookup::Active(credentials) => credentials.can_elevate,
                    Lookup::SignedOut | Lookup::IssuerUnavailable => panic!("signed out"),
                }
            }
        };
        assert!(can_elevate(alice.clone()).await);
        assert!(!can_elevate(carol.clone()).await);

        rig.issuer.with(|r| {
            r.cannot_elevate.remove("carol");
            r.cannot_elevate.insert("alice".to_owned())
        });
        rig.advance(590);
        assert!(!can_elevate(alice).await);
        assert!(can_elevate(carol).await);
    }

    #[tokio::test]
    async fn administrator_rights_last_until_their_token_expires_or_are_dropped() {
        let rig = Rig::new(Limits::default()).await;
        let id = rig.sign_in("alice").await;
        let elevated = rig.tokens("alice", Purpose::Elevate).await;
        let until = rig.sessions.elevate(&id, &elevated).unwrap();
        assert_eq!(until, START + 300);
        let Lookup::Active(credentials) = rig.sessions.current(&id).await else {
            panic!("signed out")
        };
        let admin = credentials.elevated.unwrap();
        assert!(admin.value.starts_with("alice:elevated:"));
        // The everyday credential is unchanged.
        assert!(!credentials.base.value.contains("elevated"));

        rig.advance(300);
        let Lookup::Active(credentials) = rig.sessions.current(&id).await else {
            panic!("signed out")
        };
        assert!(credentials.elevated.is_none());

        let elevated = rig.tokens("alice", Purpose::Elevate).await;
        rig.sessions.elevate(&id, &elevated).unwrap();
        rig.sessions.drop_elevation(&id);
        let Lookup::Active(credentials) = rig.sessions.current(&id).await else {
            panic!("signed out")
        };
        assert!(credentials.elevated.is_none());
        assert_eq!(
            rig.sessions.elevate("no-such-session", &elevated),
            Err(SessionError::SignedOut)
        );
    }

    #[tokio::test]
    async fn signing_out_ends_the_session_and_hands_back_the_refresh_token() {
        let rig = Rig::new(Limits::default()).await;
        let id = rig.sign_in("alice").await;
        let refresh = rig.sessions.end_session(&id).unwrap();
        assert!(refresh.expose_secret().starts_with("refresh-"));
        assert!(matches!(rig.sessions.current(&id).await, Lookup::SignedOut));
        assert!(rig.sessions.end_session(&id).is_none());
    }
}
