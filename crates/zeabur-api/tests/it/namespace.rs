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
    use zeabur_core::problem::KubernetesStatus;
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{AccessRule, AccessRules, Identity, WriteBody};

    use crate::common::{app, app_with, plan_and_apply, send};

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

    /// What `zeabur-project-developer` holds on the Namespace through its RoleBinding.
    fn developer_rules() -> AccessRules {
        AccessRules {
            rules: vec![AccessRule {
                verbs: vec!["get".to_owned(), "patch".to_owned()],
                groups: vec![String::new()],
                resources: vec!["namespaces".to_owned()],
                names: Vec::new(),
            }],
            incomplete: false,
        }
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
            s.access_rules
                .insert("shop".to_owned(), Some(developer_rules()));
        });
        let uri = "/api/v1/projects/shop/namespace";
        let (status, guarded) = send(&app_with(&cluster, with_roles(true)), "GET", uri, None).await;
        assert_eq!(status, StatusCode::OK, "{guarded}");
        assert_eq!(
            (
                &guarded["canChange"],
                &guarded["canAdopt"],
                &guarded["labelsGuarded"]
            ),
            (&json!("allowed"), &json!("denied"), &json!(true))
        );
        // The rules hold no `pods/log`, as for a Viewer.
        assert_eq!(guarded["can"]["readLogs"], "denied");
        assert_eq!(
            guarded["workloadPatch"][0],
            json!({"kind": "Deployment", "all": "denied", "names": []})
        );
        let (_, open) = send(&app_with(&cluster, with_roles(false)), "GET", uri, None).await;
        assert_eq!(
            (
                &open["canChange"],
                &open["canAdopt"],
                &open["labelsGuarded"]
            ),
            (&json!("allowed"), &json!("allowed"), &json!(false))
        );

        cluster.update(|s| {
            let mut manager = developer_rules();
            manager.rules[0].verbs.push("zeabur-manage".to_owned());
            s.access_rules.insert("shop".to_owned(), Some(manager));
        });
        let (_, manager) = send(&app_with(&cluster, with_roles(true)), "GET", uri, None).await;
        assert_eq!(manager["canAdopt"], "allowed");

        // A rule limited to names allows patching only those.
        cluster.update(|s| {
            let mut named = developer_rules();
            named.rules.push(AccessRule {
                verbs: vec!["patch".to_owned()],
                groups: vec!["apps".to_owned()],
                resources: vec!["deployments".to_owned()],
                names: vec!["web".to_owned()],
            });
            s.access_rules.insert("shop".to_owned(), Some(named));
        });
        let (_, named) = send(&app_with(&cluster, with_roles(true)), "GET", uri, None).await;
        assert_eq!(
            named["workloadPatch"][0],
            json!({"kind": "Deployment", "all": "denied", "names": ["web"]})
        );
    }

    fn rule(verbs: &[&str], group: &str, resources: &[&str]) -> AccessRule {
        AccessRule {
            verbs: verbs.iter().map(|&v| v.to_owned()).collect(),
            groups: vec![group.to_owned()],
            resources: resources.iter().map(|&r| r.to_owned()).collect(),
            names: Vec::new(),
        }
    }

    /// A Viewer's rules: read what runs, change nothing.
    fn viewer_rules() -> AccessRules {
        AccessRules {
            rules: vec![
                rule(
                    &["get", "list", "watch"],
                    "",
                    &["pods", "services", "configmaps"],
                ),
                rule(&["get", "list", "watch"], "apps", &["deployments"]),
            ],
            incomplete: false,
        }
    }

    /// The namespace says what the person may change there and their role, from the one review;
    /// what an incomplete review does not allow, or a failed one, is unknown.
    #[tokio::test]
    async fn the_namespace_says_what_the_person_may_change_and_their_role() {
        let cluster = cluster();
        let uri = "/api/v1/projects/shop/namespace";
        let app = app(&cluster);

        cluster.update(|s| {
            s.access_rules
                .insert("shop".to_owned(), Some(viewer_rules()));
        });
        let (status, viewer) = send(&app, "GET", uri, None).await;
        assert_eq!(status, StatusCode::OK, "{viewer}");
        assert_eq!(viewer["role"], "viewer");
        for (right, answer) in viewer["can"].as_object().unwrap() {
            assert_eq!(answer, "denied", "{right}");
        }

        cluster.update(|s| {
            let mut developer = viewer_rules();
            developer.rules.extend([
                rule(&["get"], "", &["pods/log", "secrets"]),
                rule(&["create"], "", &["pods/exec"]),
                rule(
                    &["create", "update", "patch", "delete"],
                    "apps",
                    &["deployments"],
                ),
                rule(&["create", "update", "patch", "delete"], "", &["services"]),
            ]);
            s.access_rules.insert("shop".to_owned(), Some(developer));
        });
        let (_, developer) = send(&app, "GET", uri, None).await;
        assert_eq!(developer["role"], "developer");
        let can = &developer["can"];
        assert_eq!(
            (
                &can["deploy"],
                &can["changeServices"],
                &can["deleteServices"]
            ),
            (&json!("allowed"), &json!("allowed"), &json!("allowed"))
        );
        assert_eq!(
            (&can["readLogs"], &can["openTerminal"], &can["readSecrets"]),
            (&json!("allowed"), &json!("allowed"), &json!("allowed"))
        );
        assert_eq!(
            (&can["changeDomains"], &can["changeMembers"], &can["scale"]),
            (&json!("denied"), &json!("denied"), &json!("denied"))
        );

        cluster.update(|s| {
            let mut incomplete = viewer_rules();
            incomplete.incomplete = true;
            s.access_rules.insert("shop".to_owned(), Some(incomplete));
        });
        let (_, incomplete) = send(&app, "GET", uri, None).await;
        assert_eq!(incomplete["can"]["deploy"], "unknown");
        assert_eq!(incomplete["role"], "viewer");

        cluster.update(|s| {
            s.access_rules.insert("shop".to_owned(), None);
        });
        let (status, failed) = send(&app, "GET", uri, None).await;
        assert_eq!(status, StatusCode::OK, "{failed}");
        assert!(failed.get("role").is_none(), "{failed}");
        assert_eq!(failed["can"]["changeMembers"], "unknown");
    }

    /// The cluster says whether the person may install components, from a review in
    /// `kube-system`.
    #[tokio::test]
    async fn the_cluster_says_whether_the_person_may_install_components() {
        let cluster = cluster();
        let app = app(&cluster);
        let (status, open) = send(&app, "GET", "/api/v1/cluster", None).await;
        assert_eq!(status, StatusCode::OK, "{open}");
        assert_eq!(
            open["can"],
            json!({"installComponents": "allowed", "changeNodes": "allowed"})
        );
        cluster.update(|s| {
            s.access_rules.insert(
                "kube-system".to_owned(),
                Some(AccessRules {
                    rules: vec![rule(&["get", "list"], "", &["nodes", "namespaces"])],
                    incomplete: false,
                }),
            );
        });
        let (_, member) = send(&app, "GET", "/api/v1/cluster", None).await;
        assert_eq!(
            member["can"],
            json!({"installComponents": "denied", "changeNodes": "denied"})
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
        let app = app_with(&cluster, with_roles(true));
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
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].target.namespace, "");
        assert_eq!(writes[0].target.plural, "namespaces");
        assert!(matches!(writes[0].body, WriteBody::Create(_)));
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
