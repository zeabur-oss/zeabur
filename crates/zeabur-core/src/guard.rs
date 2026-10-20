//! Which requests a server of Zeabur answers, before it looks at a session.
//!
//! The in-cluster server (whose public URL is configured) and `zeaburctl ui` (which answers on
//! `127.0.0.1` and `localhost` at its port) apply the same rules, each with its own [`Origins`].
//!
//! A session cookie travels with requests another site makes the browser send; what stops that
//! site from changing something through the person's browser (cross-site request forgery) is
//! this check, on every request whose method is not safe ([rfc9110] §9.2.1):
//!
//! 1. **Fetch Metadata.** `Sec-Fetch-Site`, which browsers set and pages cannot, must be
//!    `same-origin`; OWASP calls it the primary signal [owasp-csrf] ("Fetch Metadata headers").
//!    `same-site` is refused too: a sibling host of the same site is not Zeabur, and every port of
//!    `127.0.0.1` is the same site.
//! 2. **`Origin`, when Fetch Metadata is absent** (older browsers), must name one of the server's
//!    origins exactly; OWASP makes this fallback mandatory, and a missing or `null` `Origin` is not
//!    evidence of a same-origin request ("Using Standard Headers to Verify Origin").
//! 3. **A custom header** ([`REQUEST_HEADER`]) the web interface sends with every API request.
//!    Another origin cannot add it without a CORS preflight, which Zeabur never grants ("Employing
//!    Custom Request Headers for AJAX/API"). This also covers a browser that sends neither header
//!    above.
//!
//! **`Host` must name Zeabur** on every request, safe ones included: a page that points a DNS
//! name of its own at Zeabur (DNS rebinding) reaches it with another `Host`, and is refused
//! [jackson2007] (§5.3). Health checks from the kubelet carry the pod's address as `Host`, so
//! the in-cluster server mounts `/api/healthz` outside this guard.
//!
//! **A WebSocket handshake** is a `GET`, which a page of any origin can open with the person's
//! cookies, and the same-origin policy does not cover a WebSocket: the server must check
//! `Origin` itself [rfc6455] (§10.2). [`websocket_allowed`] wants `Origin`, which browsers always
//! send on a handshake (§4.1), to be one of the server's, and Fetch Metadata, when sent, to say
//! `same-origin`. A handshake cannot carry a custom header, so that check does not apply.
//!
//! # References
//!
//! - [owasp-csrf] OWASP Foundation, "Cross-Site Request Forgery Prevention Cheat Sheet," OWASP
//!   Cheat Sheet Series. <https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html>
//! - [fetch-metadata] M. West, "Fetch Metadata Request Headers," W3C Working Draft. <https://www.w3.org/TR/fetch-metadata/>
//! - [rfc9110] R. Fielding, M. Nottingham and J. Reschke, Eds., "HTTP Semantics," RFC 9110,
//!   IETF, June 2022. <https://www.rfc-editor.org/rfc/rfc9110>
//! - [rfc6455] I. Fette and A. Melnikov, "The WebSocket Protocol," RFC 6455, IETF, Dec. 2011. <https://www.rfc-editor.org/rfc/rfc6455>
//! - [jackson2007] C. Jackson, A. Barth, A. Bortz, W. Shao and D. Boneh, "Protecting Browsers
//!   from DNS Rebinding Attacks," in *Proc. 14th ACM Conf. Computer and Communications Security
//!   (CCS '07)*, 2007. <https://crypto.stanford.edu/dns/dns-rebinding.pdf>
//!
//! [owasp-csrf]: https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html
//! [fetch-metadata]: https://www.w3.org/TR/fetch-metadata/
//! [rfc9110]: https://www.rfc-editor.org/rfc/rfc9110
//! [rfc6455]: https://www.rfc-editor.org/rfc/rfc6455
//! [jackson2007]: https://crypto.stanford.edu/dns/dns-rebinding.pdf

use http::{HeaderMap, Method, header};

/// The header the web interface sends with every API request; its value is `1`.
pub const REQUEST_HEADER: &str = "zeabur-request";

const SEC_FETCH_SITE: &str = "sec-fetch-site";

/// Why a request was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// `Host` does not name this server.
    Host,
    /// A change from another site.
    CrossSite,
    /// A change without [`REQUEST_HEADER`].
    NotFromZeabur,
}

