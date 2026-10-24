//! Plan, then apply, against an in-memory cluster: a plan writes nothing; an apply carries out
//! exactly the plan, dry runs first, and refuses a token that was altered, has expired, belongs
//! to someone else or no longer matches the objects; of many applies of one plan, one wins.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicI64, Ordering};

    use axum::Router;
    use axum::body::Body;
    use axum::http::request::Parts;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt as _;
    use serde_json::{Value, json};
    use tower::ServiceExt as _;
    use zeabur_api::access::{Access, ClusterAccess, Elevation, SingleCluster};
    use zeabur_api::plan_token::LIFETIME_SECONDS;
    use zeabur_core::problem::Problem;
    use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::k8s_openapi::jiff::Timestamp;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Cluster, Identity, NamespaceObjects, WriteBody};

    use crate::common::{app, send};

    fn deployment(name: &str, labels: &Value) -> Deployment {
        serde_json::from_value(json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {"name": name, "namespace": "shop", "resourceVersion": "41", "labels": labels},
            "spec": {
                "selector": {"matchLabels": {"app": name}},
                "template": {
                    "metadata": {"labels": {"app": name}},
                    "spec": {"containers": [{"name": "app", "image": "example/app:1",
                        "env": [{"name": "MODE", "value": "production"}]}]}
                }
            }
        }))
        .unwrap()
    }

    fn cluster() -> MemoryCluster {
        let namespace: Namespace = serde_json::from_value(json!({"metadata": {"name": "shop",
            "labels": {"zeabur.com/project": "true"}}}))
        .unwrap();
        MemoryCluster::new(State {
            identity: Identity {
                username: "alice".to_owned(),
                groups: vec!["developers".to_owned()],
            },
            namespaces: vec![namespace],
            objects: BTreeMap::from([(
                "shop".to_owned(),
                NamespaceObjects {
                    namespace: "shop".to_owned(),
                    deployments: vec![
                        deployment("web", &json!({"app": "web"})),
                        deployment("chart", &json!({"app.kubernetes.io/managed-by": "Helm"})),
                    ],
                    ..NamespaceObjects::default()
                },
            )]),
            ..State::default()
        })
    }

    async fn post(app: Router, uri: &str, body: &Value) -> (StatusCode, Value) {
        send(&app, "POST", uri, Some(body)).await
    }

    fn set_env(workload: &str) -> Value {
        json!({"changes": [{"op": "env", "workload": {"kind": "Deployment", "name": workload},
            "changes": [{"op": "set", "name": "MODE", "value": "maintenance"}]}]})
    }

    async fn plan(app: &Router, request: &Value) -> Value {
        let (status, body) = post(app.clone(), "/api/v1/projects/shop/plan", request).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    #[tokio::test]
    async fn a_plan_writes_nothing_and_its_apply_writes_exactly_the_plan() {
        let cluster = cluster();
        let app = app(&cluster);
        let planned = plan(&app, &set_env("web")).await;
        assert_eq!(
            cluster.inspect(|s| (s.writes.len(), s.dry_runs)),
            Some((0, 0))
        );
        let object = &planned["objects"][0];
        assert_eq!(object["target"]["name"], "web");
        assert_eq!(
            object["changes"][0]["path"],
            "spec.template.spec.containers[name=app].env[name=MODE].value"
        );
        assert_eq!(object["changes"][0]["before"], "production");
        assert_eq!(object["changes"][0]["after"], "maintenance");
        assert!(planned["expiresAt"].as_str().unwrap().ends_with('Z'));

        let (status, applied) = post(
            app.clone(),
            "/api/v1/projects/shop/apply",
            &json!({"token": planned["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert_eq!(applied["applied"][0]["resourceVersion"], "42");
        let (writes, dry_runs) = cluster.inspect(|s| (s.writes.clone(), s.dry_runs)).unwrap();
        assert_eq!(dry_runs, 1, "every write is dry-run first");
        assert_eq!(writes.len(), 1);
        let WriteBody::JsonPatch(operations) = &writes[0].body else {
            panic!("an env change is a JSON Patch");
        };
        assert_eq!(operations, &object["write"]["operations"]);
        assert_eq!(
            operations[0],
            json!({"op": "test", "path": "/metadata/resourceVersion", "value": "41"})
        );
    }

    #[tokio::test]
    async fn a_rebuild_reads_the_build_and_plans_its_annotation() {
        let cluster = cluster();
        let app = app(&cluster);
        let (status, problem) = post(
            app.clone(),
            "/api/v1/projects/shop/plan",
            &json!({"changes": [{"op": "rebuild", "build": "web"}]}),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
        cluster.update(|s| {
            s.served.push(zeabur_kube::ServedResource::readable(
                "builder.zeabur.com",
                "v1alpha1",
                "builds",
                "Build",
                true,
            ));
            s.dynamic.push(
                serde_json::from_value(json!({
                    "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "Build",
                    "metadata": {"name": "web", "namespace": "shop", "resourceVersion": "7"},
                    "spec": {"builderRef": "dockerfile",
                        "source": {"git": {"url": "https://example.com/acme/web", "ref": "main"}}}
                }))
                .unwrap(),
            );
        });
        let planned = plan(
            &app,
            &json!({"changes": [{"op": "rebuild", "build": "web"}]}),
        )
        .await;
        let object = &planned["objects"][0];
        assert_eq!(object["target"]["kind"], "Build");
        assert_eq!(
            object["changes"][0]["path"],
            "metadata.annotations.builder.zeabur.com/rebuild"
        );
    }

    #[tokio::test]
    async fn a_ready_made_network_policy_is_created_once() {
        let cluster = cluster();
        let app = app(&cluster);
        let policy = json!({
            "apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
            "metadata": {"name": "zeabur-isolate-web"},
            "spec": {"podSelector": {"matchLabels": {"app": "web"}}, "policyTypes": ["Ingress"]}
        });
        let request = json!({"changes": [{"op": "create", "object": policy}]});
        let planned = plan(&app, &request).await;
        let (status, applied) = post(
            app.clone(),
            "/api/v1/projects/shop/apply",
            &json!({"token": planned["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        let WriteBody::Create(body) = &writes[0].body else {
            panic!("a creation is a POST");
        };
        assert_eq!(body["metadata"]["namespace"], "shop");

        // Once it exists, the same creation is refused rather than applied over it.
        cluster.update(|s| {
            s.dynamic.push(
                serde_json::from_value(json!({
                    "apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
                    "metadata": {"name": "zeabur-isolate-web", "namespace": "shop"},
                    "spec": {"podSelector": {}}
                }))
                .unwrap(),
            );
        });
        let (status, problem) = post(app, "/api/v1/projects/shop/plan", &request).await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
    }

    #[tokio::test]
    async fn a_page_server_is_planned_and_applied_from_the_vendored_template() {
        let cluster = cluster();
        let app = app(&cluster);
        let request = json!({"changes": [{"op": "pageServer", "maintenance": "<p>Back soon</p>"}]});
        let planned = plan(&app, &request).await;
        let (status, applied) = post(
            app.clone(),
            "/api/v1/projects/shop/apply",
            &json!({"token": planned["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        let created: Vec<&Value> = writes
            .iter()
            .map(|w| {
                let WriteBody::Create(body) = &w.body else {
                    panic!("not a creation: {:?}", w.body);
                };
                body
            })
            .collect();
        let named = |kind: &str, name: &str| {
            created
                .iter()
                .copied()
                .find(|o| o["kind"] == kind && o["metadata"]["name"] == name)
        };
        assert!(named("NetworkPolicy", "default-deny").is_none());
        assert!(named("NetworkPolicy", "error-pages").is_some());
        assert!(named("Deployment", "error-pages").is_some());
        let service = named("Service", "error-pages").unwrap();
        assert_eq!(
            service["metadata"]["labels"]["zeabur.com/page-server"],
            "true"
        );
        let pages = named("ConfigMap", "error-pages").unwrap();
        assert_eq!(pages["data"]["maintenance.html"], "<p>Back soon</p>");
        assert_eq!(pages["metadata"]["namespace"], "shop");
    }

    /// Sets the generation of the Deployment `name`, and its `resourceVersion` to `version`.
    fn set_meta(cluster: &MemoryCluster, name: &str, generation: i64, version: &str) {
        cluster.update(|s| {
            let deployment = s
                .objects
                .get_mut("shop")
                .and_then(|o| {
                    o.deployments
                        .iter_mut()
                        .find(|d| d.metadata.name.as_deref() == Some(name))
                })
                .unwrap();
            deployment.metadata.generation = Some(generation);
            deployment.metadata.resource_version = Some(version.to_owned());
        });
    }

    #[tokio::test]
    async fn a_status_write_after_the_plan_does_not_make_it_stale() {
        let cluster = cluster();
        set_meta(&cluster, "web", 3, "41");
        let app = app(&cluster);
        let planned = plan(&app, &set_env("web")).await;
        let operations = &planned["objects"][0]["write"]["operations"];
        assert_eq!(
            operations[0],
            json!({"op": "test", "path": "/metadata/generation", "value": 3})
        );
        // The rollout controller writes the status: resourceVersion moves, generation does not.
        set_meta(&cluster, "web", 3, "57");
        let (status, applied) = post(
            app,
            "/api/v1/projects/shop/apply",
            &json!({"token": planned["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
    }

    #[tokio::test]
    async fn a_spec_change_after_the_plan_makes_it_stale() {
        let cluster = cluster();
        set_meta(&cluster, "web", 3, "41");
        let app = app(&cluster);
        let planned = plan(&app, &set_env("web")).await;
        // Someone else edits the spec: the generation moves.
        set_meta(&cluster, "web", 4, "60");
        let (status, problem) = post(
            app,
            "/api/v1/projects/shop/apply",
            &json!({"token": planned["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
    }

    #[tokio::test]
    async fn an_altered_token_is_refused() {
        let cluster = cluster();
        let app = app(&cluster);
        let planned = plan(&app, &set_env("web")).await;
        let token = planned["token"].as_str().unwrap();
        let mut altered = token.to_owned();
        let last = altered.pop().unwrap();
        altered.push(if last == 'A' { 'B' } else { 'A' });
        let (status, problem) = post(
            app.clone(),
            "/api/v1/projects/shop/apply",
            &json!({"token": altered}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(problem["kind"], "bad-request");
        // A token made by another server (another key) is refused the same way.
        let (status, _) = post(
            crate::common::app(&cluster),
            "/api/v1/projects/shop/apply",
            &json!({"token": token}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        // A token for another project is refused.
        let (status, _) = post(
            app.clone(),
            "/api/v1/projects/other/apply",
            &json!({"token": token}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(cluster.inspect(|s| s.writes.len()), Some(0));
    }

    /// Given a plan made at a fixed time, when it is applied once its lifetime has passed, then
    /// it is refused with the problem the interface shows as an expired review (`failureOf` in
    /// web/src/components/plan/describe.ts: kind `plan-expired`) and nothing is written; a
    /// moment earlier the same token applies.
    #[tokio::test]
    async fn a_plan_applied_after_its_lifetime_is_refused_as_expired() {
        static NOW: AtomicI64 = AtomicI64::new(1_792_022_400); // 2026-10-15T00:00:00Z
        fn clock() -> Timestamp {
            Timestamp::from_second(NOW.load(Ordering::SeqCst)).unwrap()
        }
        let cluster = cluster();
        let app = zeabur_api::router_at(Arc::new(SingleCluster(Arc::new(cluster.clone()))), clock);
        let planned = plan(&app, &set_env("web")).await;
        assert_eq!(planned["expiresAt"], "2026-10-15T00:10:00Z");
        let token = json!({"token": planned["token"]});

        NOW.fetch_add(LIFETIME_SECONDS, Ordering::SeqCst);
        let (status, problem) = post(app.clone(), "/api/v1/projects/shop/apply", &token).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{problem}");
        assert_eq!(problem["kind"], "plan-expired");
        assert_eq!(problem["type"], "/problems/plan-expired");
        assert_eq!(
            cluster.inspect(|s| (s.writes.len(), s.dry_runs)),
            Some((0, 0))
        );

        NOW.fetch_sub(1, Ordering::SeqCst);
        let (status, applied) = post(app, "/api/v1/projects/shop/apply", &token).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
    }

    /// The refusal is Zeabur's own, so it stays `forbidden` although admin rights are offered:
    /// they would not change who made the plan.
    #[tokio::test]
    async fn a_plan_is_applied_only_by_the_person_who_made_it() {
        let cluster = cluster();
        let app = elevating(&cluster, None);
        let planned = plan(&app, &set_env("web")).await;
        cluster.update(|s| s.identity.username = "mallory".to_owned());
        let (status, problem) = post(
            app.clone(),
            "/api/v1/projects/shop/apply",
            &json!({"token": planned["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(problem["type"], "/problems/forbidden");
        assert_eq!(cluster.inspect(|s| s.writes.len()), Some(0));
    }

    #[tokio::test]
    async fn an_object_changed_after_the_plan_is_not_overwritten() {
        let cluster = cluster();
        let app = app(&cluster);
        let planned = plan(&app, &set_env("web")).await;
        cluster.update(|s| {
            let objects = s.objects.get_mut("shop").unwrap();
            objects.deployments[0].metadata.resource_version = Some("50".to_owned());
        });
        let (status, problem) = post(
            app.clone(),
            "/api/v1/projects/shop/apply",
            &json!({"token": planned["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
        assert_eq!(cluster.inspect(|s| s.writes.len()), Some(0));
    }

    #[tokio::test]
    async fn of_sixty_four_concurrent_applies_of_one_plan_exactly_one_succeeds() {
        let cluster = cluster();
        let app = app(&cluster);
        let planned = plan(&app, &set_env("web")).await;
        let body = json!({"token": planned["token"]});
        let applies = (0..64).map(|_| {
            let app = app.clone();
            let body = body.clone();
            tokio::spawn(async move { post(app, "/api/v1/projects/shop/apply", &body).await.0 })
        });
        let mut statuses = Vec::new();
        for apply in applies {
            statuses.push(apply.await.unwrap());
        }
        assert_eq!(
            statuses.iter().filter(|s| **s == StatusCode::OK).count(),
            1,
            "{statuses:?}"
        );
        assert!(
            statuses
                .iter()
                .all(|s| *s == StatusCode::OK || *s == StatusCode::CONFLICT),
            "{statuses:?}"
        );
        assert_eq!(cluster.inspect(|s| s.writes.len()), Some(1));
    }

    #[tokio::test]
    async fn an_edit_of_a_helm_object_is_refused_but_a_restart_is_planned() {
        let cluster = cluster();
        let app = app(&cluster);
        let (status, problem) =
            post(app.clone(), "/api/v1/projects/shop/plan", &set_env("chart")).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(problem["managedBy"], "Helm", "{problem}");
        assert_eq!(problem["type"], "/problems/managed");
        assert_eq!(problem["title"], "Another tool manages it");
        assert_eq!(problem["kind"], "managed");
        let restart = json!({"changes": [{"op": "restart", "workload": {"kind": "Deployment", "name": "chart"}}]});
        let planned = plan(&app, &restart).await;
        // One write method: a restart is a JSON Patch, its first operation testing the
        // resourceVersion read.
        let write = &planned["objects"][0]["write"];
        assert_eq!(write["type"], "patch");
        let operations = write["operations"].as_array().unwrap();
        assert_eq!(operations[0]["op"], "test");
        assert_eq!(operations[0]["path"], "/metadata/resourceVersion");
        assert!(
            operations.iter().any(|op| op
                .to_string()
                .contains("kubectl.kubernetes.io~1restartedAt")
                || op.to_string().contains("kubectl.kubernetes.io/restartedAt")),
            "{write}"
        );
    }

    /// Given a plan, when Argo CD takes over the field it changes before the apply, then the
    /// apply is refused with `409` and `managedBy: "Argo CD"`, which the interface reads instead
    /// of the wording of `detail`, and nothing is written.
    #[tokio::test]
    async fn an_apply_refused_for_a_managing_tool_names_the_tool() {
        let cluster = cluster();
        let app = app(&cluster);
        let planned = plan(&app, &set_env("web")).await;
        cluster.update(|s| {
            let web = s
                .objects
                .get_mut("shop")
                .and_then(|o| {
                    o.deployments
                        .iter_mut()
                        .find(|d| d.metadata.name.as_deref() == Some("web"))
                })
                .unwrap();
            web.metadata.managed_fields = serde_json::from_value(json!([{
                "manager": "argocd-controller", "operation": "Apply", "apiVersion": "apps/v1",
                "fieldsType": "FieldsV1", "fieldsV1": {"f:spec": {"f:template": {"f:spec": {
                    "f:containers": {"k:{\"name\":\"app\"}": {"f:env": {
                        "k:{\"name\":\"MODE\"}": {".": {}, "f:value": {}}}}}}}}}
            }]))
            .unwrap();
        });
        let (status, problem) = post(
            app.clone(),
            "/api/v1/projects/shop/apply",
            &json!({"token": planned["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
        assert_eq!(problem["managedBy"], "Argo CD", "{problem}");
        assert_eq!(problem["type"], "/problems/managed");
        assert_eq!(problem["title"], "Another tool manages it");
        assert_eq!(cluster.inspect(|s| s.writes.len()), Some(0));
    }

    #[tokio::test]
    async fn a_refused_write_writes_nothing_and_keeps_kubernetes_answer() {
        let cluster = cluster();
        let app = app(&cluster);
        let planned = plan(&app, &set_env("web")).await;
        cluster.update(|s| {
            s.forbidden_writes.insert("Deployment/web".to_owned());
        });
        let (status, problem) = post(
            app.clone(),
            "/api/v1/projects/shop/apply",
            &json!({"token": planned["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(problem["kubernetes"]["code"], 403);
        // The refused request, from what Zeabur sent (`failureOf` names the permission from it).
        assert_eq!(
            problem["denied"],
            json!({"verb": "patch", "group": "apps", "resource": "deployments",
                   "name": "web", "namespace": "shop"})
        );
        assert_eq!(cluster.inspect(|s| s.writes.len()), Some(0));
    }

    /// Everything as `elevated` while it is set, else as `base`, offering elevation when
    /// `offered`; a plan's binding always as `base`.
    struct Elevating {
        base: MemoryCluster,
        elevated: Option<MemoryCluster>,
        offered: bool,
    }

    impl ClusterAccess for Elevating {
        fn everyday(&self, _parts: &Parts) -> Result<Arc<dyn Cluster>, Box<Problem>> {
            Ok(Arc::new(self.base.clone()))
        }

        fn for_request(&self, _parts: &Parts) -> Result<Access, Box<Problem>> {
            let everyday = || Access {
                cluster: Arc::new(self.base.clone()),
                elevation: if self.offered {
                    Elevation::Available
                } else {
                    Elevation::Unavailable
                },
            };
            Ok(self
                .elevated
                .as_ref()
                .map_or_else(everyday, |elevated| Access {
                    cluster: Arc::new(elevated.clone()),
                    elevation: Elevation::Active {
                        until: Timestamp::MAX,
                    },
                }))
        }
    }

    /// One key for every router, as on a server that reads it from a file, so a plan made
    /// through one applies through another.
    fn elevating(base: &MemoryCluster, elevated: Option<&MemoryCluster>) -> Router {
        zeabur_api::router_with_key(
            Arc::new(Elevating {
                base: base.clone(),
                elevated: elevated.cloned(),
                offered: true,
            }),
            zeabur_api::plan_token::PlanKey::from_bytes([7; 32]),
            zeabur_api::Settings::default(),
            Arc::new(zeabur_api::account::Fixed(None)),
            None,
        )
    }

    /// Given elevation is offered and not in use, when the cluster refuses a write, then the
    /// answer is `elevation-required`, still a `403` with kube-apiserver's answer and the refused
    /// request; nothing is written.
    #[tokio::test]
    async fn a_refused_write_offers_elevation_where_it_is_available() {
        let cluster = cluster();
        cluster.update(|s| {
            s.forbidden_writes.insert("Deployment/web".to_owned());
        });
        let app = elevating(&cluster, None);
        let planned = plan(&app, &set_env("web")).await;
        let (status, problem) = post(
            app.clone(),
            "/api/v1/projects/shop/apply",
            &json!({"token": planned["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(problem["type"], "/problems/elevation-required");
        assert_eq!(problem["kind"], "elevation-required");
        assert_eq!(problem["fixKey"]["key"], "problem.forbidden.fix.askOrRetry");
        assert_eq!(problem["kubernetes"]["code"], 403);
        assert_eq!(problem["denied"]["verb"], "patch");
        assert_eq!(cluster.inspect(|s| s.writes.len()), Some(0));

        // Where it is not offered, the same refusal stays a plain `forbidden`.
        let plain = zeabur_api::router(Arc::new(Elevating {
            base: cluster.clone(),
            elevated: None,
            offered: false,
        }));
        let planned = plan(&plain, &set_env("web")).await;
        let (_, problem) = post(
            plain,
            "/api/v1/projects/shop/apply",
            &json!({"token": planned["token"]}),
        )
        .await;
        assert_eq!(problem["type"], "/problems/forbidden");
    }

    /// Given elevation is offered, when admission refuses a write the person's rights allow
    /// (a quota's `403`), then the answer is `quota-exceeded` with the quota's limit and use read
    /// from its status, and admin rights are not offered.
    #[tokio::test]
    async fn a_write_over_a_quota_is_quota_exceeded_with_the_quotas_numbers() {
        let cluster = cluster();
        cluster.update(|s| {
            s.admission_refusals.insert(
                "Deployment/web".to_owned(),
                zeabur_core::problem::KubernetesStatus::new(
                    403,
                    "Forbidden",
                    "deployments.apps \"web\" is forbidden: exceeded quota: compute, requested: \
                     count/deployments.apps=1, used: count/deployments.apps=2, limited: \
                     count/deployments.apps=2",
                ),
            );
            s.resource_quotas.insert(
                "shop".to_owned(),
                vec![
                    serde_json::from_value(json!({
                        "metadata": {"name": "compute", "namespace": "shop"},
                        "spec": {"hard": {"count/deployments.apps": "2", "requests.cpu": "4"}},
                        "status": {"hard": {"count/deployments.apps": "2", "requests.cpu": "4"},
                                   "used": {"count/deployments.apps": "2", "requests.cpu": "3500m"}}
                    }))
                    .unwrap(),
                ],
            );
        });
        let app = elevating(&cluster, None);
        let planned = plan(&app, &set_env("web")).await;
        let (status, problem) = post(
            app,
            "/api/v1/projects/shop/apply",
            &json!({"token": planned["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(problem["kind"], "quota-exceeded");
        assert_eq!(problem["type"], "/problems/quota-exceeded");
        assert_eq!(
            problem["why"],
            json!({"key": "problem.why.quota-exceeded", "params": {
                "quota": "compute", "resource": "count/deployments.apps",
                "hard": "2", "used": "2"}})
        );
        assert_eq!(problem["summary"]["key"], "problem.summary.quota-exceeded");
        assert_eq!(problem["summary"]["params"]["name"], "web");
        assert_eq!(
            problem["evidence"],
            json!([
                {"apiVersion": "v1", "kind": "ResourceQuota", "namespace": "shop", "name": "compute",
                 "fieldPath": "status.hard.count/deployments.apps", "value": "2"},
                {"apiVersion": "v1", "kind": "ResourceQuota", "namespace": "shop", "name": "compute",
                 "fieldPath": "status.used.count/deployments.apps", "value": "2"}
            ])
        );
        // A fix written for the occasion, with its key for other languages.
        assert_eq!(
            problem["fixKey"],
            json!({"key": "problem.quotaExceeded.fix.scale",
                   "params": {"quota": "compute", "object": "web"}})
        );
        assert_eq!(problem["kubernetes"]["code"], 403);
        assert_eq!(cluster.inspect(|s| s.writes.len()), Some(0));
    }

    /// When a webhook denies a write with `400` (it gave no status code), the answer is
    /// `policy-denied` quoting the webhook, not a malformed request; with a `403` from a webhook
    /// for a request the rights allow, the same.
    #[tokio::test]
    async fn a_webhooks_denial_is_policy_denied_in_the_webhooks_words() {
        let words = "images must come from registry.example.com";
        for code in [400, 403] {
            let cluster = cluster();
            cluster.update(|s| {
                let reason = if code == 400 {
                    "BadRequest"
                } else {
                    "Forbidden"
                };
                s.admission_refusals.insert(
                    "Deployment/web".to_owned(),
                    zeabur_core::problem::KubernetesStatus::new(
                        code,
                        reason,
                        format!(
                            "admission webhook \"images.example.com\" denied the request: {words}"
                        ),
                    ),
                );
            });
            let app = elevating(&cluster, None);
            let planned = plan(&app, &set_env("web")).await;
            let (status, problem) = post(
                app,
                "/api/v1/projects/shop/apply",
                &json!({"token": planned["token"]}),
            )
            .await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
            assert_eq!(problem["kind"], "policy-denied");
            assert_eq!(
                problem["why"],
                json!({"key": "problem.why.policy-denied",
                       "params": {"policy": "images.example.com", "message": words}})
            );
            assert_eq!(problem["kubernetes"]["code"], code);
            assert_eq!(problem["fixKey"]["key"], "problem.policyDenied.fix.change");
        }
    }

    /// A refused read is offered elevation as a refused write is, from the one place every
    /// answer passes: only where elevation is offered and not in use.
    #[tokio::test]
    async fn a_refused_read_offers_elevation_only_where_it_would_help() {
        let refusing = cluster();
        refusing.update(|s| {
            s.forbidden_resources.insert("nodes".to_owned());
        });
        let offered = |elevated: Option<&MemoryCluster>, offered: bool| {
            zeabur_api::router(Arc::new(Elevating {
                base: refusing.clone(),
                elevated: elevated.cloned(),
                offered,
            }))
        };
        for (app, expected) in [
            (offered(None, true), "elevation-required"),
            (offered(None, false), "forbidden"),
            (offered(Some(&refusing), true), "forbidden"),
        ] {
            let request = Request::get("/api/v1/cluster/nodes")
                .body(Body::empty())
                .unwrap();
            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let problem: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(problem["kind"], expected);
            assert_eq!(problem["kubernetes"]["code"], 403);
            assert_eq!(problem["denied"]["resource"], "nodes");
        }
        // With the rights in use, the read goes with them.
        let request = Request::get("/api/v1/cluster/nodes")
            .body(Body::empty())
            .unwrap();
        let response = offered(Some(&cluster()), true)
            .oneshot(request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    /// Given a plan made with the everyday credential, when the person elevates (same user name,
    /// more groups) and applies it, then the plan still applies, its writes go through the
    /// elevated credential, and none through the everyday one. When the elevated credential is
    /// refused too, the answer is a plain `forbidden`: retrying as an administrator again would
    /// not help.
    #[tokio::test]
    async fn an_elevated_person_applies_a_plan_made_before_with_the_elevated_credential() {
        let base = cluster();
        base.update(|s| {
            s.forbidden_writes.insert("Deployment/web".to_owned());
        });
        let planned = plan(&elevating(&base, None), &set_env("web")).await;

        let elevated = cluster();
        elevated.update(|s| s.identity.groups.push("elevated:admins".to_owned()));
        let (status, body) = post(
            elevating(&base, Some(&elevated)),
            "/api/v1/projects/shop/apply",
            &json!({"token": planned["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(base.inspect(|s| s.writes.len()), Some(0));
        assert_eq!(elevated.inspect(|s| s.writes.len()), Some(1));

        let refused = cluster();
        refused.update(|s| {
            s.forbidden_writes.insert("Deployment/web".to_owned());
        });
        let (status, problem) = post(
            elevating(&base, Some(&refused)),
            "/api/v1/projects/shop/apply",
            &json!({"token": planned["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(problem["type"], "/problems/forbidden");
    }

    fn delete(resource: &Value, name: &str, version: Option<&Value>) -> Value {
        let mut change = json!({"op": "delete", "resource": resource, "name": name});
        if let Some(version) = version {
            change["resourceVersion"] = version.clone();
        }
        json!({"changes": [change]})
    }

    fn deployments() -> Value {
        json!({"group": "apps", "version": "v1", "resource": "deployments"})
    }

    /// Given a ready-made policy just created, when its Undo plans the deletion at the
    /// `resourceVersion` the creation returned, then the plan says which service it no longer
    /// isolates, and the apply sends a `DELETE` whose preconditions are the policy's `uid` and
    /// that version, dry run first; the policy is gone.
    /// Given a policy that isolates web and admits only the project's pods, when a port of web is
    /// opened to the internet, then the plan warns that the policy keeps the port from answering.
    #[tokio::test]
    async fn a_public_port_behind_an_isolating_policy_is_warned_about() {
        let cluster = cluster();
        cluster.update(|s| {
            s.served.push(zeabur_kube::ServedResource::readable(
                "networking.k8s.io",
                "v1",
                "networkpolicies",
                "NetworkPolicy",
                true,
            ));
        });
        let app = app(&cluster);
        let objects = [
            json!({"apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
                "metadata": {"name": "web-isolate"},
                "spec": {"podSelector": {"matchLabels": {"app": "web"}},
                    "ingress": [{"from": [{"podSelector": {}}]}]}}),
            json!({"apiVersion": "v1", "kind": "Service", "metadata": {"name": "web"},
                "spec": {"selector": {"app": "web"}, "ports": [{"port": 8080}]}}),
        ];
        for object in objects {
            let planned = plan(
                &app,
                &json!({"changes": [{"op": "create", "object": object}]}),
            )
            .await;
            let (status, applied) = post(
                app.clone(),
                "/api/v1/projects/shop/apply",
                &json!({"token": planned["token"]}),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{applied}");
        }
        let planned = plan(
            &app,
            &json!({"changes": [{"op": "publicPort",
                "workload": {"kind": "Deployment", "name": "web"}, "service": "web", "port": 8080}]}),
        )
        .await;
        assert_eq!(
            planned["warnings"],
            json!([
                {"code": "publicPort", "workload": {"kind": "Deployment", "name": "web"}, "port": 8080},
                {"code": "publicPortBlocked", "workload": {"kind": "Deployment", "name": "web"},
                    "port": 8080, "policies": ["web-isolate"]}
            ]),
            "{planned}"
        );
    }

    #[tokio::test]
    async fn a_created_policy_is_undone_by_deleting_the_version_it_was_created_at() {
        let cluster = cluster();
        cluster.update(|s| {
            s.served.push(zeabur_kube::ServedResource::readable(
                "networking.k8s.io",
                "v1",
                "networkpolicies",
                "NetworkPolicy",
                true,
            ));
        });
        let app = app(&cluster);
        let policy = json!({
            "apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
            "metadata": {"name": "zeabur-isolate-web"},
            "spec": {"podSelector": {"matchLabels": {"app": "web"}}, "policyTypes": ["Ingress"]}
        });
        let planned = plan(
            &app,
            &json!({"changes": [{"op": "create", "object": policy}]}),
        )
        .await;
        let (status, applied) = post(
            app.clone(),
            "/api/v1/projects/shop/apply",
            &json!({"token": planned["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let version = applied["applied"][0]["resourceVersion"].clone();

        let policies =
            json!({"group": "networking.k8s.io", "version": "v1", "resource": "networkpolicies"});
        let undo = delete(&policies, "zeabur-isolate-web", Some(&version));
        let planned = plan(&app, &undo).await;
        let object = &planned["objects"][0];
        assert_eq!(object["write"]["type"], "delete", "{planned}");
        assert_eq!(object["write"]["resourceVersion"], version);
        assert!(object.get("after").is_none(), "{object}");
        assert_eq!(
            planned["warnings"],
            json!([{"code": "networkPolicyRemoved", "policy": "zeabur-isolate-web",
                "ingressOpened": [{"kind": "Deployment", "name": "web"}],
                "egressOpened": [], "connectionsOpened": []}])
        );
        let (status, applied) = post(
            app.clone(),
            "/api/v1/projects/shop/apply",
            &json!({"token": planned["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let (writes, dry_runs) = cluster.inspect(|s| (s.writes.clone(), s.dry_runs)).unwrap();
        assert_eq!(
            dry_runs, 2,
            "the creation and the deletion are each dry-run first"
        );
        let WriteBody::Delete(preconditions) = &writes[1].body else {
            panic!("an undo of a creation is a deletion: {writes:?}");
        };
        assert_eq!(
            preconditions.resource_version.as_deref(),
            version.as_str(),
            "{preconditions:?}"
        );
        assert!(preconditions.uid.is_some(), "{preconditions:?}");
        assert_eq!(cluster.inspect(|s| s.dynamic.len()), Some(0));

        // Undone again: the policy is gone.
        let (status, problem) = post(app, "/api/v1/projects/shop/plan", &undo).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    }

    /// Given a deletion planned against a Deployment, when the Deployment changes before the
    /// apply, then the apply is refused with `409` and nothing is deleted; a deletion that names
    /// a version the object has moved past is refused when planned.
    #[tokio::test]
    async fn a_deletion_of_an_object_changed_after_the_plan_is_a_conflict() {
        let cluster = cluster();
        let app = app(&cluster);
        let planned = plan(&app, &delete(&deployments(), "web", None)).await;
        assert_eq!(planned["objects"][0]["write"]["resourceVersion"], "41");
        set_meta(&cluster, "web", 1, "50");
        let (status, problem) = post(
            app.clone(),
            "/api/v1/projects/shop/apply",
            &json!({"token": planned["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
        assert_eq!(cluster.inspect(|s| s.writes.len()), Some(0));

        let (status, problem) = post(
            app,
            "/api/v1/projects/shop/plan",
            &delete(&deployments(), "web", Some(&json!("41"))),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
    }

    #[tokio::test]
    async fn a_refused_deletion_names_the_delete_that_was_refused() {
        let cluster = cluster();
        let app = app(&cluster);
        let planned = plan(&app, &delete(&deployments(), "web", None)).await;
        cluster.update(|s| {
            s.forbidden_writes.insert("Deployment/web".to_owned());
        });
        let (status, problem) = post(
            app.clone(),
            "/api/v1/projects/shop/apply",
            &json!({"token": planned["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(
            problem["denied"],
            json!({"verb": "delete", "group": "apps", "resource": "deployments",
                   "name": "web", "namespace": "shop"})
        );
        assert_eq!(cluster.inspect(|s| s.writes.len()), Some(0));
    }

    fn with_mongodb(cluster: &MemoryCluster, finalizers: &Value) {
        cluster.update(|s| {
            s.served.push(zeabur_kube::ServedResource::readable(
                "psmdb.percona.com",
                "v1",
                "perconaservermongodbs",
                "PerconaServerMongoDB",
                true,
            ));
            s.dynamic.push(
                serde_json::from_value(json!({
                    "apiVersion": "psmdb.percona.com/v1", "kind": "PerconaServerMongoDB",
                    "metadata": {"name": "mongo", "namespace": "shop", "uid": "u-mongo",
                        "resourceVersion": "9", "finalizers": finalizers},
                    "spec": {"replsets": [{"name": "rs0", "size": 2}],
                        "sharding": {"enabled": false}}
                }))
                .unwrap(),
            );
            let objects = s.objects.get_mut("shop").unwrap();
            for (i, name) in [
                "mongod-data-mongo-rs0-0",
                "mongod-data-mongo-rs0-1",
                "mongod-data-mongo-rs0-extra-0",
                "mongod-data-mongo2-rs0-0",
                "data",
            ]
            .into_iter()
            .enumerate()
            {
                objects.persistent_volume_claims.push(
                    serde_json::from_value(json!({
                        "apiVersion": "v1", "kind": "PersistentVolumeClaim",
                        "metadata": {"name": name, "namespace": "shop",
                            "uid": format!("u-{name}"), "resourceVersion": format!("{}", 20 + i)},
                        "spec": {"accessModes": ["ReadWriteOnce"],
                            "resources": {"requests": {"storage": "1Gi"}}}
                    }))
                    .unwrap(),
                );
            }
        });
    }

    fn mongodbs() -> Value {
        json!({"group": "psmdb.percona.com", "version": "v1", "resource": "perconaservermongodbs"})
    }

    fn claims_left(cluster: &MemoryCluster) -> Vec<String> {
        cluster
            .inspect(|s| {
                s.objects["shop"]
                    .persistent_volume_claims
                    .iter()
                    .filter_map(|c| c.metadata.name.clone())
                    .collect()
            })
            .unwrap()
    }

    /// Given a MongoDB made before Zeabur set Percona's `percona.com/delete-psmdb-pvc`
    /// finalizer, when it is deleted, then the plan deletes its data claims after it, and the
    /// apply leaves no `mongod-data-mongo-*` claim and every other claim as it was.
    #[tokio::test]
    async fn a_mongodb_without_the_claims_finalizer_is_deleted_with_its_data_claims() {
        let cluster = cluster();
        with_mongodb(&cluster, &json!([]));
        let app = app(&cluster);
        let planned = plan(&app, &delete(&mongodbs(), "mongo", None)).await;
        let deleted: Vec<(&str, &str)> = planned["objects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| {
                assert_eq!(o["write"]["type"], "delete", "{o}");
                (
                    o["target"]["kind"].as_str().unwrap(),
                    o["target"]["name"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            deleted,
            [
                ("PerconaServerMongoDB", "mongo"),
                ("PersistentVolumeClaim", "mongod-data-mongo-rs0-0"),
                ("PersistentVolumeClaim", "mongod-data-mongo-rs0-1"),
            ]
        );
        let lost: Vec<&Value> = planned["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|w| w["code"] == "claimDataMayBeLost")
            .map(|w| &w["claim"])
            .collect();
        assert_eq!(
            lost,
            [
                &json!("mongod-data-mongo-rs0-0"),
                &json!("mongod-data-mongo-rs0-1")
            ]
        );

        let (status, applied) = post(
            app,
            "/api/v1/projects/shop/apply",
            &json!({"token": planned["token"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert_eq!(
            claims_left(&cluster),
            [
                "mongod-data-mongo-rs0-extra-0",
                "mongod-data-mongo2-rs0-0",
                "data"
            ]
        );
    }

    /// Given a MongoDB with the finalizer, when it is deleted, then the plan deletes only the
    /// MongoDB and says its data claims go with it: the operator deletes them.
    #[tokio::test]
    async fn a_mongodb_with_the_claims_finalizer_takes_its_data_claims_with_it() {
        let cluster = cluster();
        with_mongodb(&cluster, &json!(["percona.com/delete-psmdb-pvc"]));
        let app = app(&cluster);
        let planned = plan(&app, &delete(&mongodbs(), "mongo", None)).await;
        assert_eq!(planned["objects"].as_array().unwrap().len(), 1, "{planned}");
        assert_eq!(
            planned["warnings"],
            json!([{"code": "deletesDependents",
                "target": {"kind": "PerconaServerMongoDB", "name": "mongo"},
                "dependents": [
                    {"kind": "PersistentVolumeClaim", "name": "mongod-data-mongo-rs0-0"},
                    {"kind": "PersistentVolumeClaim", "name": "mongod-data-mongo-rs0-1"}]}])
        );
    }

    #[tokio::test]
    async fn a_deletion_of_a_helm_object_is_refused_naming_helm() {
        let cluster = cluster();
        let app = app(&cluster);
        let (status, problem) = post(
            app,
            "/api/v1/projects/shop/plan",
            &delete(&deployments(), "chart", None),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
        assert_eq!(problem["managedBy"], "Helm", "{problem}");
        assert_eq!(problem["kind"], "managed");
    }
}
