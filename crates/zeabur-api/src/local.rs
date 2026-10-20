//! Who may use the interface `zeaburctl ui` serves on this machine (local mode).
//!
//! The server acts with the person's kubeconfig, so anything that can talk to it can act as
//! them. Four checks keep it to the person who started it:
//!
//! 1. **Loopback only.** The listener binds `127.0.0.1` on a port the system picks, so other
//!    machines cannot connect at all. The link uses `127.0.0.1`, which Secure Contexts counts as
//!    potentially trustworthy without conditions, where `localhost` is only conditionally so
//!    [secure-contexts] (§3.1).
//! 2. **The Host header names this server.** A page from anywhere could point a DNS name at
//!    127.0.0.1 and make the browser send it requests (DNS rebinding); the defence is to "reject
//!    incoming HTTP requests with unexpected Host headers" [jackson2007] (§5.3), as local servers
//!    do: webpack-dev-server's `allowedHosts` [webpack], Jupyter's `local_hostnames`
//!    [jupyter-config] and `kubectl proxy --accept-hosts` [kubectl-proxy]. Zeabur is stricter than
//!    these: only `127.0.0.1:<port>` and `localhost:<port>`, with this server's port.
//! 3. **A one-time link, then a cookie.** The URL `zeaburctl` opens carries a random 256-bit token,
//!    exchanged for a session cookie and then spent. Jupyter turns a URL token into a cookie the same
//!    way, but its token stays valid [jupyter-security]; here a second use is refused. The cookie is
//!    `HttpOnly` (scripts cannot read it) and `SameSite=Strict` [rfc6265bis] (§5.6.7). It is not marked
//!    `Secure` and has no `__Host-` prefix: the draft allows both on potentially trustworthy origins,
//!    but whether browsers accept them on `http://127.0.0.1` is not verified, and nothing here
//!    depends on them.
//! 4. **Changes need this origin.** Cookies are not isolated by port [rfc6265bis] (§8.5), and
//!    `SameSite` treats every port of 127.0.0.1 as the same site, so a page served by another local
//!    server could otherwise make the browser send our cookie. Requests that change something pass
//!    the in-cluster server's check ([`zeabur_core::guard`]) for this server's two origins, port
//!    included: Fetch Metadata saying `same-origin` (or, without it, an `Origin` naming this
//!    server) and the header the web interface sends with every request.
//!
//! Another process of the same user on this machine is out of scope: it can read the kubeconfig
//! directly.
//!
//! Every comparison of a secret is constant-time.
//!
//! # References
//!
//! - [jackson2007] C. Jackson, A. Barth, A. Bortz, W. Shao and D. Boneh, "Protecting Browsers
//!   from DNS Rebinding Attacks," in *Proc. 14th ACM Conf. Computer and Communications Security
//!   (CCS '07)*, 2007. <https://crypto.stanford.edu/dns/dns-rebinding.pdf>
//! - [webpack] webpack contributors, "Dev server options: `devServer.allowedHosts`." <https://webpack.js.org/configuration/dev-server/>
//! - [jupyter-config] Jupyter Development Team, "Config file and command line options,"
//!   Jupyter Server documentation.
//!   <https://jupyter-server.readthedocs.io/en/latest/other/full-config.html>
//! - [jupyter-security] Jupyter Development Team, "Security in the Jupyter Server," Jupyter
//!   Server documentation. <https://jupyter-server.readthedocs.io/en/latest/operators/security.html>
//! - [kubectl-proxy] The Kubernetes Authors, "kubectl proxy," Kubernetes documentation. <https://kubernetes.io/docs/reference/kubectl/generated/kubectl_proxy/>
//! - [rfc6265bis] S. Bingler, M. West and J. Wilander, Eds., "Cookies: HTTP State Management
//!   Mechanism," IETF Internet-Draft draft-ietf-httpbis-rfc6265bis-22, Dec. 2025.
//!   <https://www.ietf.org/archive/id/draft-ietf-httpbis-rfc6265bis-22.txt>
//! - [secure-contexts] M. West, Ed., "Secure Contexts," W3C Candidate Recommendation Draft. <https://w3c.github.io/webappsec-secure-contexts/>
//!
//! [jackson2007]: https://crypto.stanford.edu/dns/dns-rebinding.pdf
//! [webpack]: https://webpack.js.org/configuration/dev-server/
//! [jupyter-config]: https://jupyter-server.readthedocs.io/en/latest/other/full-config.html
//! [jupyter-security]: https://jupyter-server.readthedocs.io/en/latest/operators/security.html
//! [kubectl-proxy]: https://kubernetes.io/docs/reference/kubectl/generated/kubectl_proxy/
//! [rfc6265bis]: https://www.ietf.org/archive/id/draft-ietf-httpbis-rfc6265bis-22.txt
//! [secure-contexts]: https://w3c.github.io/webappsec-secure-contexts/

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse as _, Response};
use zeabur_core::crypto::{RandomError, equal, random_token};
use zeabur_core::guard::{self, Origins, Refusal};
use zeabur_core::problem::{Kind, Problem};