/// Whether `method` is safe ([rfc9110] §9.2.1): it changes nothing, so it needs no check
/// against other sites.
///
/// [rfc9110]: https://www.rfc-editor.org/rfc/rfc9110#section-9.2.1
#[must_use]
pub const fn is_safe(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

/// The origins a server answers as, each `scheme://host[:port]` as `Origin` writes it
/// ([rfc6454] §6.1); compared without regard to case, as scheme and host are.
///
/// [rfc6454]: https://www.rfc-editor.org/rfc/rfc6454#section-6.1
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origins(Vec<String>);

impl Origins {
    /// The server answering as each of `origins`.
    #[must_use]
    pub fn new(origins: impl IntoIterator<Item = String>) -> Self {
        Self(origins.into_iter().collect())
    }

    fn contains(&self, origin: &str) -> bool {
        self.0
            .iter()
            .any(|known| known.eq_ignore_ascii_case(origin))
    }

    fn has_authority(&self, host: &str) -> bool {
        self.0.iter().any(|origin| {
            origin
                .split_once("://")
                .is_some_and(|(_, authority)| authority.eq_ignore_ascii_case(host))
        })
    }
}

fn text<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

/// Whether the request's `Host` names one of `origins`.
#[must_use]
pub fn host_allowed(origins: &Origins, headers: &HeaderMap) -> bool {
    text(headers, header::HOST.as_str()).is_some_and(|host| origins.has_authority(host))
}

/// Whether the browser says the request comes from the server's own pages: `Sec-Fetch-Site:
/// same-origin`, or, without that header, an `Origin` that is one of `origins`.
#[must_use]
pub fn same_origin(origins: &Origins, headers: &HeaderMap) -> bool {
    text(headers, SEC_FETCH_SITE).map_or_else(
        || text(headers, header::ORIGIN.as_str()).is_some_and(|origin| origins.contains(origin)),
        |site| site == "same-origin",
    )
}

/// Whether a WebSocket handshake comes from the server's own pages: an `Origin` that is one of
/// `origins` and, when the browser sends Fetch Metadata, `Sec-Fetch-Site: same-origin`.
#[must_use]
pub fn websocket_allowed(origins: &Origins, headers: &HeaderMap) -> bool {
    text(headers, SEC_FETCH_SITE).is_none_or(|site| site == "same-origin")
        && text(headers, header::ORIGIN.as_str()).is_some_and(|origin| origins.contains(origin))
}

/// Checks a request to the server answering as `origins`.
///
/// # Errors
///
/// The first check the request fails.
pub fn check(origins: &Origins, method: &Method, headers: &HeaderMap) -> Result<(), Refusal> {
    if !host_allowed(origins, headers) {
        return Err(Refusal::Host);
    }
    if is_safe(method) {
        return Ok(());
    }
    if !same_origin(origins, headers) {
        return Err(Refusal::CrossSite);
    }
    if headers.get(REQUEST_HEADER).is_none_or(|v| v != "1") {
        return Err(Refusal::NotFromZeabur);
    }
    Ok(())
}

/// The value of the first cookie called `name` in the request.
#[must_use]
pub fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value)
}

#[cfg(test)]
mod tests {
    use http::HeaderValue;

    use super::*;

