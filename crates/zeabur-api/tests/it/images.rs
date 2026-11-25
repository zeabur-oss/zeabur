//! An image's ports from its registry: a private image is read with the project's pull Secret,
//! read as the person, and the credentials go only to the image's registry. An image in the build
//! registry is read trusting the CA that the catalog's `provides.registry.ca` names, read as the
//! person.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};

    use axum::http::StatusCode;
    use secrecy::ExposeSecret as _;
    use serde_json::json;
    use sha2::{Digest as _, Sha256};
    use zeabur_api::Settings;
    use zeabur_api::images::Inspector;
    use zeabur_api::images::registry::{Answer, Get, Transport};
    use zeabur_kube::memory::{MemoryCluster, State};

    use crate::common::send;
    use crate::support::{RegistryCatalog, TestApi};

    /// `me:pw`, as Basic credentials.
    const BASIC: &str = "Basic bWU6cHc=";
    const PATH: &str = "/api/v1/projects/shop/images/inspect";

    fn digest(body: &[u8]) -> String {
        format!("sha256:{}", hex::encode(Sha256::digest(body)))
    }

    /// A registry that wants Basic credentials and holds one single-platform image.
    struct Private {
        routes: BTreeMap<String, Vec<u8>>,
        sent: Mutex<Vec<(String, Option<String>)>>,
    }

    impl Private {
        fn new() -> Self {
            let config = serde_json::to_vec(&json!({
                "architecture": "arm64", "os": "linux",
                "config": {"ExposedPorts": {"3000/tcp": {}}, "User": "node"},
            }))
            .unwrap();
            let manifest = serde_json::to_vec(&json!({
                "schemaVersion": 2,
                "mediaType": "application/vnd.docker.distribution.manifest.v2+json",
                "config": {"digest": digest(&config), "size": config.len()},
                "layers": [],
            }))
            .unwrap();
            let base = "https://registry.test/v2/acme/api";
            Self {
                routes: BTreeMap::from([
                    (format!("{base}/manifests/1"), manifest),
                    (format!("{base}/blobs/{}", digest(&config)), config),
                ]),
                sent: Mutex::new(Vec::new()),
            }
        }
    }

    impl Transport for Private {
        fn get<'a>(
            &'a self,
            get: Get,
        ) -> Pin<Box<dyn Future<Output = Result<Answer, String>> + Send + 'a>> {
            let authorization = get
                .authorization
                .as_ref()
                .map(|a| a.expose_secret().to_owned());
            self.sent
                .lock()
                .unwrap()
                .push((get.url.clone(), authorization.clone()));
            let answer = if authorization.as_deref() == Some(BASIC) {
                self.routes.get(&get.url).map_or_else(
                    || Answer {
                        status: 404,
                        ..Answer::default()
                    },
                    |body| Answer {
                        status: 200,
                        body: body.clone(),
                        ..Answer::default()
                    },
                )
            } else {
                Answer {
                    status: 401,
                    www_authenticate: Some("Basic realm=\"registry.test\"".to_owned()),
                    ..Answer::default()
                }
            };
            Box::pin(async move { Ok(answer) })
        }
    }

    fn cluster(forbidden: bool) -> MemoryCluster {
        let config = json!({"auths": {
            "registry.test": {"auth": "bWU6cHc="},
            "ghcr.io": {"username": "other", "password": "elsewhere"},
        }});
        MemoryCluster::new(State {
            secret_values: BTreeMap::from([(
                "shop/api-registry".to_owned(),
                BTreeMap::from([(
                    ".dockerconfigjson".to_owned(),
                    serde_json::to_vec(&config).unwrap(),
                )]),
            )]),
            nodes: vec![
                serde_json::from_value(json!({"metadata": {"name": "a",
                    "labels": {"kubernetes.io/arch": "arm64"}}}))
                .unwrap(),
            ],
            forbidden_namespaces: if forbidden {
                ["shop".to_owned()].into()
            } else {
                std::collections::BTreeSet::new()
            },
            ..State::default()
        })
    }

    fn app(cluster: &MemoryCluster, registry: &Arc<Private>) -> axum::Router {
        let transport: Arc<dyn Transport> = Arc::<Private>::clone(registry);
        TestApi::new(cluster)
            .with_images(Inspector::new(transport, Vec::new()))
            .router()
    }

    #[tokio::test]
    async fn a_private_image_is_read_with_the_projects_pull_secret() {
        let registry = Arc::new(Private::new());
        let app = app(&cluster(false), &registry);
        let (status, body) = send(
            &app,
            "POST",
            PATH,
            Some(&json!({"image": "registry.test/acme/api:1", "pullSecret": "api-registry"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body["exposedPorts"],
            json!([{"port": 3000, "protocol": "TCP"}])
        );
        assert_eq!(body["platform"], "linux/arm64");
        assert_eq!(body["user"], "node");
        assert_eq!(body["runsAsRoot"], false);
        assert!(body.get("unreadable").is_none(), "{body}");
        let sent = registry.sent.lock().unwrap().clone();
        assert!(
            sent.iter()
                .all(|(url, _)| url.starts_with("https://registry.test/")),
            "{sent:?}"
        );
    }

    #[tokio::test]
    async fn a_secret_the_person_may_not_read_lends_nothing() {
        let registry = Arc::new(Private::new());
        let app = app(&cluster(true), &registry);
        let (status, body) = send(
            &app,
            "POST",
            PATH,
            Some(&json!({"image": "registry.test/acme/api:1", "pullSecret": "api-registry"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["unreadable"], "credentials");
        assert_eq!(body["exposedPorts"], json!([]));
        assert!(registry.sent.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn typed_credentials_go_only_to_the_registry_they_name() {
        let registry = Arc::new(Private::new());
        let app = app(&cluster(false), &registry);
        let (status, body) = send(
            &app,
            "POST",
            PATH,
            Some(&json!({"image": "registry.test/acme/api:1",
                         "registry": {"server": "ghcr.io", "username": "me", "password": "pw"}})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["unreadable"], "unauthorized");
        assert!(
            registry
                .sent
                .lock()
                .unwrap()
                .iter()
                .all(|(_, authorization)| authorization.is_none())
        );
        let (_, body) = send(
            &app,
            "POST",
            PATH,
            Some(&json!({"image": "registry.test/acme/api:1",
                         "registry": {"server": "https://registry.test", "username": "me", "password": "pw"}})),
        )
        .await;
        assert_eq!(body["exposedPorts"][0]["port"], 3000, "{body}");
    }

    #[tokio::test]
    async fn what_is_not_an_image_reference_is_a_bad_request() {
        let app = app(&cluster(false), &Arc::new(Private::new()));
        let (status, body) =
            send(&app, "POST", PATH, Some(&json!({"image": "Not An Image"}))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }

    /// An HTTPS registry whose certificate CA A signed, holding `<address>/shop/web:1`.
    async fn tls_registry() -> String {
        let config = json!({"architecture": "amd64", "os": "linux",
            "config": {"ExposedPorts": {"8080/tcp": {}}, "User": "node"}})
        .to_string();
        let manifest = json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": {"mediaType": "application/vnd.oci.image.config.v1+json",
                "digest": digest(config.as_bytes()), "size": config.len()},
            "layers": [],
        })
        .to_string();
        let config_path = format!("/v2/shop/web/blobs/{}", digest(config.as_bytes()));
        let address = zeabur_test_support::tls::serve_answering(move |target| {
            let (content_type, body) = if target == "/v2/shop/web/manifests/1" {
                ("application/vnd.oci.image.manifest.v1+json", manifest.as_str())
            } else if target == config_path {
                ("application/octet-stream", config.as_str())
            } else {
                return "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                    .to_owned();
            };
            format!(
                "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
        })
        .await;
        address.to_string()
    }

    /// A project that never built: the only object is the Secret `zot/zot-tls` that the catalog
    /// names in `provides.registry.ca`, holding `ca` when there is one.
    fn published(ca: Option<&str>, forbidden: bool) -> MemoryCluster {
        MemoryCluster::new(State {
            secret_values: ca
                .map(|ca| {
                    (
                        "zot/zot-tls".to_owned(),
                        BTreeMap::from([("ca.crt".to_owned(), ca.as_bytes().to_vec())]),
                    )
                })
                .into_iter()
                .collect(),
            forbidden_namespaces: if forbidden {
                ["zot".to_owned()].into()
            } else {
                std::collections::BTreeSet::new()
            },
            ..State::default()
        })
    }

    /// The build registry is read trusting the CA that the catalog's `provides.registry.ca`
    /// names, in a project with no Build: only when that Secret holds the CA that signed the
    /// registry's certificate and the person may read it.
    #[tokio::test]
    async fn the_build_registry_is_read_with_the_published_ca() {
        use zeabur_test_support::tls::{ca_a, ca_b};

        let registry = tls_registry().await;
        let image = format!("{registry}/shop/web:1");
        for (case, ca, forbidden, read) in [
            ("the CA that signed it", Some(ca_a()), false, true),
            ("another CA", Some(ca_b()), false, false),
            ("no Secret", None, false, false),
            (
                "a Secret the person may not read",
                Some(ca_a()),
                true,
                false,
            ),
        ] {
            let mut settings = Settings::default();
            settings.builds.registry = Some(registry.clone());
            let app = TestApi::new(&published(ca, forbidden))
                .with_settings(settings)
                .with_catalog(Arc::new(RegistryCatalog {
                    host: registry.clone(),
                }))
                .router();
            let (status, body) = send(&app, "POST", PATH, Some(&json!({"image": image}))).await;
            assert_eq!(status, StatusCode::OK, "{case}: {body}");
            if read {
                assert_eq!(
                    body["exposedPorts"],
                    json!([{"port": 8080, "protocol": "TCP"}]),
                    "{case}: {body}"
                );
                assert_eq!(body["user"], "node", "{case}: {body}");
            } else {
                assert_eq!(body["unreadable"], "unreachable", "{case}: {body}");
            }
        }
    }

    /// The published CA is trusted only for the build registry: another registry the catalog
    /// provides is read with the server's own trust.
    #[tokio::test]
    async fn only_the_build_registry_trusts_the_published_ca() {
        let registry = tls_registry().await;
        let mut settings = Settings::default();
        settings.builds.registry = Some("zot.zot.svc:5000".to_owned());
        let app = TestApi::new(&published(Some(zeabur_test_support::tls::ca_a()), false))
            .with_settings(settings)
            .with_catalog(Arc::new(RegistryCatalog {
                host: registry.clone(),
            }))
            .router();
        let image = format!("{registry}/shop/web:1");
        let (status, body) = send(&app, "POST", PATH, Some(&json!({"image": image}))).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["unreadable"], "unreachable", "{body}");
    }
}