/// The prefix of the session cookie's name; the port follows, because cookies are shared by every
/// port of a host and two `zeaburctl ui` runs (two kubeconfig contexts) must not replace each
/// other's session.
pub const COOKIE_PREFIX: &str = "zeabur-session-";
/// The query parameter that carries the one-time launch token.
pub const LAUNCH_PARAMETER: &str = "zeabur-launch";

/// The secrets of one `zeaburctl ui` run.
#[derive(Debug)]
pub struct Access {
    port: u16,
    /// `http://127.0.0.1:<port>` and `http://localhost:<port>`.
    origins: Origins,
    launch_token: String,
    launched: AtomicBool,
    session: String,
}

impl Access {
    /// Fresh secrets for a server listening on `port`.
    ///
    /// # Errors
    ///
    /// When the system random number generator fails.
    pub fn new(port: u16) -> Result<Arc<Self>, RandomError> {
        Ok(Arc::new(Self {
            port,
            origins: Origins::new([
                format!("http://127.0.0.1:{port}"),
                format!("http://localhost:{port}"),
            ]),
            launch_token: random_token::<32>()?,
            launched: AtomicBool::new(false),
            session: random_token::<32>()?,
        }))
    }

    /// The link to open in the browser.
    pub fn launch_url(&self) -> String {
        format!(
            "http://127.0.0.1:{}/?{LAUNCH_PARAMETER}={}",
            self.port, self.launch_token
        )
    }

    fn cookie_name(&self) -> String {
        format!("{COOKIE_PREFIX}{}", self.port)
    }

    fn has_session(&self, headers: &HeaderMap) -> bool {
        guard::cookie(headers, &self.cookie_name())
            .is_some_and(|value| equal(value.as_bytes(), self.session.as_bytes()))
    }

    /// Consumes the launch token if `presented` is it and it was not used before.
    fn redeem(&self, presented: &str) -> bool {
        equal(presented.as_bytes(), self.launch_token.as_bytes())
            && self
                .launched
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
    }
}

/// The middleware that enforces [`Access`] on every request; mount it with
/// `axum::middleware::from_fn_with_state`.
pub async fn guard(State(access): State<Arc<Access>>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    if !guard::host_allowed(&access.origins, headers) {
        return not_this_address();
    }
    if let Some(presented) = launch_token(request.uri().query()) {
        if request.method() == Method::GET && access.redeem(&presented) {
            return start_session(&access);
        }
        return problem(
            Kind::Forbidden,
            "This link was already used or is not valid. Run `zeaburctl ui` again to get a new one.",
        );
    }
    if !access.has_session(headers) {
        return problem(
            Kind::Unauthenticated,
            "Open the link `zeaburctl ui` printed to use this interface.",
        );
    }
    match guard::check(&access.origins, request.method(), headers) {
        Ok(()) => next.run(request).await,
        Err(Refusal::Host) => not_this_address(),
        Err(Refusal::CrossSite | Refusal::NotFromZeabur) => problem(
            Kind::Forbidden,
            "Changes are accepted only from the Zeabur page itself.",
        ),
    }
}

