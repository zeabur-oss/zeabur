//! Deploying from Git, against an in-memory cluster: the deploy key of a Git service is made at
//! apply and its private half reaches only the Secret the apply creates, never a plan or an answer;
//! a host whose key Zeabur does not pin needs its `known_hosts` lines; a service waiting for its
//! first built image is building, or failed with the build, rather than failing to pull an image
//! that does not exist yet; and a running service whose next build cannot happen says so.

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

    use crate::common::{app, send};
    use crate::support::TestApi;

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

    fn build() -> Value {
        json!({"apiVersion": "builder.zeabur.com/v1alpha1", "kind": "Build",
               "metadata": {"name": "site"},
               "spec": {"builderRef": "dockerfile",
                        "source": {"git": {"url": "git@github.com:acme/site.git", "ref": "main"},
                                   "credentialsRef": {"name": "site-git"}},
                        "output": {"image": "zot.zot.svc:5000/shop/site"}}})
    }

    /// The API with a build registry, which a Git service needs.
    fn git_app(cluster: &MemoryCluster) -> Router {
        let settings = Settings {
            builds: BuildSettings {
                registry: Some("zot.zot.svc:5000".to_owned()),
            },
            ..Settings::default()
        };
        TestApi::new(cluster).with_settings(settings).router()
    }

    /// A Git service `site` read with a deploy key.
    fn git_service(url: &str, known_hosts: Option<&str>) -> Value {
        let mut access = json!({"type": "deployKey"});
        if let Some(lines) = known_hosts {
            access["knownHosts"] = json!(lines);
        }
        json!({"op": "createGitService", "name": "site", "url": url, "branch": "main",
               "method": "dockerfile", "port": 8080, "access": access})
    }

    async fn plan_and_apply(app: &Router, changes: Value) -> (Value, Value) {
        let request = json!({ "changes": changes });
        let (status, planned) =
            send(app, "POST", "/api/v1/projects/shop/plan", Some(&request)).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        let (status, applied) = send(
            app,
            "POST",
            "/api/v1/projects/shop/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        (planned, applied)
    }

    fn created_secret(cluster: &MemoryCluster) -> Value {
        cluster
            .inspect(|s| s.writes.clone())
            .unwrap()
            .iter()
            .filter(|w| w.target.kind == "Secret")
            .find_map(|w| {
                if let WriteBody::Create(body) = &w.body {
                    Some(body.clone())
                } else {
                    None
                }
            })
            .expect("the Secret is created")
    }

    /// The plan lists the Secret with its values hidden and makes no key; the apply makes the key
    /// and creates an `ssh-auth` Secret Zeabur Builder reads, with GitHub's host key; the person
    /// then reads its public half through the `Build`.
    #[tokio::test]
    async fn a_deploy_key_is_made_at_apply_and_reaches_only_its_secret() {
        let cluster = cluster();
        let app = git_app(&cluster);
        let changes = json!([git_service("git@github.com:acme/site.git", None)]);
        let (planned, applied) = plan_and_apply(&app, changes).await;
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
        assert!(!applied.to_string().contains("PRIVATE KEY"), "{applied}");
        let secret = created_secret(&cluster);
        assert_eq!(secret["type"], "kubernetes.io/ssh-auth");
        assert_eq!(
            secret["metadata"]["labels"]["builder.zeabur.com/git-credentials"],
            "true"
        );
        let private = secret["stringData"]["ssh-privatekey"].as_str().unwrap();
        assert!(private.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));
        assert!(
            secret["stringData"]["known_hosts"]
                .as_str()
                .unwrap()
                .starts_with("github.com ssh-ed25519 ")
        );

        cluster.update(|s| {
            s.secret_values.insert(
                "shop/site-git".to_owned(),
                BTreeMap::from([("ssh-privatekey".to_owned(), private.as_bytes().to_vec())]),
            );
        });
        let (status, public) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/builds/site/deploy-key",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{public}");
        assert_eq!(
            Some(public["publicKey"].as_str().unwrap().to_owned()),
            zeabur_core::ssh::public_key(private)
        );
    }

    /// A host whose key Zeabur does not pin gets no deploy key until the person gives its
    /// `known_hosts` lines, at plan.
    #[tokio::test]
    async fn a_deploy_key_for_a_host_zeabur_does_not_pin_needs_its_lines() {
        let cluster = cluster();
        let app = git_app(&cluster);
        let request = json!({"changes": [git_service("git@git.example.com:acme/site.git", None)]});
        let (status, refusal) =
            send(&app, "POST", "/api/v1/projects/shop/plan", Some(&request)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{refusal}");
        assert!(refusal["detail"].as_str().unwrap().contains("known_hosts"));
        assert_eq!(cluster.inspect(|s| s.writes.len()), Some(0));
    }

    /// Another host's key trusts the Ed25519 and ECDSA lines the person gave for that host and
    /// port, and the host check says their fingerprints; lines for another host or only RSA are
    /// refused.
    #[tokio::test]
    async fn a_deploy_key_for_another_host_trusts_the_lines_given() {
        let cluster = cluster();
        let app = git_app(&cluster);
        let ed25519 = "ssh-ed25519 \
            AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";
        let given = format!(
            "# git.example.com:2222 SSH-2.0-OpenSSH_9.6\n\
             [git.example.com]:2222 ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQC7\n\
             [git.example.com]:2222 {ed25519}\n\
             git.example.com {ed25519}\n"
        );
        let (status, host) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/deploy-keys/host-keys",
            Some(&json!({"host": "git.example.com:2222", "knownHosts": given})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{host}");
        assert_eq!(
            host["hostKeys"],
            json!([{"algorithm": "ssh-ed25519",
                    "fingerprint": "SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU"}])
        );
        let changes = json!([git_service(
            "ssh://git@git.example.com:2222/acme/site.git",
            Some(&given)
        )]);
        plan_and_apply(&app, changes).await;
        assert_eq!(
            created_secret(&cluster)["stringData"]["known_hosts"],
            format!("[git.example.com]:2222 {ed25519}\n")
        );
        for refused in [
            json!({"host": "git.example.com:2222"}),
            json!({"host": "git.example.com:2222",
                   "knownHosts": format!("git.example.com {ed25519}\n")}),
            json!({"host": "git.example.com:2222",
                   "knownHosts": "[git.example.com]:2222 ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQC7\n"}),
        ] {
            let (status, problem) = send(
                &app,
                "POST",
                "/api/v1/projects/shop/deploy-keys/host-keys",
                Some(&refused),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{refused} {problem}");
        }
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
            &TestApi::new(&cluster).with_settings(settings).router(),
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

    /// A service whose folder nobody has uploaded waits for the upload; it is not building.
    #[tokio::test]
    async fn a_service_waiting_for_its_upload_is_not_building() {
        let cluster = cluster();
        waiting_service_with(
            &cluster,
            None,
            Some(
                &json!({"uploadToken": {"request": "u1", "secretName": "site-upload",
                    "expiresAt": "2026-10-23T08:15:00Z"},
                "conditions": [
                    {"type": "SourceReady", "status": "False", "reason": "WaitingForUpload"},
                    {"type": "Ready", "status": "False", "reason": "SourceNotReady"}]}),
            ),
        );
        cluster.update(|s| {
            let build = s.dynamic.last_mut().unwrap();
            build.data["spec"]["source"] = json!({"archive": {}});
        });
        let service = site(&cluster).await;
        assert_eq!(service["status"]["word"], "waitingUpload", "{service}");
        assert_eq!(service["diagnoses"], json!([]), "{service}");
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
                           "updatedReplicas": 1,
                           "conditions": [{"type": "Available", "status": "True"}]}
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
            let mut conditions =
                vec![json!({"type": "Ready", "status": "True", "reason": "Built"})];
            if let Some((reason, since)) = problem {
                conditions.push(
                    json!({"type": "SourceReady", "status": "False", "reason": reason,
                    "message": "github.com refused the key for `git`",
                    "lastTransitionTime": since}),
                );
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

    /// When the host refuses a key, the person can see its public half again, read through the
    /// `Build` from its Secret as them; a `Build` read with a token says so.
    #[tokio::test]
    async fn a_deploy_key_shows_its_public_half_again() {
        let cluster = cluster();
        let key = zeabur_core::ssh::deploy_key("zeabur shop").unwrap();
        cluster.update(|s| {
            s.secret_values.insert(
                "shop/site-git".to_owned(),
                BTreeMap::from([(
                    "ssh-privatekey".to_owned(),
                    key.private.clone().into_bytes(),
                )]),
            );
            s.secret_values.insert(
                "shop/blog-git".to_owned(),
                BTreeMap::from([("password".to_owned(), b"t".to_vec())]),
            );
            for (name, credentials) in [("site", "site-git"), ("blog", "blog-git"), ("docs", "")] {
                let mut build = json!({"apiVersion": "builder.zeabur.com/v1alpha1",
                    "kind": "Build",
                    "metadata": {"name": name, "namespace": "shop"},
                    "spec": {"builderRef": "dockerfile",
                             "source": {"git": {"url": "https://github.com/acme/site.git",
                                                "ref": "main"}},
                             "output": {"image": "zot.zot.svc:5000/shop/site"}}});
                if !credentials.is_empty() {
                    build["spec"]["source"]["credentialsRef"] = json!({"name": credentials});
                }
                s.dynamic.push(serde_json::from_value(build).unwrap());
            }
        });
        let app = app(&cluster);
        let reveal = |build: &str| format!("/api/v1/projects/shop/builds/{build}/deploy-key");
        let (status, answer) = send(&app, "POST", &reveal("site"), None).await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        assert!(
            key.public
                .starts_with(answer["publicKey"].as_str().unwrap())
        );
        assert!(!answer.to_string().contains("PRIVATE"));
        for build in ["blog", "docs"] {
            let (status, _) = send(&app, "POST", &reveal(build), None).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{build}");
        }
    }
}