    fn public() -> Origins {
        Origins::new(["https://zeabur.example.com".to_owned()])
    }

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(*name, HeaderValue::from_static(value));
        }
        map
    }

    const HOST: (&str, &str) = ("host", "zeabur.example.com");
    const ZEABUR: (&str, &str) = (REQUEST_HEADER, "1");

    /// The outcome of a `POST` for every combination that matters.
    #[test]
    fn the_cross_site_matrix() {
        type Case<'a> = (&'a [(&'a str, &'a str)], Result<(), Refusal>);
        let cases: &[Case<'_>] = &[
            // The web interface in a current browser.
            (&[HOST, ("sec-fetch-site", "same-origin"), ZEABUR], Ok(())),
            // An older browser without Fetch Metadata, with Origin.
            (
                &[HOST, ("origin", "https://zeabur.example.com"), ZEABUR],
                Ok(()),
            ),
            // Another site's form or fetch.
            (
                &[HOST, ("sec-fetch-site", "cross-site"), ZEABUR],
                Err(Refusal::CrossSite),
            ),
            // A sibling host of the same site.
            (
                &[HOST, ("sec-fetch-site", "same-site"), ZEABUR],
                Err(Refusal::CrossSite),
            ),
            // Typed into the address bar or opened from a bookmark: not a change.
            (
                &[HOST, ("sec-fetch-site", "none"), ZEABUR],
                Err(Refusal::CrossSite),
            ),
            // Fetch Metadata wins over a matching Origin.
            (
                &[
                    HOST,
                    ("sec-fetch-site", "cross-site"),
                    ("origin", "https://zeabur.example.com"),
                    ZEABUR,
                ],
                Err(Refusal::CrossSite),
            ),
            // No Fetch Metadata: another Origin, `null`, or none at all.
            (
                &[HOST, ("origin", "https://attacker.example"), ZEABUR],
                Err(Refusal::CrossSite),
            ),
            (&[HOST, ("origin", "null"), ZEABUR], Err(Refusal::CrossSite)),
            (
                &[HOST, ("origin", "http://zeabur.example.com"), ZEABUR],
                Err(Refusal::CrossSite),
            ),
            (&[HOST, ZEABUR], Err(Refusal::CrossSite)),
            // Same origin, without the interface's header.
            (
                &[HOST, ("sec-fetch-site", "same-origin")],
                Err(Refusal::NotFromZeabur),
            ),
            (
                &[
                    HOST,
                    ("sec-fetch-site", "same-origin"),
                    (REQUEST_HEADER, "0"),
                ],
                Err(Refusal::NotFromZeabur),
            ),
            // Another Host, or none.
            (
                &[
                    ("host", "attacker.example"),
                    ("sec-fetch-site", "same-origin"),
                    ZEABUR,
                ],
                Err(Refusal::Host),
            ),
            (
                &[("sec-fetch-site", "same-origin"), ZEABUR],
                Err(Refusal::Host),
            ),
        ];
        for (pairs, expected) in cases {
            assert_eq!(
                check(&public(), &Method::POST, &headers(pairs)),
                *expected,
                "{pairs:?}"
            );
        }
        // Every unsafe method is checked.
        for method in [Method::PUT, Method::PATCH, Method::DELETE] {
            assert_eq!(
                check(
                    &public(),
                    &method,
                    &headers(&[HOST, ("sec-fetch-site", "cross-site")])
                ),
                Err(Refusal::CrossSite)
            );
        }
    }

    #[test]
    fn safe_requests_need_only_the_host() {
        for method in [Method::GET, Method::HEAD, Method::OPTIONS] {
            assert_eq!(
                check(
                    &public(),
                    &method,
                    &headers(&[HOST, ("sec-fetch-site", "cross-site")])
                ),
                Ok(())
            );
            assert_eq!(
                check(&public(), &method, &headers(&[("host", "10.42.0.7:8080")])),
                Err(Refusal::Host)
            );
        }
        assert_eq!(
            check(
                &public(),
                &Method::GET,
                &headers(&[("host", "ZEABUR.example.com")])
            ),
            Ok(())
        );
    }

    /// `zeaburctl ui` answers as two origins, at its own port only.
    #[test]
    fn a_local_server_answers_as_both_loopback_names_at_its_port() {
        let local = Origins::new([
            "http://127.0.0.1:41234".to_owned(),
            "http://localhost:41234".to_owned(),
        ]);
        for host in ["127.0.0.1:41234", "localhost:41234", "LOCALHOST:41234"] {
            assert_eq!(
                check(&local, &Method::GET, &headers(&[("host", host)])),
                Ok(()),
                "{host}"
            );
        }
        for host in [
            "127.0.0.1:1",
            "127.0.0.1",
            "[::1]:41234",
            "attacker.example:41234",
        ] {
            assert_eq!(
                check(&local, &Method::GET, &headers(&[("host", host)])),
                Err(Refusal::Host),
                "{host}"
            );
        }
        let host = ("host", "127.0.0.1:41234");
        assert_eq!(
            check(
                &local,
                &Method::POST,
                &headers(&[host, ("origin", "http://localhost:41234"), ZEABUR])
            ),
            Ok(())
        );
        // Another local server is the same site, not the same origin.
        assert_eq!(
            check(
                &local,
                &Method::POST,
                &headers(&[host, ("sec-fetch-site", "same-site"), ZEABUR])
            ),
            Err(Refusal::CrossSite)
        );
        assert_eq!(
            check(
                &local,
                &Method::POST,
                &headers(&[host, ("origin", "http://127.0.0.1:41235"), ZEABUR])
            ),
            Err(Refusal::CrossSite)
        );
    }

    #[test]
    fn a_websocket_handshake_needs_this_origin() {
        let cases: &[(&[(&str, &str)], bool)] = &[
            (&[("origin", "https://zeabur.example.com")], true),
            (
                &[
                    ("origin", "https://zeabur.example.com"),
                    ("sec-fetch-site", "same-origin"),
                ],
                true,
            ),
            // Browsers always send Origin on a handshake: none is not one of ours.
            (&[("sec-fetch-site", "same-origin")], false),
            (&[], false),
            (&[("origin", "https://attacker.example")], false),
            (&[("origin", "null")], false),
            (
                &[
                    ("origin", "https://zeabur.example.com"),
                    ("sec-fetch-site", "cross-site"),
                ],
                false,
            ),
        ];
        for (pairs, allowed) in cases {
            assert_eq!(
                websocket_allowed(&public(), &headers(pairs)),
                *allowed,
                "{pairs:?}"
            );
        }
    }

    #[test]
    fn a_cookie_is_found_by_its_exact_name() {
        let mut headers = HeaderMap::new();
        headers.append(
            header::COOKIE,
            HeaderValue::from_static("__Host-zeabur-signin=flow; other=1"),
        );
        headers.append(header::COOKIE, HeaderValue::from_static("__Host-zeabur=id"));
        assert_eq!(cookie(&headers, "__Host-zeabur"), Some("id"));
        assert_eq!(cookie(&headers, "__Host-zeabur-signin"), Some("flow"));
        assert_eq!(cookie(&headers, "__Host-zeabur-elevate"), None);
    }
}
