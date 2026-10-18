//! What engawa needs from kube-rs, checked against a real kube-apiserver: JSON Patch writes with
//! the `engawa` field manager and their `test` preconditions, creations, dry runs, and that a
//! [`UserClient`] acts as the person whose token it carries, with that person's rights and no
//! others. Revoking a right is checked through the API (`engawa-api`'s cluster tests).
//!
//! These tests need a disposable cluster where the current kubeconfig context is cluster admin
//! (CONTRIBUTING.md, "Tests"). The admin connection only prepares a namespace, a service account
//! standing in for a person, and its RBAC; every assertion is made through a [`UserClient`]
//! built from that account's token.
//!
//! The write contract (`contract/mod.rs`) runs here against k3s as the person, as it runs against
//! the in-memory cluster in `memory.rs`.

#[cfg(test)]
mod contract;

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use engawa_kube::{
        Cluster as _, ClusterError, ClusterExt as _, ExecRequest, ExitStatus, ObjectTarget,
        ResourceRef, TerminalSize, WriteBody, WriteRequest,
    };
    use engawa_test_support::sandbox::{PERSON, Sandbox};
    use k8s_openapi::api::authentication::v1::SelfSubjectReview;
    use k8s_openapi::api::core::v1::{ConfigMap, Node, Pod, ResourceQuota};
    use k8s_openapi::api::rbac::v1::{ClusterRole, ClusterRoleBinding, Role, RoleBinding};
    use kube::api::{Api, ListParams, PostParams};
    use serde_json::json;

    use super::contract;

    /// The person's Role: ConfigMaps in full, Pods to read (and exec into, where a test adds
    /// it), Deployments to create and patch.
    fn rules() -> serde_json::Value {
        json!([
            {"apiGroups": [""], "resources": ["configmaps"],
             "verbs": ["get", "list", "create", "patch", "delete"]},
            {"apiGroups": [""], "resources": ["pods"], "verbs": ["get"]},
            {"apiGroups": ["apps"], "resources": ["deployments"],
             "verbs": ["get", "create", "patch"]}
        ])
    }

    fn status_code(error: &kube::Error) -> Option<u16> {
        if let kube::Error::Api(status) = error {
            Some(status.code)
        } else {
            None
        }
    }

    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    async fn the_client_is_the_person_and_nobody_else() {
        let sandbox = Sandbox::new("engawa-identity", rules()).await;
        let review = Api::<SelfSubjectReview>::all(sandbox.user().kube().clone())
            .create(&PostParams::default(), &SelfSubjectReview::default())
            .await
            .expect("self subject review");
        let username = review
            .status
            .and_then(|s| s.user_info)
            .and_then(|u| u.username);
        assert_eq!(
            username.as_deref(),
            Some(format!("system:serviceaccount:{}:{PERSON}", sandbox.namespace()).as_str())
        );
        // Outside the person's rights: another namespace.
        let elsewhere = Api::<ConfigMap>::namespaced(sandbox.user().kube().clone(), "default");
        let error = elsewhere.list(&ListParams::default()).await.unwrap_err();
        assert_eq!(status_code(&error), Some(403), "{error}");
    }

    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    async fn any_served_resource_is_read_generically_as_the_person() {
        let sandbox = Sandbox::new("engawa-generic", rules()).await;
        let user = sandbox.user();
        sandbox
            .user_api::<ConfigMap>()
            .create(
                &PostParams::default(),
                &serde_json::from_value(json!({
                    "metadata": {"name": "app"}, "data": {"mode": "fast"}
                }))
                .unwrap(),
            )
            .await
            .expect("config map");
        let served = user.served_resources().await.expect("discovery");
        let config_maps = served
            .iter()
            .find(|r| r.group.is_empty() && r.resource == "configmaps")
            .expect("config maps are served");
        assert!(config_maps.namespaced && config_maps.listable());
        assert_eq!(user.serves("v1").await.expect("core"), Some(true));
        assert_eq!(user.serves("apps/v1").await.expect("apps"), Some(true));

        let resource = config_maps.as_ref();
        let names: Vec<String> = user
            .list_metadata(&resource, Some(sandbox.namespace()))
            .await
            .expect("metadata")
            .expect("served")
            .into_iter()
            .filter_map(|m| m.name)
            .collect();
        assert!(names.contains(&"app".to_owned()), "{names:?}");
        let object = user
            .get_object(&resource, Some(sandbox.namespace()), "app")
            .await
            .expect("get")
            .expect("served");
        assert_eq!(object.data["data"]["mode"], "fast");
        let missing = user
            .get_object(&resource, Some(sandbox.namespace()), "nope")
            .await;
        assert_eq!(missing.unwrap_err().code(), Some(404));

        // Secrets are outside the person's role: refused, not "not installed".
        let secrets = ResourceRef::new("", "v1", "secrets", "Secret");
        let refused = user
            .list_metadata(&secrets, Some(sandbox.namespace()))
            .await;
        assert_eq!(refused.unwrap_err().code(), Some(403));

        // The listing path skips discovery and may be served from the watch cache; the rules
        // review hints at the same answer without a refusal.
        let cached: Vec<String> = user
            .list_served_metadata(&resource, sandbox.namespace())
            .await
            .expect("metadata")
            .expect("served")
            .into_iter()
            .filter_map(|m| m.name)
            .collect();
        assert!(cached.contains(&"app".to_owned()), "{cached:?}");
        let rules = user
            .access_rules(sandbox.namespace())
            .await
            .expect("rules review");
        assert_eq!(rules.list(&resource), engawa_kube::Access::Allowed);
        assert_ne!(rules.list(&secrets), engawa_kube::Access::Allowed);
    }

    /// The ConfigMap `name` of the sandbox, as a write addresses it.
    fn configmap(sandbox: &Sandbox, name: &str) -> ObjectTarget {
        ObjectTarget {
            api_version: "v1".to_owned(),
            kind: "ConfigMap".to_owned(),
            plural: "configmaps".to_owned(),
            namespace: sandbox.namespace().to_owned(),
            name: name.to_owned(),
        }
    }

    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    async fn a_json_patch_writes_as_engawa_and_its_tests_refuse_a_stale_plan() {
        let sandbox = Sandbox::new("engawa-patch", rules()).await;
        // Another manager (here: a Helm release) made the object with two keys.
        Api::<ConfigMap>::namespaced(sandbox.admin().clone(), sandbox.namespace())
            .create(
                &PostParams {
                    field_manager: Some("helm".to_owned()),
                    ..PostParams::default()
                },
                &serde_json::from_value(
                    json!({"metadata": {"name": "app"}, "data": {"a": "1", "b": "2"}}),
                )
                .unwrap(),
            )
            .await
            .expect("helm's ConfigMap");
        let configmaps = sandbox.user_api::<ConfigMap>();
        let read = configmaps.get("app").await.expect("read");
        let version = read.metadata.resource_version.clone().unwrap();
        let patch = WriteRequest {
            target: configmap(&sandbox, "app"),
            body: WriteBody::JsonPatch(json!([
                {"op": "test", "path": "/metadata/resourceVersion", "value": version},
                {"op": "test", "path": "/data/a", "value": "1"},
                {"op": "replace", "path": "/data/a", "value": "10"}
            ])),
            dry_run: false,
        };
        sandbox.user().write(&patch).await.expect("patch");

        // Only the patched key changed, and `managedFields` names engawa for it.
        let written = configmaps.get("app").await.expect("read again");
        let data = written.data.clone().unwrap_or_default();
        assert_eq!(data.get("a").map(String::as_str), Some("10"));
        assert_eq!(data.get("b").map(String::as_str), Some("2"));
        let managers: Vec<&str> = written
            .metadata
            .managed_fields
            .iter()
            .flatten()
            .filter_map(|m| m.manager.as_deref())
            .collect();
        assert!(managers.contains(&"engawa"), "{managers:?}");

        // The same patch again tests a resourceVersion that has moved on. kube-apiserver
        // answers a failed JSON Patch test with 422 Invalid; its message is not an interface
        // (kube-rs may report its own), so only the code and reason are checked. The API reads
        // the object again to tell this from an invalid object.
        let error = sandbox.user().write(&patch).await.unwrap_err();
        let ClusterError::Status(status) = error else {
            panic!("a status answer: {error:?}");
        };
        assert_eq!(status.code, 422, "{status:?}");
        assert_eq!(status.reason, "Invalid", "{status:?}");
    }

    fn refused<T: std::fmt::Debug>(result: Result<T, ClusterError>) -> bool {
        matches!(result, Err(e) if e.code() == Some(403))
    }

    /// A quota's refusal is a `403` like a refusal for missing rights; one
    /// `SelfSubjectAccessReview` tells them apart: the person's rights allow the first.
    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    async fn a_quota_refusal_is_told_from_missing_rights_by_an_access_review() {
        let sandbox = Sandbox::new("engawa-quota", rules()).await;
        // Every namespace holds `kube-root-ca.crt`, so a limit of one ConfigMap is used up.
        let quotas = Api::<ResourceQuota>::namespaced(sandbox.admin().clone(), sandbox.namespace());
        quotas
            .create(
                &PostParams::default(),
                &serde_json::from_value(json!({
                    "metadata": {"name": "objects"},
                    "spec": {"hard": {"count/configmaps": "1"}}
                }))
                .unwrap(),
            )
            .await
            .expect("quota");
        // Quota admission refuses everything until the quota controller has filled the status.
        let deadline = Instant::now() + Duration::from_secs(60);
        while quotas
            .get("objects")
            .await
            .expect("read the quota")
            .status
            .and_then(|s| s.used)
            .is_none()
        {
            assert!(
                Instant::now() < deadline,
                "the quota's status was never set"
            );
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        let create = WriteRequest {
            target: configmap(&sandbox, "extra"),
            body: WriteBody::Create(json!({"apiVersion": "v1", "kind": "ConfigMap",
                "metadata": {"name": "extra"}})),
            dry_run: true,
        };
        let error = sandbox.user().write(&create).await.unwrap_err();
        let ClusterError::Status(status) = error else {
            panic!("a status answer: {error:?}");
        };
        assert_eq!(status.code, 403, "{status:?}");
        assert!(
            status.message.contains("exceeded quota: objects"),
            "{status:?}"
        );

        let request = |verb: &str, group: &str, resource: &str| engawa_core::problem::Denied {
            verb: verb.to_owned(),
            group: group.to_owned(),
            resource: resource.to_owned(),
            name: None,
            namespace: Some(sandbox.namespace().to_owned()),
        };
        let user = sandbox.user();
        assert!(
            user.review_access(&request("create", "", "configmaps"))
                .await
                .unwrap()
        );
        assert!(
            !user
                .review_access(&request("delete", "apps", "deployments"))
                .await
                .unwrap()
        );
        // The person's role does not read quotas; the admin's does, with the numbers.
        assert!(refused(
            user.list::<ResourceQuota>(Some(sandbox.namespace())).await
        ));
    }

    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    async fn cluster_wide_reads_need_the_cluster_overview_role() {
        let sandbox = Sandbox::new("engawa-wide", rules()).await;
        let user = sandbox.user();
        // A namespaced role gives none of the cluster-wide reads.
        assert!(refused(user.list::<Node>(None).await));
        assert!(refused(user.cluster_pods(5_000).await));
        assert!(refused(user.cluster_events(5_000).await));
        // k3s ships metrics-server, so node metrics are there and refused, once its APIService
        // is available (it is not for a while after the cluster starts).
        let deadline = Instant::now() + Duration::from_secs(180);
        while sandbox
            .admin()
            .list_api_group_resources("metrics.k8s.io/v1beta1")
            .await
            .is_err()
        {
            assert!(
                Instant::now() < deadline,
                "metrics-server did not become available"
            );
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        let node_metrics = ResourceRef::new("metrics.k8s.io", "v1beta1", "nodes", "NodeMetrics");
        let plans = ResourceRef::new("upgrade.cattle.io", "v1", "plans", "Plan");
        assert!(refused(user.list_objects(&node_metrics, None).await));
        // No system-upgrade-controller in a test cluster. Listing Plans would be refused before
        // kube-apiserver looked the API up; discovery says it is not installed, so it is not
        // reported as a missing permission.
        assert!(
            user.list_objects(&plans, None)
                .await
                .expect("plans")
                .is_none()
        );

        // The ClusterRole the overview's "Copy ClusterRole" gives.
        let name = format!("{}-overview", sandbox.namespace());
        sandbox.delete_cluster_role(&name);
        let pp = PostParams::default();
        Api::<ClusterRole>::all(sandbox.admin().clone())
            .create(
                &pp,
                &serde_json::from_value(json!({
                    "metadata": {"name": name},
                    "rules": [
                        {"apiGroups": [""], "resources": ["nodes", "namespaces", "pods"], "verbs": ["list"]},
                        {"apiGroups": ["events.k8s.io"], "resources": ["events"], "verbs": ["list"]},
                        {"apiGroups": ["metrics.k8s.io"], "resources": ["nodes"], "verbs": ["list"]},
                        {"apiGroups": ["upgrade.cattle.io"], "resources": ["plans"], "verbs": ["list"]}
                    ]
                }))
                .unwrap(),
            )
            .await
            .expect("cluster role");
        Api::<ClusterRoleBinding>::all(sandbox.admin().clone())
            .create(
                &pp,
                &serde_json::from_value(json!({
                    "metadata": {"name": name},
                    "roleRef": {"apiGroup": "rbac.authorization.k8s.io", "kind": "ClusterRole", "name": name},
                    "subjects": [{"kind": "ServiceAccount", "name": PERSON, "namespace": sandbox.namespace()}]
                }))
                .unwrap(),
            )
            .await
            .expect("cluster role binding");

        // kube-apiserver's RBAC authorizer reads bindings from a watch cache, so a new binding
        // takes effect within moments rather than on the very next request.
        let deadline = Instant::now() + Duration::from_secs(10);
        let nodes = loop {
            match user.list::<Node>(None).await {
                Ok(nodes) => break nodes,
                Err(error) => assert!(
                    Instant::now() < deadline,
                    "nodes still refused 10 s after the binding: {error:?}"
                ),
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        assert_ne!(nodes.len(), 0);
        let pods = user.cluster_pods(5_000).await.expect("pods");
        assert!(!pods.truncated);
        assert!(pods.pods.iter().any(|p| p.namespace == "kube-system"));
        assert!(
            pods.pods
                .iter()
                .all(|p| p.phase.as_deref() != Some("Succeeded"))
        );
        if pods.pods.len() > 1 {
            let first = user.cluster_pods(1).await.expect("one pod");
            assert_eq!(first.pods.len(), 1);
            assert!(first.truncated);
        }
        user.cluster_events(5_000).await.expect("events");
        // k3s ships metrics-server; it may not have measured anything yet.
        user.list_objects(&node_metrics, None)
            .await
            .expect("node usage")
            .expect("metrics-server is installed");
        // No system-upgrade-controller in a test cluster: no Plans, not an error.
        assert!(
            user.list_objects(&plans, None)
                .await
                .expect("plans")
                .is_none()
        );
    }

    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    async fn a_terminal_runs_as_the_person_and_needs_create_on_pods_exec() {
        use tokio::io::AsyncReadExt as _;

        let sandbox = Sandbox::new("engawa-exec", rules()).await;
        let pp = PostParams::default();
        let pods = Api::<Pod>::namespaced(sandbox.admin().clone(), sandbox.namespace());
        pods.create(
            &pp,
            &serde_json::from_value(json!({
                "metadata": {"name": "shell"},
                "spec": {"containers": [{"name": "web", "image": "busybox:1.37",
                                         "command": ["sleep", "3600"]}]}
            }))
            .unwrap(),
        )
        .await
        .expect("pod");
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let pod = pods.get("shell").await.expect("pod");
            if pod.status.and_then(|s| s.phase).as_deref() == Some("Running") {
                break;
            }
            assert!(Instant::now() < deadline, "the pod did not start");
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        let request = ExecRequest {
            container: "web".to_owned(),
            command: vec![
                "sh".to_owned(),
                "-c".to_owned(),
                "echo engawa; exit 3".to_owned(),
            ],
        };
        // The person's role has `get pods` only: the stream is refused before it opens.
        let refused = sandbox
            .user()
            .exec(sandbox.namespace(), "shell", &request)
            .await;
        assert_eq!(refused.err().and_then(|e| e.code()), Some(403));

        Api::<Role>::namespaced(sandbox.admin().clone(), sandbox.namespace())
            .create(
                &pp,
                &serde_json::from_value(json!({
                    "metadata": {"name": "exec"},
                    "rules": [{"apiGroups": [""], "resources": ["pods/exec"], "verbs": ["get", "create"]}]
                }))
                .unwrap(),
            )
            .await
            .expect("role");
        Api::<RoleBinding>::namespaced(sandbox.admin().clone(), sandbox.namespace())
            .create(
                &pp,
                &serde_json::from_value(json!({
                    "metadata": {"name": "exec"},
                    "roleRef": {"apiGroup": "rbac.authorization.k8s.io", "kind": "Role", "name": "exec"},
                    "subjects": [{"kind": "ServiceAccount", "name": PERSON, "namespace": sandbox.namespace()}]
                }))
                .unwrap(),
            )
            .await
            .expect("role binding");
        // RBAC bindings take effect within moments (see above).
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut session = loop {
            match sandbox
                .user()
                .exec(sandbox.namespace(), "shell", &request)
                .await
            {
                Ok(session) => break session,
                Err(error) => assert!(
                    Instant::now() < deadline,
                    "exec still refused 10 s after the binding: {error:?}"
                ),
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        };
        (session.resize)(TerminalSize {
            cols: 100,
            rows: 30,
        });
        let mut output = Vec::new();
        tokio::time::timeout(
            Duration::from_secs(30),
            session.output.read_to_end(&mut output),
        )
        .await
        .expect("output in time")
        .expect("output");
        assert!(
            String::from_utf8_lossy(&output).contains("engawa"),
            "{output:?}"
        );
        assert_eq!(session.exit.await, ExitStatus::Exited(3));
    }

    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    async fn contract_a_patch_whose_tests_hold_is_stored_as_computed() {
        let sandbox = Sandbox::new("engawa-contract", rules()).await;
        contract::a_patch_whose_tests_hold_is_stored_as_computed(
            sandbox.user(),
            sandbox.namespace(),
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    async fn contract_a_test_that_fails_refuses_the_write_and_stores_nothing() {
        let sandbox = Sandbox::new("engawa-contract", rules()).await;
        contract::a_test_that_fails_refuses_the_write_and_stores_nothing(
            sandbox.user(),
            sandbox.namespace(),
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    async fn contract_a_taken_name_is_refused_and_a_missing_object_is_not_found() {
        let sandbox = Sandbox::new("engawa-contract", rules()).await;
        contract::a_taken_name_is_refused_and_a_missing_object_is_not_found(
            sandbox.user(),
            sandbox.namespace(),
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    async fn contract_a_dry_run_validates_strictly_and_stores_nothing() {
        let sandbox = Sandbox::new("engawa-contract", rules()).await;
        contract::a_dry_run_validates_strictly_and_stores_nothing(
            sandbox.user(),
            sandbox.namespace(),
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    async fn contract_of_concurrent_patches_of_one_version_exactly_one_is_stored() {
        let sandbox = Sandbox::new("engawa-contract", rules()).await;
        contract::of_concurrent_patches_of_one_version_exactly_one_is_stored(
            sandbox.user(),
            sandbox.namespace(),
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    async fn contract_a_deletion_keeps_to_its_preconditions() {
        let sandbox = Sandbox::new("engawa-contract", rules()).await;
        contract::a_deletion_keeps_to_its_preconditions(sandbox.user(), sandbox.namespace()).await;
    }
}
