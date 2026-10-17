//! The cookies: the session and the sign-ins in progress.
//!
//! Every cookie is `__Host-` prefixed, `Secure`, `HttpOnly`, `SameSite=Lax` and `Path=/`
//! [rfc6265bis]:
//!
//! - `__Host-` makes the browser accept the cookie only when it is `Secure`, has no `Domain` and
//!   has `Path=/` (§4.1.3.2), so another host of the same site cannot set or shadow it (the weak
//!   integrity of §8.6).
//! - `HttpOnly`: scripts cannot read it (§4.1.2.6).
//! - `SameSite=Lax`: other sites' subrequests do not carry it (§5.6.7.1). Not `Strict`: the issuer
//!   sends the browser back with a cross-site navigation, which must carry the sign-in cookie, and
//!   a link to Engawa from elsewhere should open it signed in. Cross-site changes are refused by
//!   [`engawa_core::guard`], not by the cookie.
//!
//! The session cookie has no `Max-Age`: it ends with the browser, and the server ends the session
//! on its own limits ([`crate::session::Limits`]).
//!
//! [rfc6265bis]: https://www.ietf.org/archive/id/draft-ietf-httpbis-rfc6265bis-22.txt

/// The session cookie.
pub const SESSION: &str = "__Host-engawa";
/// The cookie of a sign-in in progress.
pub const SIGN_IN: &str = "__Host-engawa-signin";
/// The cookie of an elevation in progress, separate so that it does not replace a sign-in.
pub const ELEVATE: &str = "__Host-engawa-elevate";

const ATTRIBUTES: &str = "Path=/; Secure; HttpOnly; SameSite=Lax";

/// A `Set-Cookie` value for `name`, lasting `max_age` seconds or, when `None`, the browser
/// session.
#[must_use]
pub fn set(name: &str, value: &str, max_age: Option<i64>) -> String {
    let age = max_age.map_or_else(String::new, |seconds| format!("Max-Age={seconds}; "));
    format!("{name}={value}; {age}{ATTRIBUTES}")
}

/// A `Set-Cookie` value that removes `name`.
#[must_use]
pub fn clear(name: &str) -> String {
    format!("{name}=; Max-Age=0; {ATTRIBUTES}")
}

/// The value of the first cookie called `name` in the request.
pub use engawa_core::guard::cookie as get;

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn cookies_are_host_only_secure_http_only_and_lax() {
        let session = set(SESSION, "abc", None);
        assert_eq!(
            session,
            "__Host-engawa=abc; Path=/; Secure; HttpOnly; SameSite=Lax"
        );
        assert!(!session.contains("Domain") && !session.contains("Max-Age"));
        assert_eq!(
            set(SIGN_IN, "x", Some(600)),
            "__Host-engawa-signin=x; Max-Age=600; Path=/; Secure; HttpOnly; SameSite=Lax"
        );
        assert!(clear(ELEVATE).starts_with("__Host-engawa-elevate=; Max-Age=0; Path=/; Secure"));
    }
}
