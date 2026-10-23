//! Deploying from Git, against an in-memory cluster: a deploy key's private half reaches only
//! the Secret the apply creates, never an answer; a key works only for the person and project it
//! was made for; a service waiting for its first built image is building, or failed with the
//! build, rather than failing to pull an image that does not exist yet; and a running service
//! whose next build cannot happen says so.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::Router;
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_api::settings::{BuildSettings, Settings};
    use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
    use zeabur_kube::k8s_openapi::api::core::v1::{Namespace, Pod};
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects, ServedResource, WriteBody};

    use crate::common::{app, app_with, send};

    const IMAGE: &str = "zot.zot.svc:5000/shop/site:pending";

    fn namespace(name: &str) -> Namespace {
        serde_json::from_value(json!({"metadata": {"name": name,
            "labels": {"zeabur.com/project": "true"}}}))
        .unwrap()
    }

    fn cluster() -> MemoryCluster {
        let objects = |name: &str| NamespaceObjects {
            namespace: name.to_owned(),
            ..NamespaceObjects::default()
        };
        let cluster = MemoryCluster::new(State {
            identity: Identity {
                username: "alice".to_owned(),
                groups: Vec::new(),
            },
            namespaces: vec![namespace("shop"), namespace("blog")],
            objects: BTreeMap::from([
                ("shop".to_owned(), objects("shop")),
                ("blog".to_owned(), objects("blog")),
            ]),
            ..State::default()
        });
        cluster.update(|s| {
            for (resource, kind) in [("builds", "Build"), ("buildruns", "BuildRun")] {
                s.served.push(ServedResource::readable(
                    "builder.zeabur.com",
                    "v1alpha1",
                    resource,
                    kind,
                    true,
                ));
            }
        });
        cluster
    }

    async fn deploy_key(app: &Router, project: &str) -> Value {
        let (status, key) = send(
            app,
            "POST",
            &format!("/api/v1/projects/{project}/deploy-keys"),
            Some(&json!({"host": "github.com"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{key}");
        key
    }

    fn build() -> Value {
        json!({"apiVersion": "builder.zeabur.com/v1alpha1", "kind": "Build",
               "metadata": {"name": "site"},
               "spec": {"builderRef": "dockerfile",
                        "source": {"git": {"url": "git@github.com:acme/site.git", "ref": "main"},
                                   "credentialsRef": {"name": "site-git"}},
                        "output": {"image": "zot.zot.svc:5000/shop/site"}}})
    }

    /// The public key is one `ssh-ed25519` line; the sealed key does not show the private key; the
    /// plan lists the Secret with its values hidden and the token does not carry them either; the
    /// apply creates an `ssh-auth` Secret Zeabur Builder reads, with GitHub's host key.
    #[tokio::test]
    async fn a_deploy_key_reaches_only_its_secret() {
        let cluster = cluster();
        let app = app(&cluster);
        let key = deploy_key(&app, "shop").await;
        assert!(
            key["publicKey"]
                .as_str()
                .unwrap()
                .starts_with("ssh-ed25519 AAAA"),
            "{key}"
        );
        assert!(!key.to_string().contains("PRIVATE KEY"), "{key}");
        let request = json!({"changes": [
            {"op": "deployKey", "name": "site-git", "key": key["key"]},
            {"op": "create", "object": build()},
        ]});
        let (status, planned) =
            send(&app, "POST", "/api/v1/projects/shop/plan", Some(&request)).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        assert!(!planned.to_string().contains("PRIVATE KEY"), "{planned}");
        let kinds: Vec<&str> = planned["objects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["target"]["kind"].as_str().unwrap())
            .collect();
        assert!(
            kinds.contains(&"Secret") && kinds.contains(&"Build"),
            "{kinds:?}"
        );
        let (status, applied) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert!(!applied.to_string().contains("PRIVATE KEY"), "{applied}");
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        let secret = writes
            .iter()
            .filter(|w| w.target.kind == "Secret")
            .find_map(|w| {
                if let WriteBody::Create(body) = &w.body {
                    Some(body.clone())
                } else {
                    None
                }
            })
            .expect("the Secret is created");
        assert_eq!(secret["type"], "kubernetes.io/ssh-auth");
        assert_eq!(
            secret["metadata"]["labels"]["builder.zeabur.com/git-credentials"],
            "true"
        );
        assert!(
            secret["stringData"]["ssh-privatekey"]
                .as_str()
                .unwrap()
                .starts_with("-----BEGIN OPENSSH PRIVATE KEY-----")
        );
        assert!(
            secret["stringData"]["known_hosts"]
                .as_str()
                .unwrap()
                .starts_with("github.com ssh-ed25519 ")
        );
    }

    /// A key made for one project does not open in another's plan, nor does an altered one; a
    /// host without a pinned host key gets none.
    #[tokio::test]
    async fn a_deploy_key_works_only_where_it_was_made() {
        let cluster = cluster();
        let app = app(&cluster);
        let key = deploy_key(&app, "shop").await;
        let elsewhere =
            json!({"changes": [{"op": "deployKey", "name": "site-git", "key": key["key"]}]});
        let (status, refusal) =
            send(&app, "POST", "/api/v1/projects/blog/plan", Some(&elsewhere)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{refusal}");
        let mut altered = key["key"].as_str().unwrap().to_owned();
        altered.push('A');
        let (status, _) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/plan",
            Some(&json!({"changes": [{"op": "deployKey", "name": "site-git", "key": altered}]})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/deploy-keys",
            Some(&json!({"host": "git.example.com"})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    /// The cluster answer names the registry builds push to, from the settings.
    #[tokio::test]
    async fn the_cluster_names_the_build_registry() {
        let cluster = cluster();
        let settings = Settings {
            builds: BuildSettings {
                registry: Some("zot.zot.svc:5000".to_owned()),
            },
            ..Settings::default()
        };
        let (status, answer) = send(
            &app_with(&cluster, settings),
            "GET",
            "/api/v1/cluster",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        assert_eq!(answer["builds"]["registry"], "zot.zot.svc:5000");
    }

    fn waiting_service(cluster: &MemoryCluster, run_phase: Option<&str>) {
        waiting_service_with(cluster, run_phase, None);
    }

    fn waiting_service_with(
        cluster: &MemoryCluster,
        run_phase: Option<&str>,
        build_status: Option<&Value>,
    ) {
        let deployment: Deployment = serde_json::from_value(json!({
            "metadata": {"name": "site", "namespace": "shop", "labels": {"app": "site"},
                         "annotations": {"keel.sh/policy": "force"}},
            "spec": {"selector": {"matchLabels": {"app": "site"}},
                     "template": {"metadata": {"labels": {"app": "site"}},
                                  "spec": {"containers": [{"name": "site", "image": IMAGE}]}}}
        }))
        .unwrap();
        let pod: Pod = serde_json::from_value(json!({
            "metadata": {"name": "site-1", "namespace": "shop", "labels": {"app": "site"}},
            "spec": {"containers": [{"name": "site", "image": IMAGE}]},
            "status": {"phase": "Pending", "containerStatuses": [{
                "name": "site", "image": IMAGE, "imageID": "", "ready": false, "restartCount": 0,
                "state": {"waiting": {"reason": "ImagePullBackOff",
                                      "message": "Back-off pulling image"}}}]}
        }))
        .unwrap();
        cluster.update(|s| {
            let objects = s.objects.get_mut("shop").unwrap();
            objects.deployments.push(deployment);
            objects.pods.push(pod);
            let mut build = build();
            build["metadata"]["namespace"] = json!("shop");
            if run_phase == Some("Failed") {
                build["status"] = json!({"conditions": [{"type": "Ready", "status": "False",
                    "reason": "BuildFailed"}]});
            }
            if let Some(status) = build_status {
                build["status"] = status.clone();
            }
            s.dynamic.push(serde_json::from_value(build).unwrap());
            if let Some(phase) = run_phase {
                s.dynamic.push(
                    serde_json::from_value(json!({
                        "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "BuildRun",
                        "metadata": {"name": "site-x1", "namespace": "shop",
                            "creationTimestamp": "2026-10-23T08:00:00Z"},
                        "spec": {"buildRef": "site"},
                        "status": {"phase": phase}
                    }))
                    .unwrap(),
                );
            }
        });
    }

    async fn site(cluster: &MemoryCluster) -> Value {
        let (status, document) = send(&app(cluster), "GET", "/api/v1/projects/shop", None).await;
        assert_eq!(status, StatusCode::OK, "{document}");
        document["services"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "site")
            .cloned()
            .unwrap()
    }

    /// Before its first image, a service built from Git is building, with nothing else diagnosed;
    /// when the build fails, the service fails with the build.
    #[tokio::test]
    async fn a_service_waiting_for_its_first_image_is_building_or_failed_with_the_build() {
        let cluster = cluster();
        waiting_service(&cluster, Some("Building"));
        let service = site(&cluster).await;
        assert_eq!(service["status"]["word"], "building", "{service}");
        assert_eq!(service["status"]["health"], "progress");
        assert_eq!(service["diagnoses"], json!([]), "{service}");

        let cluster = cluster_failed();
        let service = site(&cluster).await;
        assert_eq!(service["status"]["word"], "failed", "{service}");
        assert_eq!(service["status"]["reason"], "BuildFailed");
    }

    fn cluster_failed() -> MemoryCluster {
        let cluster = cluster();
        waiting_service(&cluster, Some("Failed"));
        cluster
    }

    fn source_not_ready(reason: &str, since: &str) -> Value {
        json!({"conditions": [
            {"type": "BuilderReady", "status": "True", "reason": "Found"},
            {"type": "SourceReady", "status": "False", "reason": reason,
                "message": "no branch or tag `main`", "lastTransitionTime": since},
            {"type": "Ready", "status": "Unknown", "reason": "NotBuilt"}]})
    }

    /// A ref that cannot be resolved never builds: the service fails with the build at once, and
    /// the Builds answer carries the controller's message. A timeout waits a few minutes first.
    #[tokio::test]
    async fn a_source_that_cannot_be_resolved_fails_the_service() {
        let missing = cluster();
        waiting_service_with(
            &missing,
            None,
            Some(&source_not_ready("RefNotFound", "2026-10-23T08:00:00Z")),
        );
        let service = site(&missing).await;
        assert_eq!(service["status"]["word"], "failed", "{service}");
        assert_eq!(service["status"]["health"], "fail");
        assert_eq!(service["status"]["reason"], "BuildFailed");
        assert_eq!(service["diagnoses"], json!([]), "{service}");
        let (status, builds) =
            send(&app(&missing), "GET", "/api/v1/projects/shop/builds", None).await;
        assert_eq!(status, StatusCode::OK, "{builds}");
        assert_eq!(builds["builds"][0]["problem"]["condition"], "SourceReady");
        assert_eq!(builds["builds"][0]["problem"]["reason"], "RefNotFound");
        assert_eq!(
            builds["builds"][0]["problem"]["message"],
            "no branch or tag `main`"
        );
        assert_eq!(builds["builds"][0]["problem"]["lasting"], true);

        let slow = cluster();
        let now = zeabur_kube::k8s_openapi::jiff::Timestamp::now().to_string();
        waiting_service_with(&slow, None, Some(&source_not_ready("ResolveFailed", &now)));
        let service = site(&slow).await;
        assert_eq!(service["status"]["word"], "building", "{service}");
    }

    /// A service that runs its built image fails with the build once the source cannot be read,
    /// such as a deploy key taken off the repository, and keeps its replicas; a timeout that may
    /// clear by itself changes nothing yet.
    #[tokio::test]
    async fn a_running_service_whose_source_fails_fails_with_the_build() {
        const BUILT: &str = "zot.zot.svc:5000/shop/site:main-1-abc@sha256:00";
        let running = |problem: Option<(&str, &str)>| {
            let cluster = cluster();
            let deployment: Deployment = serde_json::from_value(json!({
                "metadata": {"name": "site", "namespace": "shop", "labels": {"app": "site"}},
                "spec": {"replicas": 1, "selector": {"matchLabels": {"app": "site"}},
                         "template": {"metadata": {"labels": {"app": "site"}},
                                      "spec": {"containers": [{"name": "site", "image": BUILT}]}}},
                "status": {"replicas": 1, "readyReplicas": 1, "availableReplicas": 1,
                           "updatedReplicas": 1}
            }))
            .unwrap();
            let pod: Pod = serde_json::from_value(json!({
                "metadata": {"name": "site-1", "namespace": "shop", "labels": {"app": "site"}},
                "spec": {"containers": [{"name": "site", "image": BUILT}]},
                "status": {"phase": "Running", "conditions": [{"type": "Ready", "status": "True"}],
                    "containerStatuses": [{"name": "site", "image": BUILT, "imageID": "",
                        "ready": true, "restartCount": 0,
                        "state": {"running": {"startedAt": "2026-10-23T08:00:00Z"}}}]}
            }))
            .unwrap();
            let mut conditions = vec![json!({"type": "Ready", "status": "True", "reason": "Built"})];
            if let Some((reason, since)) = problem {
                conditions.push(json!({"type": "SourceReady", "status": "False", "reason": reason,
                    "message": "github.com refused the key for `git`",
                    "lastTransitionTime": since}));
            }
            cluster.update(|s| {
                let objects = s.objects.get_mut("shop").unwrap();
                objects.deployments.push(deployment);
                objects.pods.push(pod);
                let mut build = build();
                build["metadata"]["namespace"] = json!("shop");
                build["status"] = json!({"latestImage": BUILT, "conditions": conditions});
                s.dynamic.push(serde_json::from_value(build).unwrap());
            });
            cluster
        };

        let service = site(&running(None)).await;
        assert_eq!(service["status"]["health"], "ok", "{service}");

        let refused = site(&running(Some(("AuthFailed", "2026-10-23T08:00:00Z")))).await;
        assert_eq!(refused["status"]["health"], "fail", "{refused}");
        assert_eq!(refused["status"]["reason"], "BuildFailed");
        assert_eq!(refused["status"]["word"], "rolloutStuck");

        let now = zeabur_kube::k8s_openapi::jiff::Timestamp::now().to_string();
        let slow = site(&running(Some(("ResolveFailed", &now)))).await;
        assert_eq!(slow["status"]["health"], "ok", "{slow}");
    }

    /// When the host refuses a key, the person can see its public half again, read from the
    /// Secret as them; a Secret without an SSH key says so.
    #[tokio::test]
    async fn a_deploy_key_shows_its_public_half_again() {
        let cluster = cluster();
        let key = zeabur_core::ssh::deploy_key("zeabur shop").unwrap();
        cluster.update(|s| {
            s.secret_values.insert(
                "shop/site-git".to_owned(),
                BTreeMap::from([("ssh-privatekey".to_owned(), key.private.clone().into_bytes())]),
            );
            s.secret_values.insert(
                "shop/site-token".to_owned(),
                BTreeMap::from([("password".to_owned(), b"t".to_vec())]),
            );
        });
        let app = app(&cluster);
        let (status, answer) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/deploy-keys/site-git/public",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        assert!(key.public.starts_with(answer["publicKey"].as_str().unwrap()));
        assert!(!answer.to_string().contains("PRIVATE"));
        let (status, _) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/deploy-keys/site-token/public",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