fn not_this_address() -> Response {
    problem(
        Kind::Forbidden,
        "This address does not belong to the Zeabur running on this machine.",
    )
}

/// The launch token in `query`, percent-decoded as a form value
/// (`application/x-www-form-urlencoded`, as `URLSearchParams` writes it).
fn launch_token(query: Option<&str>) -> Option<String> {
    form_urlencoded::parse(query?.as_bytes())
        .find(|(name, _)| name == LAUNCH_PARAMETER)
        .map(|(_, value)| value.into_owned())
}

/// Sets the session cookie and sends the browser to the page without the token in its address.
fn start_session(access: &Access) -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::SEE_OTHER;
    let headers = response.headers_mut();
    headers.insert(header::LOCATION, HeaderValue::from_static("/"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    if let Ok(cookie) = HeaderValue::from_str(&format!(
        "{}={}; Path=/; HttpOnly; SameSite=Strict",
        access.cookie_name(),
        access.session
    )) {
        headers.insert(header::SET_COOKIE, cookie);
    }
    response
}

fn problem(kind: Kind, detail: &str) -> Response {
    crate::error::ApiError::from(Problem::new(kind, detail)).into_response()
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::routing::{get, post};
    use tower::ServiceExt as _;

    use super::*;

    const PORT: u16 = 41_234;

    fn app(access: &Arc<Access>) -> Router {
        Router::new()
            .route("/", get(|| async { "page" }))
            .route("/api/change", post(|| async { "changed" }))
            .layer(axum::middleware::from_fn_with_state(
                Arc::clone(access),
                guard,
            ))
    }

    async fn send(app: Router, method: Method, uri: &str, headers: &[(&str, &str)]) -> Response {
        let mut request = Request::builder().method(method).uri(uri);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        app.oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    fn token_of(access: &Access) -> String {
        access.launch_token.clone()
    }

    fn session_cookie(response: &Response) -> String {
        let set = response.headers()[header::SET_COOKIE].to_str().unwrap();
        set.split(';').next().unwrap().to_owned()
    }

    const HOST: (&str, &str) = ("host", "127.0.0.1:41234");

    #[tokio::test]
    async fn the_launch_link_works_once_and_becomes_a_cookie() {
        let access = Access::new(PORT).unwrap();
        let uri = format!("/?{LAUNCH_PARAMETER}={}", token_of(&access));

        let first = send(app(&access), Method::GET, &uri, &[HOST]).await;
        assert_eq!(first.status(), StatusCode::SEE_OTHER);
        assert_eq!(first.headers()[header::LOCATION], "/");
        let set = first.headers()[header::SET_COOKIE].to_str().unwrap();
        assert!(set.contains("HttpOnly") && set.contains("SameSite=Strict"));

        let again = send(app(&access), Method::GET, &uri, &[HOST]).await;
        assert_eq!(again.status(), StatusCode::FORBIDDEN);

        let cookie = session_cookie(&first);
        let page = send(app(&access), Method::GET, "/", &[HOST, ("cookie", &cookie)]).await;
        assert_eq!(page.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn a_wrong_token_is_refused_and_does_not_use_up_the_real_one() {
        let access = Access::new(PORT).unwrap();
        let wrong = format!("/?{LAUNCH_PARAMETER}={}", "0".repeat(64));
        let response = send(app(&access), Method::GET, &wrong, &[HOST]).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let right = format!("/?{LAUNCH_PARAMETER}={}", token_of(&access));
        let response = send(app(&access), Method::GET, &right, &[HOST]).await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
    }

    #[tokio::test]
    async fn without_the_cookie_nothing_is_served() {
        let access = Access::new(PORT).unwrap();
        let response = send(app(&access), Method::GET, "/", &[HOST]).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let response = send(
            app(&access),
            Method::GET,
            "/",
            &[HOST, ("cookie", "zeabur-session-41234=guess")],
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn a_rebound_host_name_is_refused_even_with_the_cookie() {
        let access = Access::new(PORT).unwrap();
        let uri = format!("/?{LAUNCH_PARAMETER}={}", token_of(&access));
        let launched = send(app(&access), Method::GET, &uri, &[HOST]).await;
        let cookie = session_cookie(&launched);
        for host in [
            "attacker.example:41234",
            "127.0.0.1:1",
            "127.0.0.1",
            "[::1]:41234",
        ] {
            let response = send(
                app(&access),
                Method::GET,
                "/",
                &[("host", host), ("cookie", &cookie)],
            )
            .await;
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{host}");
        }
        let missing = send(app(&access), Method::GET, "/", &[("cookie", &cookie)]).await;
        assert_eq!(missing.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn changes_need_this_origin() {
        let access = Access::new(PORT).unwrap();
        let uri = format!("/?{LAUNCH_PARAMETER}={}", token_of(&access));
        let cookie = session_cookie(&send(app(&access), Method::GET, &uri, &[HOST]).await);
        let cookie = ("cookie", cookie.as_str());

        let none = send(app(&access), Method::POST, "/api/change", &[HOST, cookie]).await;
        assert_eq!(none.status(), StatusCode::FORBIDDEN);
        let other = send(
            app(&access),
            Method::POST,
            "/api/change",
            &[HOST, cookie, ("origin", "https://attacker.example")],
        )
        .await;
        assert_eq!(other.status(), StatusCode::FORBIDDEN);
        // Without Fetch Metadata, this Origin and the interface's header.
        let zeabur = (guard::REQUEST_HEADER, "1");
        let same = send(
            app(&access),
            Method::POST,
            "/api/change",
            &[HOST, cookie, ("origin", "http://127.0.0.1:41234"), zeabur],
        )
        .await;
        assert_eq!(same.status(), StatusCode::OK);
        let without_header = send(
            app(&access),
            Method::POST,
            "/api/change",
            &[HOST, cookie, ("origin", "http://127.0.0.1:41234")],
        )
        .await;
        assert_eq!(without_header.status(), StatusCode::FORBIDDEN);
        // Fetch Metadata decides when the browser sends it: another local server is the same
        // site, not the same origin, whatever its Origin says.
        let fetched = send(
            app(&access),
            Method::POST,
            "/api/change",
            &[HOST, cookie, ("sec-fetch-site", "same-origin"), zeabur],
        )
        .await;
        assert_eq!(fetched.status(), StatusCode::OK);
        let sibling = send(
            app(&access),
            Method::POST,
            "/api/change",
            &[
                HOST,
                cookie,
                ("sec-fetch-site", "same-site"),
                ("origin", "http://127.0.0.1:41234"),
                zeabur,
            ],
        )
        .await;
        assert_eq!(sibling.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn the_launch_token_is_read_as_a_form_value() {
        assert_eq!(
            launch_token(Some("a=1&zeabur-launch=ab%2Bc&b=2")).as_deref(),
            Some("ab+c")
        );
        assert_eq!(
            launch_token(Some("zeabur%2Dlaunch=abc")).as_deref(),
            Some("abc")
        );
        assert_eq!(launch_token(Some("zeabur-launchx=abc")), None);
        assert_eq!(launch_token(None), None);
    }

    #[tokio::test]
    async fn another_runs_cookie_does_not_open_this_one() {
        let first = Access::new(PORT).unwrap();
        let other = Access::new(41_235).unwrap();
        let uri = format!("/?{LAUNCH_PARAMETER}={}", token_of(&other));
        let launched = send(
            app(&other),
            Method::GET,
            &uri,
            &[("host", "127.0.0.1:41235")],
        )
        .await;
        let cookie = session_cookie(&launched);
        assert!(cookie.starts_with("zeabur-session-41235="));
        let response = send(app(&first), Method::GET, "/", &[HOST, ("cookie", &cookie)]).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn each_run_has_its_own_secrets() {
        let a = Access::new(PORT).unwrap();
        let b = Access::new(PORT).unwrap();
        assert_ne!(a.launch_token, b.launch_token);
        assert_ne!(a.session, b.session);
        assert_ne!(a.launch_token, a.session);
        assert!(
            a.launch_url()
                .starts_with("http://127.0.0.1:41234/?zeabur-launch=")
        );
    }
}
