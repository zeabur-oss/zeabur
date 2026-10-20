//! The server end to end, with an in-process issuer and in-memory clusters: one per token, so
//! what a request does shows which credential it used.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::response::Response;
use http_body_util::BodyExt as _;
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::{Value, json};
use tower::ServiceExt as _;
use zeabur_auth::client::{ClientConfig, ExtraRoots, OAuthClient};
use zeabur_auth::flow::FlowKey;
use zeabur_auth::session::{Limits, Sessions};
use zeabur_auth::testing::{CLIENT_ID, CLIENT_SECRET, FakeIssuer};
use zeabur_auth::url::BaseUrl;
use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
use zeabur_kube::memory::{MemoryCluster, State};
use zeabur_kube::{Identity, NamespaceObjects};
use zeabur_plan::token::PlanKey;

use super::{Auth, Connection, app};

const PUBLIC: &str = "http://127.0.0.1:8080";
const HOST: (&str, &str) = ("host", "127.0.0.1:8080");
const SAME_ORIGIN: [(&str, &str); 2] = [("sec-fetch-site", "same-origin"), ("zeabur-request", "1")];
const NOW: i64 = 1_800_000_000;

/// A project whose Deployment `web` the everyday credential may not change, in a cluster whose
/// nodes it may not list.
fn cluster(user: &str, elevated: bool) -> MemoryCluster {
    let namespace: Namespace = serde_json::from_value(json!({"metadata": {"name": "shop",
        "labels": {"zeabur.com/project": "true"}}}))
    .unwrap();
    let deployment = serde_json::from_value(json!({
        "apiVersion": "apps/v1", "kind": "Deployment",
        "metadata": {"name": "web", "namespace": "shop", "resourceVersion": "41"},
        "spec": {"selector": {"matchLabels": {"app": "web"}}, "template": {
            "metadata": {"labels": {"app": "web"}},
            "spec": {"containers": [{"name": "app", "image": "example/app:1",
                "env": [{"name": "MODE", "value": "production"}]}]}}}
    }))
    .unwrap();
    let mut groups = vec!["developers".to_owned()];
    if elevated {
        groups.push("elevated:admins".to_owned());
    }
    let cluster = MemoryCluster::new(State {
        identity: Identity {
            username: user.to_owned(),
            groups,
        },
        namespaces: vec![namespace],
        objects: BTreeMap::from([(
            "shop".to_owned(),
            NamespaceObjects {
                namespace: "shop".to_owned(),
                deployments: vec![deployment],
                ..NamespaceObjects::default()
            },
        )]),
        ..State::default()
    });
    if !elevated {
        cluster.update(|s| {
            s.forbidden_writes.insert("Deployment/web".to_owned());
            s.forbidden_resources.insert("nodes".to_owned());
        });
    }
    cluster
}

struct Rig {
    issuer: FakeIssuer,
    app: Router,
    now: Arc<AtomicI64>,
}

impl Rig {
    async fn new(elevation: bool) -> Self {
        let issuer = FakeIssuer::start().await;
        let public = BaseUrl::parse_root(PUBLIC).unwrap();
        let oauth = Arc::new(
            OAuthClient::new(ClientConfig {
                issuer: issuer.url(),
                public_url: public.clone(),
                client_id: CLIENT_ID.to_owned(),
                client_secret: SecretString::from(CLIENT_SECRET.to_owned()),
                issuer_ca: ExtraRoots::default(),
            })
            .unwrap(),
        );
        let now = Arc::new(AtomicI64::new(NOW));
        let read = Arc::clone(&now);
        let clock: zeabur_auth::session::Clock = Arc::new(move || read.load(Ordering::SeqCst));
        // Tokens read `<user>:<n>` or `<user>:elevated:<n>`; kube-apiserver refuses `mallory`.
        let connect: zeabur_auth::session::Bind<Connection> = Arc::new(|token: &SecretString| {
            let token = token.expose_secret();
            let user = token.split(':').next().unwrap_or("");
            if user == "mallory" {
                return Err("Unauthorized".to_owned());
            }
            let cluster: Connection = Arc::new(cluster(user, token.contains(":elevated:")));
            Ok(cluster)
        });
        let sessions = Arc::new(Sessions::new(
            Arc::clone(&oauth),
            Arc::clone(&connect),
            Limits::default(),
            Arc::clone(&clock),
        ));
        let auth = Arc::new(Auth {
            public,
            oauth,
            sessions,
            connect,
            flow_key: FlowKey::random().unwrap(),
            elevation,
            clock,
        });
        Self {
            issuer,
            app: app(
                &auth,
                PlanKey::from_bytes([3; 32]),
                zeabur_api::Settings::default(),
            ),
            now,
        }
    }

