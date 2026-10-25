//! Uploading a folder for a `Build`: Zeabur pushes what arrives to the registry as the artifact
//! Zeabur Builder reads, with the token of the `Build`'s upload Secret and only to the registry
//! that Secret names; nothing is pushed for a token the controller has not issued or for a
//! person who may not read the `Build`; naming the digest is a plan like any other write.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::body::Body;
    use axum::extract::{Request, State};
    use axum::http::{StatusCode, header};
    use axum::response::{IntoResponse as _, Response};
    use http_body_util::BodyExt as _;
    use serde_json::{Value, json};
    use sha2::{Digest as _, Sha256};
    use tower::ServiceExt as _;
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::memory::{MemoryCluster, State as ClusterState};
    use zeabur_kube::{Identity, NamespaceObjects, ServedResource};

    use crate::common::{app, plan_and_apply};

    const TOKEN: &str = "registry-token-1";

    /// What a registry that takes pushes with one token keeps.
    #[derive(Clone, Default)]
    struct Registry {
        blobs: BTreeMap<String, Vec<u8>>,
        manifests: BTreeMap<String, Vec<u8>>,
        sessions: BTreeMap<String, Vec<u8>>,
        asked_with: Vec<String>,
    }

    type Shared = Arc<Mutex<Registry>>;

    fn heard(registry: &Shared) -> Vec<String> {
        registry.lock().unwrap().asked_with.clone()
    }

    fn digest(bytes: &[u8]) -> String {
        format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
    }

    #[expect(
        clippy::significant_drop_tightening,
        reason = "a test registry that answers inside its lock"
    )]
    async fn registry_call(State(registry): State<Shared>, request: Request) -> Response {
        let (parts, body) = request.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes().to_vec();
        let auth = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let mut registry = registry.lock().unwrap();
        registry.asked_with.push(auth.clone());
        if auth != format!("Bearer {TOKEN}") {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        let path = parts.uri.path().to_owned();
        let query = parts.uri.query().unwrap_or_default().to_owned();
        let wanted = form_urlencoded::parse(query.as_bytes())
            .find(|(k, _)| k == "digest")
            .map(|(_, v)| v.into_owned());
        let last = path.rsplit('/').next().unwrap().to_owned();
        let located = |session: &str| {
            let base = path.trim_end_matches('/');
            let base = base
                .strip_suffix(&last)
                .unwrap_or(base)
                .trim_end_matches('/');
            Response::builder()
                .status(StatusCode::ACCEPTED)
                .header(header::LOCATION, format!("{base}/{session}"))
                .body(Body::empty())
                .unwrap()
        };
        match parts.method.as_str() {
            "POST" => located("session-1"),
            "PATCH" => {
                registry
                    .sessions
                    .entry(last.clone())
                    .or_default()
                    .extend(bytes);
                located(&last)
            }
            "PUT" if path.contains("/manifests/") => {
                let digest = digest(&bytes);
                registry.manifests.insert(last, bytes);
                Response::builder()
                    .status(StatusCode::CREATED)
                    .header("docker-content-digest", digest)
                    .body(Body::empty())
                    .unwrap()
            }
            "PUT" => {
                let mut data = registry.sessions.remove(&last).unwrap_or_default();
                data.extend(bytes);
                if wanted.as_deref() != Some(digest(&data).as_str()) {
                    return StatusCode::BAD_REQUEST.into_response();
                }
                registry.blobs.insert(digest(&data), data);
                StatusCode::CREATED.into_response()
            }
            _ => StatusCode::NOT_FOUND.into_response(),
        }
    }

    async fn serve_registry() -> (String, Shared) {
        let registry = Shared::default();
        let router = Router::new()
            .fallback(registry_call)
            .with_state(Arc::clone(&registry));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        tokio::spawn(async move { axum::serve(listener, router).await });
        (address, registry)
    }

    fn cluster(registry: &str, answered: &str) -> MemoryCluster {
        let namespace: Namespace = serde_json::from_value(json!({"metadata": {"name": "shop",
            "labels": {"zeabur.com/project": "true"}}}))
        .unwrap();
        let cluster = MemoryCluster::new(ClusterState {
            identity: Identity {
                username: "alice".to_owned(),
                groups: Vec::new(),
            },
            namespaces: vec![namespace],
            objects: BTreeMap::from([(
                "shop".to_owned(),
                NamespaceObjects {
                    namespace: "shop".to_owned(),
                    ..NamespaceObjects::default()
                },
            )]),
            ..ClusterState::default()
        });
        let config = json!({"auths": {registry: {"registrytoken": TOKEN}}}).to_string();
        cluster.update(|s| {
            s.served.push(ServedResource::readable(
                "builder.zeabur.com",
                "v1alpha1",
                "builds",
                "Build",
                true,
            ));
            s.dynamic.push(
                serde_json::from_value(json!({
                    "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "Build",
                    "metadata": {"name": "site", "namespace": "shop", "resourceVersion": "7",
                        "generation": 1},
                    "spec": {"builderRef": "railpack", "source": {"archive": {}}},
                    "status": {"uploadToken": {"request": answered, "secretName": "site-upload",
                        "expiresAt": "2099-01-01T00:00:00Z"}}
                }))
                .unwrap(),
            );
            s.secret_values.insert(
                "shop/site-upload".to_owned(),
                BTreeMap::from([
                    ("token".to_owned(), TOKEN.as_bytes().to_vec()),
                    (".dockerconfigjson".to_owned(), config.into_bytes()),
                ]),
            );
        });
        cluster
    }

    async fn post(app: &Router, uri: &str, body: Vec<u8>) -> (StatusCode, Value) {
        let request = Request::builder()
            .method("POST")
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/gzip")
            .header(header::CONTENT_LENGTH, body.len())
            .body(Body::from(body))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    const UPLOAD: &str = "/api/v1/projects/shop/builds/site/source?request=u1";

    /// The folder lands as one layer, byte for byte, in a manifest of the type Zeabur Builder
    /// reads, under a tag; the digest answered is the manifest's; the token of the Secret is the
    /// only credential the registry saw.
    #[tokio::test]
    async fn a_folder_is_pushed_as_the_artifact_builder_reads() {
        let (address, registry) = serve_registry().await;
        let app = app(&cluster(&address, "u1"));
        let folder: Vec<u8> = (0..70_000_u32).flat_map(u32::to_le_bytes).collect();
        let (status, answer) = post(&app, UPLOAD, folder.clone()).await;
        assert_eq!(status, StatusCode::OK, "{answer}");

        let registry = registry.lock().unwrap().clone();
        let tag = answer["tag"].as_str().unwrap();
        let manifest = registry.manifests.get(tag).expect("pushed under the tag");
        assert_eq!(answer["digest"], digest(manifest));
        let manifest: Value = serde_json::from_slice(manifest).unwrap();
        assert_eq!(
            manifest["artifactType"],
            "application/vnd.zeabur.source.v1+tar"
        );
        let layers = manifest["layers"].as_array().unwrap();
        assert_eq!(layers.len(), 1);
        assert_eq!(
            layers[0]["mediaType"],
            "application/vnd.oci.image.layer.v1.tar+gzip"
        );
        assert_eq!(layers[0]["size"], folder.len());
        assert_eq!(
            registry.blobs.get(layers[0]["digest"].as_str().unwrap()),
            Some(&folder)
        );
        assert_eq!(
            registry
                .blobs
                .get(manifest["config"]["digest"].as_str().unwrap())
                .map(Vec::as_slice),
            Some(&b"{}"[..])
        );
        assert!(
            registry
                .asked_with
                .iter()
                .all(|a| a == &format!("Bearer {TOKEN}")),
            "{:?}",
            registry.asked_with
        );
        assert!(!answer.to_string().contains(TOKEN));
    }

    /// A token the controller has not issued for this request, a body over the limit and a
    /// Build that follows Git push nothing.
    #[tokio::test]
    async fn nothing_is_pushed_without_an_issued_token_or_within_limits() {
        let (address, registry) = serve_registry().await;
        let cluster = cluster(&address, "u1");
        let app = app(&cluster);
        let (status, _) = post(
            &app,
            "/api/v1/projects/shop/builds/site/source?request=u2",
            vec![1; 10],
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        let request = Request::builder()
            .method("POST")
            .uri(UPLOAD)
            .header(header::CONTENT_LENGTH, 256 * 1024 * 1024 + 1)
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        cluster.update(|s| {
            s.dynamic[0].data["spec"]["source"] =
                json!({"git": {"url": "https://example.com/a/b"}});
        });
        let (status, _) = post(&app, UPLOAD, vec![1; 10]).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(heard(&registry), Vec::<String>::new());
    }

    /// A person who may not read the project cannot push to it, and the registry hears nothing.
    #[tokio::test]
    async fn a_person_who_may_not_read_the_build_pushes_nothing() {
        let (address, registry) = serve_registry().await;
        let cluster = cluster(&address, "u1");
        cluster.update(|s| {
            s.forbidden_namespaces.insert("shop".to_owned());
        });
        let (status, _) = post(&app(&cluster), UPLOAD, vec![1; 10]).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(heard(&registry), Vec::<String>::new());
    }

    /// A registry that points the upload at another address never gets the token there.
    #[tokio::test]
    async fn the_token_goes_to_no_other_address() {
        let (elsewhere, other) = serve_registry().await;
        let redirecting = Router::new().fallback(move || {
            let elsewhere = elsewhere.clone();
            async move {
                Response::builder()
                    .status(StatusCode::ACCEPTED)
                    .header(
                        header::LOCATION,
                        format!("http://{elsewhere}/v2/x/blobs/uploads/1"),
                    )
                    .body(Body::empty())
                    .unwrap()
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        tokio::spawn(async move { axum::serve(listener, redirecting).await });
        let (status, _) = post(&app(&cluster(&address, "u1")), UPLOAD, vec![1; 10]).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(heard(&other), Vec::<String>::new());
    }

    /// Naming the pushed digest is a plan: the Build's `archive.digest` is patched, and a digest
    /// of another form is refused.
    #[tokio::test]
    async fn the_digest_is_named_by_a_plan() {
        let (address, _) = serve_registry().await;
        let cluster = cluster(&address, "u1");
        let app = app(&cluster);
        let digest = format!("sha256:{}", "a".repeat(64));
        let (status, applied) = plan_and_apply(
            &app,
            "shop",
            json!([{"op": "archiveDigest", "build": "site", "digest": digest}]),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        assert!(
            writes
                .iter()
                .any(|w| format!("{:?}", w.body).contains(&digest)),
            "{writes:?}"
        );
        let (status, _) = plan_and_apply(
            &app,
            "shop",
            json!([{"op": "archiveDigest", "build": "site", "digest": "latest"}]),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
