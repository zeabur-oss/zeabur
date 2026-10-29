//! The cluster's GitHub App against a fake GitHub on 127.0.0.1: an administrator makes it from
//! the manifest flow and no answer shows its private key, the flow belongs to whoever started it,
//! a grant writes Builder's grants and what each project may build, and a person who may not
//! read `builder-system` is told which read was refused.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use aws_lc_rs::encoding::AsDer as _;
    use aws_lc_rs::rsa::{KeyPair, KeySize};
    use aws_lc_rs::signature::{KeyPair as _, RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey};
    use axum::extract::{Path, State as Shared};
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use base64::Engine as _;
    use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
    use serde_json::{Value, json};
    use tower::ServiceExt as _;
    use zeabur_api::Settings;
    use zeabur_api::settings::GitHubSettings;
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects, WriteBody};

    use crate::common::{app_with, send};

    const BASE: &str = "/api/v1/cluster/github-app";
    const CLIENT_ID: &str = "Iv23liexample";
    const WEBHOOK_SECRET: &str = "very-secret-webhook-secret";
    const INSTALLATION: i64 = 4242;

    struct Fake {
        pem: String,
        public: Vec<u8>,
        converted: AtomicBool,
    }

    fn signed_by_the_app(fake: &Fake, headers: &HeaderMap) -> bool {
        let jwt = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .unwrap_or_default();
        let Some((input, signature)) = jwt.rsplit_once('.') else {
            return false;
        };
        let claims = input.split('.').nth(1).unwrap_or_default();
        let claims: Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(claims).unwrap_or_default())
                .unwrap_or_default();
        UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, &fake.public)
            .verify(
                input.as_bytes(),
                &URL_SAFE_NO_PAD.decode(signature).unwrap_or_default(),
            )
            .is_ok()
            && claims["iss"] == CLIENT_ID
    }

    fn installation() -> Value {
        json!({"id": INSTALLATION, "account": {"login": "acme", "type": "Organization"},
               "repository_selection": "selected",
               "html_url": "https://github.com/organizations/acme/settings/installations/4242"})
    }

    /// GitHub's REST API as far as Zeabur uses it, at `http://127.0.0.1:<port>`.
    async fn github() -> (String, Arc<Fake>) {
        let key = KeyPair::generate(KeySize::Rsa2048).unwrap();
        let der = STANDARD.encode(key.as_der().unwrap().as_ref());
        let fake = Arc::new(Fake {
            pem: format!("-----BEGIN PRIVATE KEY-----\n{der}\n-----END PRIVATE KEY-----\n"),
            public: key.public_key().as_ref().to_vec(),
            converted: AtomicBool::new(false),
        });
        let router = Router::new()
            .route(
                "/app-manifests/{code}/conversions",
                post(
                    async |Shared(fake): Shared<Arc<Fake>>, Path(code): Path<String>| {
                        if code != "good-code" || fake.converted.swap(true, Ordering::SeqCst) {
                            return (StatusCode::NOT_FOUND, Json(json!({"message": "Not Found"})));
                        }
                        (
                            StatusCode::CREATED,
                            Json(
                                json!({"id": 99, "slug": "zeabur-shop", "client_id": CLIENT_ID,
                                "pem": fake.pem, "webhook_secret": WEBHOOK_SECRET,
                                "html_url": "https://github.com/apps/zeabur-shop",
                                "owner": {"login": "acme", "type": "Organization"}}),
                            ),
                        )
                    },
                ),
            )
            .route(
                "/app/installations",
                get(
                    async |Shared(fake): Shared<Arc<Fake>>, headers: HeaderMap| {
                        if !signed_by_the_app(&fake, &headers) {
                            return (
                                StatusCode::UNAUTHORIZED,
                                Json(json!({"message": "Bad JWT"})),
                            );
                        }
                        (StatusCode::OK, Json(json!([installation()])))
                    },
                ),
            )
            .route(
                "/app/installations/{id}",
                get(
                    async |Shared(fake): Shared<Arc<Fake>>, headers: HeaderMap| {
                        if !signed_by_the_app(&fake, &headers) {
                            return (
                                StatusCode::UNAUTHORIZED,
                                Json(json!({"message": "Bad JWT"})),
                            );
                        }
                        (StatusCode::OK, Json(installation()))
                    },
                ),
            )
            .route(
                "/app/installations/{id}/access_tokens",
                post(
                    async |Shared(fake): Shared<Arc<Fake>>,
                           headers: HeaderMap,
                           Json(body): Json<Value>| {
                        if !signed_by_the_app(&fake, &headers)
                            || body != json!({"permissions": {"metadata": "read"}})
                        {
                            return (StatusCode::FORBIDDEN, Json(json!({"message": "No"})));
                        }
                        (
                            StatusCode::CREATED,
                            Json(json!({"token": "ghs_metadata_only"})),
                        )
                    },
                ),
            )
            .route(
                "/installation/repositories",
                get(async |headers: HeaderMap| {
                    if headers.get("authorization").and_then(|v| v.to_str().ok())
                        != Some("Bearer ghs_metadata_only")
                    {
                        return (StatusCode::UNAUTHORIZED, Json(json!({"message": "No"})));
                    }
                    (
                        StatusCode::OK,
                        Json(json!({"total_count": 2, "repositories": [
                            {"full_name": "acme/web", "default_branch": "main", "private": true},
                            {"full_name": "acme/api", "default_branch": "trunk", "private": true},
                        ]})),
                    )
                }),
            )
            .with_state(Arc::clone(&fake));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await });
        (format!("http://{address}"), fake)
    }

    fn namespace(name: &str) -> Namespace {
        serde_json::from_value(json!({"metadata": {"name": name}})).unwrap()
    }

    fn cluster() -> MemoryCluster {
        let names = ["builder-system", "shop", "blog"];
        MemoryCluster::new(State {
            identity: Identity {
                username: "alice".to_owned(),
                groups: vec!["admins".to_owned()],
            },
            namespaces: names.iter().map(|n| namespace(n)).collect(),
            objects: names
                .iter()
                .map(|n| {
                    (
                        (*n).to_owned(),
                        NamespaceObjects {
                            namespace: (*n).to_owned(),
                            ..NamespaceObjects::default()
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>(),
            ..State::default()
        })
    }

    async fn setup() -> (MemoryCluster, Router, Arc<Fake>) {
        let (api, fake) = github().await;
        let cluster = cluster();
        let settings = Settings {
            github: GitHubSettings {
                web_url: "https://github.com".to_owned(),
                api_url: api,
            },
            ..Settings::default()
        };
        let app = app_with(&cluster, settings);
        (cluster, app, fake)
    }

    async fn start(app: &Router) -> String {
        let request = json!({"name": "Zeabur shop", "redirectUrl":
            "https://zeabur.example.com/settings/github-app"});
        let (status, started) =
            send(app, "POST", &format!("{BASE}/manifest"), Some(&request)).await;
        assert_eq!(status, StatusCode::OK, "{started}");
        let form = started["formUrl"].as_str().unwrap().to_owned();
        form.split_once("state=").unwrap().1.to_owned()
    }

    async fn apply(app: &Router, planned: &Value) -> Value {
        let (status, done) = send(
            app,
            "POST",
            &format!("{BASE}/apply"),
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{done}");
        done
    }

    async fn made(app: &Router, fake: &Fake) {
        let state = start(app).await;
        let state = form_urlencoded::parse(format!("s={state}").as_bytes())
            .next()
            .unwrap()
            .1
            .into_owned();
        let body = json!({"code": "good-code", "state": state});
        let (status, planned) = send(app, "POST", &format!("{BASE}/conversion"), Some(&body)).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        let key_line = fake.pem.lines().nth(1).unwrap();
        assert!(!planned.to_string().contains(key_line), "{planned}");
        assert!(!planned.to_string().contains(WEBHOOK_SECRET), "{planned}");
        apply(app, &planned).await;
    }

    #[tokio::test]
    async fn an_admin_makes_the_app_and_no_answer_shows_its_key() {
        let (cluster, app, fake) = setup().await;
        made(&app, &fake).await;
        let values = cluster
            .inspect(|s| s.secret_values["builder-system/github-app"].clone())
            .unwrap();
        assert_eq!(values["client-id"], CLIENT_ID.as_bytes());
        assert_eq!(values["private-key"], fake.pem.as_bytes());
        assert_eq!(values["webhook-secret"], WEBHOOK_SECRET.as_bytes());

        let (status, found) = send(&app, "GET", BASE, None).await;
        assert_eq!(status, StatusCode::OK, "{found}");
        assert!(!found.to_string().contains(fake.pem.lines().nth(1).unwrap()));
        assert_eq!(
            found["app"]["installUrl"],
            "https://github.com/apps/zeabur-shop/installations/new"
        );
        assert_eq!(found["installations"][0]["account"], "acme", "{found}");
        assert_eq!(found["canChange"], "allowed");
    }

    #[tokio::test]
    async fn the_form_sends_the_manifest_to_github_only() {
        let (_cluster, app, _fake) = setup().await;
        let state = start(&app).await;
        let request = axum::http::Request::get(format!("{BASE}/form?state={state}"))
            .body(axum::body::Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let policy = response.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .to_owned();
        assert!(
            policy.contains("form-action https://github.com;"),
            "{policy}"
        );
        assert!(!policy.contains("script-src"), "{policy}");
        assert!(policy.contains("style-src 'sha256-"), "{policy}");
        assert!(!policy.contains("unsafe"), "{policy}");
        let page = http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes();
        let page = String::from_utf8_lossy(&page);
        assert!(
            page.contains("action=\"https://github.com/settings/apps/new?state="),
            "{page}"
        );
        assert!(page.contains("name=\"manifest\""), "{page}");
        assert!(
            page.contains("&quot;statuses&quot;:&quot;write&quot;"),
            "{page}"
        );

        let request = axum::http::Request::get(format!("{BASE}/form?state=ghs1.forged"))
            .body(axum::body::Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn the_flow_belongs_to_whoever_started_it() {
        let (cluster, app, fake) = setup().await;
        let state = start(&app).await;
        let state = form_urlencoded::parse(format!("s={state}").as_bytes())
            .next()
            .unwrap()
            .1
            .into_owned();
        cluster.update(|s| s.identity.username = "mallory".to_owned());
        let body = json!({"code": "good-code", "state": state});
        let (status, refused) =
            send(&app, "POST", &format!("{BASE}/conversion"), Some(&body)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
        assert!(
            !fake.converted.load(Ordering::SeqCst),
            "the code is not spent"
        );
    }

    #[tokio::test]
    async fn a_forged_state_does_not_spend_the_code() {
        let (_cluster, app, fake) = setup().await;
        let body = json!({"code": "good-code", "state": "ghs1.forged"});
        let (status, refused) =
            send(&app, "POST", &format!("{BASE}/conversion"), Some(&body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
        assert!(
            refused["detail"].as_str().unwrap().contains("Start again"),
            "{refused}"
        );
        assert!(
            !fake.converted.load(Ordering::SeqCst),
            "the code is not spent"
        );
    }

    #[tokio::test]
    async fn a_grant_writes_builders_grants_and_what_each_project_may_build() {
        let (cluster, app, fake) = setup().await;
        made(&app, &fake).await;
        let grant =
            |namespaces: Value| json!({"installationId": INSTALLATION, "namespaces": namespaces});
        let (status, planned) = send(
            &app,
            "POST",
            &format!("{BASE}/plan"),
            Some(&grant(json!(["shop"]))),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        apply(&app, &planned).await;

        let (status, project) = send(&app, "GET", "/api/v1/projects/shop/github-app", None).await;
        assert_eq!(status, StatusCode::OK, "{project}");
        let installation = &project["installations"][0];
        assert_eq!(installation["account"], "acme");
        assert_eq!(
            installation["repositories"][0],
            json!({"fullName": "acme/api", "defaultBranch": "trunk", "private": true})
        );
        let (_, found) = send(&app, "GET", BASE, None).await;
        assert_eq!(found["installations"][0]["namespaces"], json!(["shop"]));

        let (status, planned) = send(
            &app,
            "POST",
            &format!("{BASE}/plan"),
            Some(&grant(json!(["blog"]))),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        assert!(
            planned["warnings"]
                .as_array()
                .unwrap()
                .contains(&json!("removesNamespaces")),
            "{planned}"
        );
        apply(&app, &planned).await;
        let (_, shop) = send(&app, "GET", "/api/v1/projects/shop/github-app", None).await;
        assert_eq!(shop["installations"], json!([]), "{shop}");
        let (_, blog) = send(&app, "GET", "/api/v1/projects/blog/github-app", None).await;
        assert_eq!(blog["installations"][0]["id"], INSTALLATION);
        let grants = cluster
            .inspect(|s| {
                s.writes
                    .iter()
                    .rev()
                    .find(|w| w.target.name == "github-app-installations")
                    .map(|w| w.body.clone())
            })
            .flatten()
            .unwrap();
        let WriteBody::JsonPatch(ops) = grants else {
            panic!("{grants:?}")
        };
        assert_eq!(ops[1]["value"], json!({"4242": "blog"}), "{ops}");
    }

    #[tokio::test]
    async fn someone_who_may_not_write_builder_system_cannot_start_the_flow() {
        let (cluster, app, _fake) = setup().await;
        cluster.update(|s| {
            s.forbidden_namespaces.insert("builder-system".to_owned());
        });
        let request = json!({"name": "Zeabur shop", "redirectUrl":
            "https://zeabur.example.com/settings/github-app"});
        let (status, refused) =
            send(&app, "POST", &format!("{BASE}/manifest"), Some(&request)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
        assert!(refused.get("formUrl").is_none(), "{refused}");
    }

    #[tokio::test]
    async fn someone_who_may_not_read_builder_system_is_told_which_read() {
        let (cluster, app, fake) = setup().await;
        made(&app, &fake).await;
        cluster.update(|s| {
            s.forbidden_namespaces.insert("builder-system".to_owned());
        });
        let (status, found) = send(&app, "GET", BASE, None).await;
        assert_eq!(status, StatusCode::OK, "{found}");
        assert!(found.get("app").is_none(), "{found}");
        let body = json!({"installationId": INSTALLATION, "namespaces": ["shop"]});
        let (status, refused) = send(&app, "POST", &format!("{BASE}/plan"), Some(&body)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
        assert_eq!(refused["denied"]["resource"], "secrets", "{refused}");
        assert_eq!(
            refused["denied"]["namespace"], "builder-system",
            "{refused}"
        );
    }

    #[tokio::test]
    async fn an_http_api_url_in_the_secret_is_refused_as_builder_does() {
        let (cluster, app, fake) = setup().await;
        made(&app, &fake).await;
        cluster.update(|s| {
            s.secret_values
                .get_mut("builder-system/github-app")
                .unwrap()
                .insert(
                    "api-url".to_owned(),
                    b"http://ghe.example.com/api/v3".to_vec(),
                );
        });
        let (status, found) = send(&app, "GET", BASE, None).await;
        assert_eq!(status, StatusCode::OK, "{found}");
        let problem = &found["installationsProblem"];
        assert_eq!(
            problem["detail"], "`api-url` must be an https URL",
            "{found}"
        );
        assert_eq!(
            problem["summary"]["key"], "problem.summary.githubApiUrl",
            "{found}"
        );
        let body = json!({"installationId": INSTALLATION, "namespaces": ["shop"]});
        let (status, refused) = send(&app, "POST", &format!("{BASE}/plan"), Some(&body)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
        assert_eq!(refused["detail"], "`api-url` must be an https URL");
        assert_eq!(refused["summary"]["key"], "problem.summary.githubApiUrl");
    }
}