    async fn send(&self, method: Method, uri: &str, headers: &[(&str, &str)]) -> Response {
        let mut request = Request::builder().method(method).uri(uri);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        self.app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn post_json(&self, uri: &str, cookie: &str, body: &Value) -> (StatusCode, Value) {
        let mut request = Request::post(uri)
            .header("host", HOST.1)
            .header("cookie", cookie)
            .header("content-type", "application/json");
        for (name, value) in SAME_ORIGIN {
            request = request.header(name, value);
        }
        let response = self
            .app
            .clone()
            .oneshot(request.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let status = response.status();
        (status, json_of(response).await)
    }

    /// `user` signs in, coming back to `return_to`; the session cookie.
    async fn sign_in(&self, user: &str) -> String {
        let login = self
            .send(Method::GET, "/auth/login?return=/projects/shop", &[HOST])
            .await;
        assert_eq!(login.status(), StatusCode::SEE_OTHER);
        let flow = cookie_pair(&login, "__Host-zeabur-signin");
        let answer = self.issuer.approve(location(&login), user);
        let back = self
            .send(
                Method::GET,
                &format!("/auth/callback?{answer}"),
                &[HOST, ("cookie", &flow)],
            )
            .await;
        assert_eq!(back.status(), StatusCode::SEE_OTHER, "{}", text(back).await);
        assert_eq!(location(&back), "/projects/shop");
        cookie_pair(&back, "__Host-zeabur")
    }

    /// `user` confirms admin rights in the window `/auth/elevate` opened; the page it ends on.
    async fn elevate(&self, session: &str, user: &str) -> String {
        let start = self
            .send(
                Method::GET,
                "/auth/elevate",
                &[HOST, ("cookie", session), ("sec-fetch-site", "same-origin")],
            )
            .await;
        assert_eq!(
            start.status(),
            StatusCode::SEE_OTHER,
            "{}",
            text(start).await
        );
        let flow = cookie_pair(&start, "__Host-zeabur-elevate");
        let answer = self.issuer.approve(location(&start), user);
        let cookies = format!("{session}; {flow}");
        let page = self
            .send(
                Method::GET,
                &format!("/auth/callback?{answer}"),
                &[HOST, ("cookie", &cookies)],
            )
            .await;
        assert_eq!(page.status(), StatusCode::OK);
        text(page).await
    }

    async fn plan_and_apply(&self, session: &str) -> (StatusCode, Value) {
        let change = json!({"changes": [{"op": "env",
            "workload": {"kind": "Deployment", "name": "web"},
            "changes": [{"op": "set", "name": "MODE", "value": "maintenance"}]}]});
        let (status, plan) = self
            .post_json("/api/v1/projects/shop/plan", session, &change)
            .await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        self.post_json(
            "/api/v1/projects/shop/apply",
            session,
            &json!({"token": plan["token"]}),
        )
        .await
    }

    async fn get_json(&self, uri: &str, session: &str) -> (StatusCode, Value) {
        let response = self
            .send(Method::GET, uri, &[HOST, ("cookie", session)])
            .await;
        let status = response.status();
        (status, json_of(response).await)
    }
}

fn location(response: &Response) -> &str {
    response.headers()[header::LOCATION].to_str().unwrap()
}

fn set_cookies(response: &Response) -> Vec<String> {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_owned())
        .collect()
}

/// `name=value` of the cookie `name` the response sets.
fn cookie_pair(response: &Response, name: &str) -> String {
    set_cookies(response)
        .into_iter()
        .find(|c| c.starts_with(&format!("{name}=")))
        .and_then(|c| c.split(';').next().map(str::to_owned))
        .unwrap_or_else(|| panic!("no {name} cookie in {:?}", set_cookies(response)))
}

async fn text(response: Response) -> String {
    String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap()
}

async fn json_of(response: Response) -> Value {
    serde_json::from_str(&text(response).await).unwrap_or(Value::Null)
}

fn query(url: &str) -> HashMap<String, String> {
    let query = url.split_once('?').map_or("", |(_, q)| q);
    form_urlencoded_pairs(query)
}

fn form_urlencoded_pairs(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

#[tokio::test]
async fn the_health_check_needs_no_host_or_session() {
    let rig = Rig::new(true).await;
    let response = rig
        .send(Method::GET, "/api/healthz", &[("host", "10.42.0.7:8080")])
        .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn without_a_session_the_api_answers_signed_out() {
    let rig = Rig::new(true).await;
    for cookie in ["", "__Host-zeabur=guess"] {
        let (status, problem) = rig.get_json("/api/v1/whoami", cookie).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(problem["type"], "/problems/signed-out");
    }
}

#[tokio::test]
async fn signing_in_sends_the_browser_to_the_issuer_with_a_sealed_flow_cookie() {
    let rig = Rig::new(true).await;
    let login = rig
        .send(
            Method::GET,
            "/auth/login?return=//attacker.example/",
            &[HOST],
        )
        .await;
    assert_eq!(login.status(), StatusCode::SEE_OTHER);
    let to = query(location(&login));
    assert!(location(&login).starts_with(&format!("{}/authorize?", rig.issuer.url())));
    assert_eq!(to["code_challenge_method"], "S256");
    assert_eq!(to["client_id"], "zeabur");
    let set = set_cookies(&login).join("\n");
    assert!(set.contains("__Host-zeabur-signin="));
    assert!(set.contains("Max-Age=600; Path=/; Secure; HttpOnly; SameSite=Lax"));
    assert_eq!(login.headers()[header::CACHE_CONTROL], "no-store");

    // The unsafe return path became `/`.
    let flow = cookie_pair(&login, "__Host-zeabur-signin");
    let answer = rig.issuer.approve(location(&login), "alice");
    let back = rig
        .send(
            Method::GET,
            &format!("/auth/callback?{answer}"),
            &[HOST, ("cookie", &flow)],
        )
        .await;
    assert_eq!(location(&back), "/");
    let set = set_cookies(&back).join("\n");
    assert!(set.contains("__Host-zeabur-signin=; Max-Age=0"));
    let session = set
        .lines()
        .find(|c| c.starts_with("__Host-zeabur="))
        .unwrap()
        .to_owned();
    assert!(
        session.ends_with("; Path=/; Secure; HttpOnly; SameSite=Lax"),
        "{session}"
    );
}

#[tokio::test]
async fn a_signed_in_person_uses_the_api_as_themselves() {
    let rig = Rig::new(true).await;
    let session = rig.sign_in("alice").await;
    let (status, whoami) = rig.get_json("/api/v1/whoami", &session).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(whoami["username"], "alice");
    let (_, view) = rig.get_json("/api/v1/session", &session).await;
    assert_eq!(view["signsIn"], true);
    assert_eq!(view["elevation"], "available");
    assert!(view["expiresAt"].as_str().unwrap().ends_with('Z'));
    assert!(view.get("elevatedUntil").is_none());
}

#[tokio::test]
async fn a_callback_that_does_not_match_a_sign_in_in_this_browser_is_refused() {
    let rig = Rig::new(true).await;
    let login = rig.send(Method::GET, "/auth/login", &[HOST]).await;
    let flow = cookie_pair(&login, "__Host-zeabur-signin");
    let answer = rig.issuer.approve(location(&login), "alice");
    let code = form_urlencoded_pairs(&answer)["code"].clone();
    let iss = rig.issuer.url().to_string();

    // Another state (a forged answer), no flow cookie (another browser), another issuer.
    for (uri, cookie) in [
        (
            format!("/auth/callback?code={code}&state=forged&iss={iss}"),
            flow.as_str(),
        ),
        (format!("/auth/callback?{answer}"), ""),
    ] {
        let response = rig
            .send(Method::GET, &uri, &[HOST, ("cookie", cookie)])
            .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{uri}");
        assert!(
            set_cookies(&response)
                .iter()
                .all(|c| !c.starts_with("__Host-zeabur="))
        );
    }
    let state = query(location(&login))["state"].clone();
    let mixed_up = rig
        .send(
            Method::GET,
            &format!("/auth/callback?code={code}&state={state}&iss=https%3A%2F%2Fattacker.example"),
            &[HOST, ("cookie", &flow)],
        )
        .await;
    assert_eq!(mixed_up.status(), StatusCode::BAD_REQUEST);
    assert!(
        set_cookies(&mixed_up)
            .iter()
            .all(|c| !c.starts_with("__Host-zeabur="))
    );
}

#[tokio::test]
async fn a_token_the_cluster_refuses_starts_no_session() {
    let rig = Rig::new(true).await;
    let login = rig.send(Method::GET, "/auth/login", &[HOST]).await;
    let flow = cookie_pair(&login, "__Host-zeabur-signin");
    let answer = rig.issuer.approve(location(&login), "mallory");
    let back = rig
        .send(
            Method::GET,
            &format!("/auth/callback?{answer}"),
            &[HOST, ("cookie", &flow)],
        )
        .await;
    assert_eq!(back.status(), StatusCode::FORBIDDEN);
    assert!(
        set_cookies(&back)
            .iter()
            .all(|c| !c.starts_with("__Host-zeabur="))
    );
}

#[tokio::test]
async fn changes_from_other_sites_and_hosts_are_refused() {
    let rig = Rig::new(true).await;
    let session = rig.sign_in("alice").await;
    let apply = "/api/v1/projects/shop/apply";
    for headers in [
        vec![
            HOST,
            ("cookie", &session),
            ("sec-fetch-site", "cross-site"),
            ("zeabur-request", "1"),
        ],
        vec![
            HOST,
            ("cookie", &session),
            ("sec-fetch-site", "same-origin"),
        ],
        vec![
            ("host", "attacker.example"),
            ("cookie", &session),
            SAME_ORIGIN[0],
            SAME_ORIGIN[1],
        ],
    ] {
        let response = rig.send(Method::POST, apply, &headers).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{headers:?}");
    }
    // A read from another Host too (DNS rebinding).
    let response = rig
        .send(
            Method::GET,
            "/api/v1/whoami",
            &[("host", "attacker.example"), ("cookie", &session)],
        )
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    // Signing out is a change.
    let response = rig
        .send(
            Method::POST,
            "/auth/logout",
            &[HOST, ("cookie", &session), ("sec-fetch-site", "cross-site")],
        )
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// The whole elevation: a refused write and a refused read offer it; the window ends with
/// `done`; the same plan applies and the read succeeds with the elevated token; dropping it
/// brings the offer back.
#[tokio::test]
async fn a_refused_request_is_retried_as_an_administrator() {
    let rig = Rig::new(true).await;
    let session = rig.sign_in("alice").await;
    let (status, problem) = rig.plan_and_apply(&session).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(problem["type"], "/problems/elevation-required");
    assert_eq!(problem["denied"]["verb"], "patch");
    let (status, problem) = rig.get_json("/api/v1/cluster/nodes", &session).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(problem["type"], "/problems/elevation-required");
    assert_eq!(problem["kubernetes"]["code"], 403);
    assert_eq!(problem["denied"]["resource"], "nodes");

    // Plan before elevating, apply after: the plan's user binding still holds.
    let change = json!({"changes": [{"op": "env",
        "workload": {"kind": "Deployment", "name": "web"},
        "changes": [{"op": "set", "name": "MODE", "value": "maintenance"}]}]});
    let (_, plan) = rig
        .post_json("/api/v1/projects/shop/plan", &session, &change)
        .await;

    let page = rig.elevate(&session, "alice").await;
    assert!(page.contains(r#"data-result="done""#), "{page}");
    assert!(
        page.contains(r#"data-until="2027-01-15T08:05:00Z""#),
        "{page}"
    );
    assert!(page.contains(r#"<script src="/auth/elevated.js"></script>"#));
    let (_, view) = rig.get_json("/api/v1/session", &session).await;
    assert_eq!(view["elevation"], "active");
    assert_eq!(view["elevatedUntil"], "2027-01-15T08:05:00Z");

    let (status, applied) = rig
        .post_json(
            "/api/v1/projects/shop/apply",
            &session,
            &json!({"token": plan["token"]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    // Reads go with the elevated token too, as with `sudo`.
    let (status, nodes) = rig.get_json("/api/v1/cluster/nodes", &session).await;
    assert_eq!(status, StatusCode::OK, "{nodes}");
    let (_, whoami) = rig.get_json("/api/v1/whoami", &session).await;
    assert_eq!(whoami["groups"], json!(["developers", "elevated:admins"]));
    // A plan made while elevated is bound to the everyday identity, so it applies as well.
    let (status, applied) = rig.plan_and_apply(&session).await;
    assert_eq!(status, StatusCode::OK, "{applied}");

    let mut headers = vec![HOST, ("cookie", session.as_str())];
    headers.extend(SAME_ORIGIN);
    let dropped = rig.send(Method::DELETE, "/auth/elevation", &headers).await;
    assert_eq!(dropped.status(), StatusCode::NO_CONTENT);
    let (_, view) = rig.get_json("/api/v1/session", &session).await;
    assert_eq!(view["elevation"], "available");
    let (_, problem) = rig.get_json("/api/v1/cluster/nodes", &session).await;
    assert_eq!(problem["type"], "/problems/elevation-required");

    // And it ends on its own after five minutes.
    rig.elevate(&session, "alice").await;
    rig.now.fetch_add(300, Ordering::SeqCst);
    let (_, view) = rig.get_json("/api/v1/session", &session).await;
    assert_eq!(view["elevation"], "available");
}

#[tokio::test]
async fn elevation_ends_plainly_when_not_allowed_or_for_someone_else() {
    let rig = Rig::new(true).await;
    let session = rig.sign_in("alice").await;
    // Someone else's passkey in the window lends nothing to this session.
    let page = rig.elevate(&session, "bob").await;
    assert!(page.contains(r#"data-result="failed""#), "{page}");
    let (_, view) = rig.get_json("/api/v1/session", &session).await;
    assert_eq!(view["elevation"], "available");

    rig.issuer.with(|r| r.elevation_allowed = false);
    let page = rig.elevate(&session, "alice").await;
    assert!(page.contains(r#"data-result="not-allowed""#), "{page}");

    // Only Zeabur's own page opens the window.
    let response = rig
        .send(
            Method::GET,
            "/auth/elevate",
            &[HOST, ("cookie", &session), ("sec-fetch-site", "cross-site")],
        )
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn without_elevation_a_refusal_stays_forbidden() {
    let rig = Rig::new(false).await;
    let session = rig.sign_in("alice").await;
    let (status, problem) = rig.plan_and_apply(&session).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(problem["type"], "/problems/forbidden");
    let (status, problem) = rig.get_json("/api/v1/cluster/nodes", &session).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(problem["type"], "/problems/forbidden");
    let (_, view) = rig.get_json("/api/v1/session", &session).await;
    assert_eq!(view["elevation"], "unavailable");
    let response = rig
        .send(
            Method::GET,
            "/auth/elevate",
            &[
                HOST,
                ("cookie", &session),
                ("sec-fetch-site", "same-origin"),
            ],
        )
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn signing_out_ends_the_session_and_revokes_its_refresh_token() {
    let rig = Rig::new(true).await;
    let session = rig.sign_in("alice").await;
    let mut headers = vec![HOST, ("cookie", session.as_str())];
    headers.extend(SAME_ORIGIN);
    let response = rig.send(Method::POST, "/auth/logout", &headers).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(
        set_cookies(&response)
            .iter()
            .any(|c| c.starts_with("__Host-zeabur=; Max-Age=0"))
    );
    assert_eq!(rig.issuer.with(|r| r.revocations.len()), 1);
    let (status, _) = rig.get_json("/api/v1/whoami", &session).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// The terminal's handshake must come from the public URL's origin: `Host` alone matching is not
/// enough (here the scheme differs). The refusal is Zeabur's own, so it stays `forbidden` although
/// elevation is offered: admin rights would not change it.
#[tokio::test]
async fn a_terminal_opens_only_from_the_public_origin() {
    let rig = Rig::new(true).await;
    let session = rig.sign_in("alice").await;
    let uri = "/api/v1/projects/shop/pods/web-1/exec";
    for (origin, refused) in [
        ("https://127.0.0.1:8080", true),
        ("http://attacker.example", true),
        (PUBLIC, false),
    ] {
        let response = rig
            .send(
                Method::GET,
                uri,
                &[HOST, ("cookie", session.as_str()), ("origin", origin)],
            )
            .await;
        // Past the origin check, this request (not a real upgrade) fails at the upgrade instead.
        assert_eq!(
            response.status() == StatusCode::FORBIDDEN,
            refused,
            "{origin}: {}",
            response.status()
        );
        if refused {
            assert_eq!(json_of(response).await["type"], "/problems/forbidden");
        }
    }
}

/// Which token a request goes with, and so what a terminal closes at: the everyday one, or while
/// the person is elevated the admin one, for reads as for writes, and its expiry. A plan's binding
/// alone keeps the everyday one.
#[test]
fn every_request_goes_with_the_token_in_use_and_ends_with_it() {
    use zeabur_api::access::{ClusterAccess as _, Elevation};
    use zeabur_auth::session::{Bound, Credentials};
    use zeabur_kube::k8s_openapi::jiff::Timestamp;

    use super::{SessionAccess, Signed};

    let base: Connection = Arc::new(cluster("alice", false));
    let elevated: Connection = Arc::new(cluster("alice", true));
    let access = SessionAccess {
        elevation: true,
        origin: PUBLIC.to_owned(),
    };
    let mut parts = Request::builder().body(()).unwrap().into_parts().0;
    assert_eq!(access.credential_expiry(&parts), None);

    let mut credentials = Credentials {
        username: "alice".to_owned(),
        base: Bound {
            value: Arc::clone(&base),
            expires_at: NOW + 600,
        },
        elevated: None,
    };
    parts.extensions.insert(Signed::In(credentials.clone()));
    assert_eq!(
        access.credential_expiry(&parts).map(Timestamp::as_second),
        Some(NOW + 600)
    );
    let now = access.for_request(&parts).unwrap();
    assert!(Arc::ptr_eq(&now.cluster, &base));
    assert!(matches!(now.elevation, Elevation::Available));

    credentials.elevated = Some(Bound {
        value: Arc::clone(&elevated),
        expires_at: NOW + 300,
    });
    parts.extensions.insert(Signed::In(credentials));
    let now = access.for_request(&parts).unwrap();
    assert!(Arc::ptr_eq(&now.cluster, &elevated));
    match now.elevation {
        Elevation::Active { until } => assert_eq!(until.as_second(), NOW + 300),
        Elevation::Available | Elevation::Unavailable => panic!("not elevated"),
    }
    assert_eq!(
        access.credential_expiry(&parts).map(Timestamp::as_second),
        Some(NOW + 300)
    );
    assert!(Arc::ptr_eq(&access.everyday(&parts).unwrap(), &base));
}
