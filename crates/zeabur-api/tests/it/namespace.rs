//! Creating a project, adopting a namespace, letting it go and a project's details through the
//! API, against an
//! in-memory cluster: the write is one patch of the Namespace, cluster-scoped; the list shows the
//! result; a system namespace and a refused patch get typed problems.

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_api::Settings;
    use zeabur_api::settings::IdentitySettings;
    use zeabur_core::names::MANAGE_VERB;
    use zeabur_core::problem::KubernetesStatus;
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{AccessRule, AccessRules, Identity, WriteBody};

    use crate::common::{app, plan_and_apply, send};
    use crate::support::TestApi;

    fn namespace(name: &str, labels: &Value) -> Namespace {
        serde_json::from_value(json!({"apiVersion": "v1", "kind": "Namespace",
            "metadata": {"name": name, "resourceVersion": "5", "labels": labels}}))
        .unwrap()
    }

    fn cluster() -> MemoryCluster {
        MemoryCluster::new(State {
            identity: Identity {
                username: "alice".to_owned(),
                groups: vec!["developers".to_owned()],
            },
            namespaces: vec![
                namespace("kube-system", &json!({})),
                namespace("shop", &json!({"team": "web"})),
            ],
            ..State::default()
        })
    }

    async fn summary(app: &Router, name: &str) -> Value {
        let (status, list) = send(app, "GET", "/api/v1/projects", None).await;
        assert_eq!(status, StatusCode::OK, "{list}");
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == name)
            .cloned()
            .unwrap()
    }

    #[tokio::test]
    async fn adopting_a_namespace_with_empty_labels_previews_no_removal() {
        let cluster = MemoryCluster::new(State {
            namespaces: vec![namespace("shop", &json!({}))],
            ..State::default()
        });
        let app = app(&cluster);
        let (status, plan) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/plan",
            Some(&json!({"changes": [{"op": "adopt"}]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        assert_eq!(plan["preview"]["removals"], json!([]), "{plan}");
        assert_eq!(
            plan["objects"][0]["changes"],
            json!([{"path": "metadata.labels.zeabur.com/project", "after": "true"}])
        );
    }

    #[tokio::test]
    async fn adopting_then_letting_go_patches_only_the_label_of_the_namespace() {
        let cluster = cluster();
        let app = app(&cluster);
        assert_eq!(summary(&app, "shop").await["adopted"], false);

        let (status, applied) = plan_and_apply(&app, "shop", json!([{"op": "adopt"}])).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert_eq!(applied["applied"][0]["target"]["kind"], "Namespace");
        let shop = summary(&app, "shop").await;
        assert_eq!(shop["adopted"], true);
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        assert_eq!(writes.len(), 1);
        // Cluster-scoped, and the patch adds the one label after pinning the resourceVersion.
        assert_eq!(writes[0].target.namespace, "");
        assert_eq!(
            writes[0].body,
            WriteBody::JsonPatch(json!([
                {"op": "test", "path": "/metadata/resourceVersion", "value": "5"},
                {"op": "add", "path": "/metadata/labels/zeabur.com~1project", "value": "true"}
            ]))
        );
        assert_eq!(cluster.inspect(|s| s.dry_runs), Some(1));

        let (status, applied) = plan_and_apply(
            &app,
            "shop",
            json!([{"op": "details", "displayName": "Web shop", "icon": "storefront"}]),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let shop = summary(&app, "shop").await;
        assert_eq!(
            (&shop["displayName"], &shop["icon"]),
            (&json!("Web shop"), &json!("storefront"))
        );

        let (status, applied) = plan_and_apply(&app, "shop", json!([{"op": "unadopt"}])).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let stored = cluster
            .inspect(|s| {
                s.namespaces
                    .iter()
                    .find(|n| n.metadata.name.as_deref() == Some("shop"))
                    .cloned()
            })
            .flatten()
            .unwrap();
        // Only the project label went; the other label and the details stay.
        assert_eq!(
            stored
                .metadata
                .labels
                .unwrap()
                .into_iter()
                .collect::<Vec<_>>(),
            [("team".to_owned(), "web".to_owned())]
        );
        assert_eq!(stored.metadata.annotations.unwrap().len(), 2);
        assert_eq!(summary(&app, "shop").await["adopted"], false);
    }

    #[tokio::test]
    async fn a_system_namespace_is_refused_with_its_own_type() {
        let cluster = cluster();
        let app = app(&cluster);
        assert_eq!(summary(&app, "kube-system").await["system"], true);
        let (status, problem) = plan_and_apply(&app, "kube-system", json!([{"op": "adopt"}])).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");
        assert_eq!(problem["type"], "/problems/system-namespace");
        assert_eq!(problem["kind"], "system-namespace");
        assert_eq!(
            cluster.inspect(|s| (s.writes.len(), s.dry_runs)),
            Some((0, 0))
        );
    }

    #[tokio::test]
    async fn a_refused_patch_names_the_right_to_ask_for() {
        let cluster = cluster();
        cluster.update(|s| {
            s.forbidden_writes.insert("Namespace/shop".to_owned());
        });
        let app = app(&cluster);
        let (status, namespace) = send(&app, "GET", "/api/v1/projects/shop/namespace", None).await;
        assert_eq!(status, StatusCode::OK, "{namespace}");
        assert_eq!(namespace["canChange"], "denied");
        assert_eq!(namespace["adopted"], false);
        assert_eq!(namespace["system"], false);

        let (status, problem) = plan_and_apply(&app, "shop", json!([{"op": "adopt"}])).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(
            problem["denied"],
            json!({"verb": "patch", "group": "", "resource": "namespaces", "name": "shop"})
        );
        assert_eq!(cluster.inspect(|s| s.writes.len()), Some(0));
    }

    fn with_roles(project_roles: bool) -> Settings {
        Settings {
            identity: IdentitySettings {
                project_roles,
                ..IdentitySettings::default()
            },
            ..Settings::default()
        }
    }

    /// With the member roles, a Developer changes the details but adopting needs
    /// `zeabur-manage`; without them nothing guards the labels, so `patch` is enough.
    #[tokio::test]
    async fn adopting_needs_the_manage_verb_only_where_the_policy_guards_the_labels() {
        let cluster = cluster();
        cluster.update(|s| {
            s.refused_rights.insert(format!("{MANAGE_VERB} namespaces"));
        });
        let uri = "/api/v1/projects/shop/namespace";
        let (status, guarded) = send(
            &TestApi::new(&cluster)
                .with_settings(with_roles(true))
                .router(),
            "GET",
            uri,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{guarded}");
        assert_eq!(
            (
                &guarded["canChange"],
                &guarded["canAdopt"],
                &guarded["labelsGuarded"]
            ),
            (&json!("allowed"), &json!("denied"), &json!(true))
        );
        let (_, open) = send(
            &TestApi::new(&cluster)
                .with_settings(with_roles(false))
                .router(),
            "GET",
            uri,
            None,
        )
        .await;
        assert_eq!(
            (
                &open["canChange"],
                &open["canAdopt"],
                &open["labelsGuarded"]
            ),
            (&json!("allowed"), &json!("allowed"), &json!(false))
        );

        cluster.update(|s| s.refused_rights.clear());
        let (_, manager) = send(
            &TestApi::new(&cluster)
                .with_settings(with_roles(true))
                .router(),
            "GET",
            uri,
            None,
        )
        .await;
        assert_eq!(manager["canAdopt"], "allowed");
    }

    fn node(name: &str, memory: &str, cpu: &str) -> zeabur_kube::k8s_openapi::api::core::v1::Node {
        serde_json::from_value(json!({
            "metadata": {"name": name, "uid": format!("uid-{name}")},
            "status": {"conditions": [{"type": "Ready", "status": "True"}],
                       "allocatable": {"cpu": cpu, "memory": memory, "pods": "110"}}
        }))
        .unwrap()
    }

    /// Build limits are the Namespace's annotations: the plan holds them to the largest node,
    /// and a person who may not patch the Namespace is refused at the apply like for any detail.
    #[tokio::test]
    async fn build_limits_stay_within_the_largest_node_and_need_the_right_to_patch_the_namespace() {
        let cluster = cluster();
        cluster.update(|s| {
            s.namespaces = vec![namespace("shop", &json!({"zeabur.com/project": "true"}))];
            s.nodes = vec![node("a", "4Gi", "2"), node("b", "16Gi", "8")];
        });
        let app = app(&cluster);
        let (_, shown) = send(&app, "GET", "/api/v1/projects/shop/namespace", None).await;
        assert_eq!(shown["maxBuildMemoryBytes"], 16_u64 << 30);
        assert_eq!(shown["maxBuildCpuMillicores"], 8000);

        let (status, problem) = plan_and_apply(
            &app,
            "shop",
            json!([{"op": "buildLimits", "buildMemory": "32Gi"}]),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");
        let (status, problem) = plan_and_apply(
            &app,
            "shop",
            json!([{"op": "buildLimits", "buildMemory": "8Gi", "buildCpu": "4"}]),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{problem}");
        let (_, shown) = send(&app, "GET", "/api/v1/projects/shop/namespace", None).await;
        assert_eq!(
            (&shown["buildMemory"], &shown["buildCpu"]),
            (&json!("8Gi"), &json!("4"))
        );

        cluster.update(|s| {
            s.forbidden_writes.insert("Namespace/shop".to_owned());
        });
        let (status, problem) = plan_and_apply(
            &app,
            "shop",
            json!([{"op": "buildLimits", "buildMemory": "2Gi", "buildCpu": "1"}]),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
    }

    fn rule(verbs: &[&str], group: &str) -> AccessRule {
        AccessRule {
            verbs: verbs.iter().map(|&v| v.to_owned()).collect(),
            groups: vec![group.to_owned()],
        }
    }

    /// A Viewer's rules: read what runs, change nothing, and ask about themselves.
    fn viewer_rules() -> AccessRules {
        AccessRules {
            rules: vec![
                rule(&["get", "list", "watch"], ""),
                rule(&["get", "list", "watch"], "apps"),
                rule(&["create"], "authorization.k8s.io"),
            ],
            incomplete: false,
        }
    }

    /// A project is read-only when its rules review has no write verb; an incomplete or failed
    /// review leaves it unknown.
    #[tokio::test]
    async fn the_namespace_says_whether_the_project_is_read_only_and_the_role() {
        let cluster = cluster();
        cluster.update(|s| {
            s.refused_rights.insert("create rolebindings".to_owned());
        });
        let uri = "/api/v1/projects/shop/namespace";
        let app = app(&cluster);
        let shown = |rules: Option<AccessRules>| {
            cluster.update(|s| {
                s.access_rules.insert("shop".to_owned(), rules);
            });
            async {
                let (status, body) = send(&app, "GET", uri, None).await;
                assert_eq!(status, StatusCode::OK, "{body}");
                (body["readOnly"].clone(), body["role"].clone())
            }
        };

        let viewer = viewer_rules();
        assert_eq!(
            shown(Some(viewer.clone())).await,
            (json!(true), json!("viewer"))
        );
        cluster.update(|s| {
            s.refused_rights.insert("get pods/log".to_owned());
        });
        let (_, logs) = send(&app, "GET", uri, None).await;
        assert_eq!(logs["canReadLogs"], "denied");
        // A read-only project may still let the person read secret values.
        assert_eq!(logs["canReadSecrets"], "allowed");
        let mut developer = viewer.clone();
        developer.rules.push(rule(&["patch"], "apps"));
        assert_eq!(
            shown(Some(developer)).await,
            (json!(false), json!("developer"))
        );
        let incomplete = AccessRules {
            incomplete: true,
            ..viewer
        };
        assert_eq!(shown(Some(incomplete)).await, (Value::Null, Value::Null));
        assert_eq!(shown(None).await, (Value::Null, Value::Null));
    }

    /// Cluster actions are cluster-wide access reviews: every right in `kube-system`, as a
    /// RoleBinding of `cluster-admin` there gives, allows none of them.
    #[tokio::test]
    async fn the_cluster_says_whether_the_person_may_change_nodes_and_backups() {
        let cluster = cluster();
        let app = app(&cluster);
        let (status, open) = send(&app, "GET", "/api/v1/cluster", None).await;
        assert_eq!(status, StatusCode::OK, "{open}");
        assert_eq!(
            open["can"],
            json!({"changeNodes": "allowed", "changeBackups": "allowed"})
        );
        cluster.update(|s| {
            s.access_rules
                .insert("kube-system".to_owned(), Some(AccessRules::everything()));
            s.cluster_wide_denied = true;
        });
        let (_, member) = send(&app, "GET", "/api/v1/cluster", None).await;
        assert_eq!(
            member["can"],
            json!({"changeNodes": "denied", "changeBackups": "denied"})
        );
    }

    /// The chart's admission policy refuses a Developer's label change with `403`; the answer
    /// quotes it as `policy-denied`, not as missing rights.
    #[tokio::test]
    async fn the_policys_refusal_of_a_label_change_is_quoted() {
        let cluster = cluster();
        let words = "Only an admin can change the namespace's labels.";
        cluster.update(|s| {
            s.admission_refusals.insert(
                "Namespace/shop".to_owned(),
                KubernetesStatus::new(
                    403,
                    "Forbidden",
                    format!(
                        "namespaces \"shop\" is forbidden: ValidatingAdmissionPolicy \
                         'zeabur-project-namespace' with binding 'zeabur-project-namespace' \
                         denied request: {words}"
                    ),
                ),
            );
        });
        let app = TestApi::new(&cluster)
            .with_settings(with_roles(true))
            .router();
        let (status, problem) = plan_and_apply(&app, "shop", json!([{"op": "adopt"}])).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(problem["kind"], "policy-denied");
        assert_eq!(
            problem["why"],
            json!({"key": "problem.why.policy-denied",
                   "params": {"policy": "zeabur-project-namespace", "message": words}})
        );
        assert_eq!(cluster.inspect(|s| s.writes.len()), Some(0));
    }

    #[tokio::test]
    async fn a_refused_read_of_the_namespace_names_the_get() {
        let cluster = cluster();
        cluster.update(|s| {
            s.forbidden_resources.insert("namespaces".to_owned());
        });
        let app = app(&cluster);
        let denied = json!({"verb": "get", "group": "", "resource": "namespaces", "name": "shop"});
        let (status, problem) = send(&app, "GET", "/api/v1/projects/shop/namespace", None).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(problem["denied"], denied);

        let (status, problem) = plan_and_apply(&app, "shop", json!([{"op": "adopt"}])).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(problem["denied"], denied);
    }

    #[tokio::test]
    async fn the_namespace_says_whether_the_person_may_change_it() {
        let cluster = cluster();
        cluster.update(|s| {
            s.namespaces.push(
                serde_json::from_value(json!({"metadata": {"name": "blog",
                "labels": {"zeabur.com/project": "true"},
                "annotations": {"zeabur.com/display-name": "Blog",
                    "zeabur.com/icon": "not-an-icon"}}}))
                .unwrap(),
            );
        });
        let app = app(&cluster);
        let (status, namespace) = send(&app, "GET", "/api/v1/projects/blog/namespace", None).await;
        assert_eq!(status, StatusCode::OK, "{namespace}");
        assert_eq!(namespace["canChange"], "allowed");
        assert_eq!(namespace["adopted"], true);
        assert_eq!(namespace["displayName"], "Blog");
        // An icon outside the built-in set is no icon.
        assert!(namespace.get("icon").is_none());
        let (status, _) = send(&app, "GET", "/api/v1/projects/nowhere/namespace", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_new_project_is_one_created_namespace_with_the_label_and_shows_in_the_list() {
        let cluster = cluster();
        let app = app(&cluster);
        let (_, list) = send(&app, "GET", "/api/v1/projects", None).await;
        assert_eq!(list["canCreate"], "allowed", "{list}");

        let (status, applied) = plan_and_apply(
            &app,
            "my-app",
            json!([{"op": "createProject", "displayName": "My app"}]),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        assert_eq!(writes[0].target.namespace, "");
        assert_eq!(writes[0].target.plural, "namespaces");
        assert!(matches!(writes[0].body, WriteBody::Create(_)));
        let account = writes
            .iter()
            .find(|w| w.target.kind == "ServiceAccount")
            .expect("the default ServiceAccount is written");
        assert_eq!(
            (
                account.target.namespace.as_str(),
                account.target.name.as_str()
            ),
            ("my-app", "default")
        );
        assert!(
            matches!(&account.body, WriteBody::Create(b) if b["automountServiceAccountToken"] == false),
            "{account:?}"
        );
        let created = summary(&app, "my-app").await;
        assert_eq!(created["adopted"], true, "{created}");
        assert_eq!(created["displayName"], "My app", "{created}");

        let (status, taken) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/plan",
            Some(&json!({"changes": [{"op": "createProject"}]})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{taken}");
    }

    /// Adopting patches the namespace, so the list asks for that right on each namespace that is
    /// not a project, not for the right to create one.
    #[tokio::test]
    async fn the_list_says_whether_the_person_may_adopt_each_namespace() {
        let cluster = cluster();
        cluster.update(|s| {
            s.cluster_wide_denied = true;
            s.namespaces
                .push(serde_json::from_value(json!({"metadata": {"name": "legacy"}})).unwrap());
        });
        let app = app(&cluster);
        let (_, list) = send(&app, "GET", "/api/v1/projects", None).await;
        let legacy = |list: &Value| {
            list["items"]
                .as_array()
                .and_then(|items| items.iter().find(|p| p["name"] == "legacy"))
                .map(|p| p["canAdopt"].clone())
        };
        assert_eq!(list["canCreate"], "denied", "{list}");
        assert_eq!(legacy(&list), Some(json!("allowed")), "{list}");

        cluster.update(|s| {
            s.refused_rights.insert("patch namespaces".to_owned());
        });
        let (_, list) = send(&app, "GET", "/api/v1/projects", None).await;
        assert_eq!(legacy(&list), Some(json!("denied")), "{list}");
    }

    #[tokio::test]
    async fn without_the_right_to_create_namespaces_the_list_says_so_and_the_apply_is_refused() {
        let cluster = cluster();
        cluster.update(|s| s.cluster_wide_denied = true);
        let app = app(&cluster);
        let (_, list) = send(&app, "GET", "/api/v1/projects", None).await;
        assert_eq!(list["canCreate"], "denied", "{list}");
        let (status, refused) =
            plan_and_apply(&app, "my-app", json!([{"op": "createProject"}])).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
        assert!(cluster.inspect(|s| s.writes.is_empty()).unwrap());
    }
}
