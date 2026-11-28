//! Deleting a project on a real cluster, as the person: a project with a database goes away, the
//! database object first, then the Namespace, as `kubectl delete namespace` does. The person's
//! Role in the namespace names the Namespace itself, which kube-apiserver authorises as a request
//! in that namespace.
//!
//! Where CloudNativePG is not installed (the bare profile), a stand-in for its `Cluster` CRD is
//! made and deleted with the sandbox. A running pod the `Cluster` owns has CloudNativePG's grace
//! period (1800s) and, like PostgreSQL, takes some seconds to stop, so a Namespace deleted while it
//! is still there waits for the namespace controller's 5-minute resync.
//!
//! A project held by a finalizer no controller removes is force deleted by a person whose rights
//! come from a ClusterRole, since the namespace controller deletes the Roles inside the project.

use std::time::Duration;

use axum::http::StatusCode;
use kube::api::{Api, DeleteParams, DynamicObject, Patch, PatchParams, PostParams};
use kube::core::GroupVersionKind;
use kube::discovery::ApiResource;
use serde_json::json;
use zeabur_kube::k8s_openapi::api::core::v1::{ConfigMap, Namespace, Pod};
use zeabur_kube::k8s_openapi::api::rbac::v1::{ClusterRole, ClusterRoleBinding};
use zeabur_test_support::sandbox::{PERSON, Sandbox, create};

use crate::common::{app, plan_and_apply};

const CRD: &str = "clusters.postgresql.cnpg.io";
/// Long enough for the namespace controller to empty a namespace.
const WAIT: Duration = Duration::from_secs(180);
/// Well below the namespace controller's 5-minute resync.
const PROMPT: Duration = Duration::from_secs(90);

fn api(group: &str, version: &str, kind: &str) -> ApiResource {
    ApiResource::from_gvk(&GroupVersionKind::gvk(group, version, kind))
}

/// Makes the stand-in CRD when CloudNativePG's is not served.
async fn database_kind(sandbox: &Sandbox) {
    let admin = sandbox.admin().clone();
    if admin
        .list_api_group_resources("postgresql.cnpg.io/v1")
        .await
        .is_ok_and(|list| list.resources.iter().any(|r| r.name == "clusters"))
    {
        return;
    }
    let crds = Api::<DynamicObject>::all_with(
        admin.clone(),
        &api("apiextensions.k8s.io", "v1", "CustomResourceDefinition"),
    );
    let crd: DynamicObject = serde_json::from_value(json!({
        "apiVersion": "apiextensions.k8s.io/v1", "kind": "CustomResourceDefinition",
        "metadata": {"name": CRD},
        "spec": {"group": "postgresql.cnpg.io", "scope": "Namespaced",
                 "names": {"plural": "clusters", "singular": "cluster", "kind": "Cluster"},
                 "versions": [{"name": "v1", "served": true, "storage": true,
                               "schema": {"openAPIV3Schema": {"type": "object",
                                   "x-kubernetes-preserve-unknown-fields": true}}}]}
    }))
    .unwrap();
    sandbox.delete_crd(CRD);
    let _exists = crds.create(&PostParams::default(), &crd).await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        let served = admin
            .list_api_group_resources("postgresql.cnpg.io/v1")
            .await
            .is_ok_and(|list| list.resources.iter().any(|r| r.name == "clusters"));
        if served {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the stand-in CRD is not served"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[tokio::test]
#[ignore = "needs a disposable cluster"]
async fn a_project_with_a_database_goes_away() {
    // A Role's rule on `namespaces` reaches only its own namespace: kube-apiserver authorises a
    // request for `namespaces/{name}` as one in namespace `name`.
    let sandbox = Sandbox::new(
        "zeabur-delete",
        json!([
            {"apiGroups": ["", "apps", "batch", "networking.k8s.io", "events.k8s.io"],
             "resources": ["*"], "verbs": ["get", "list"]},
            {"apiGroups": ["postgresql.cnpg.io"], "resources": ["clusters"],
             "verbs": ["get", "list", "watch", "delete"]},
            {"apiGroups": [""], "resources": ["namespaces"], "verbs": ["get", "delete"]}
        ]),
    )
    .await;
    let (ns, admin) = (sandbox.namespace().to_owned(), sandbox.admin().clone());
    let namespaces = Api::<Namespace>::all(admin.clone());
    namespaces
        .patch(
            &ns,
            &PatchParams::default(),
            &Patch::Merge(json!({"metadata": {"labels": {"zeabur.com/project": "true"}}})),
        )
        .await
        .unwrap();
    database_kind(&sandbox).await;
    let clusters = Api::<DynamicObject>::namespaced_with(
        admin.clone(),
        &ns,
        &api("postgresql.cnpg.io", "v1", "Cluster"),
    );
    let database: DynamicObject = serde_json::from_value(json!({
        "apiVersion": "postgresql.cnpg.io/v1", "kind": "Cluster",
        "metadata": {"name": "db"},
        "spec": {"instances": 1, "storage": {"size": "1Gi"}}
    }))
    .unwrap();
    let database = clusters
        .create(&PostParams::default(), &database)
        .await
        .unwrap();
    let pods = Api::<Pod>::namespaced(admin.clone(), &ns);
    create(
        &pods,
        json!({"metadata": {"name": "db-1", "ownerReferences": [{
                   "apiVersion": "postgresql.cnpg.io/v1", "kind": "Cluster", "name": "db",
                   "uid": database.metadata.uid, "controller": true,
                   "blockOwnerDeletion": true}]},
               "spec": {"terminationGracePeriodSeconds": 1800,
                        "containers": [{"name": "db", "image": "busybox:1.37",
                            "command": ["sh", "-c",
                                "trap 'sleep 20; exit 0' TERM; while true; do sleep 1; done"]}]}}),
    )
    .await;
    let deadline = tokio::time::Instant::now() + WAIT;
    while pods
        .get("db-1")
        .await
        .unwrap()
        .status
        .and_then(|s| s.phase)
        .as_deref()
        != Some("Running")
    {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the database pod is not running"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    let app = app(sandbox.user());
    // RBAC follows the Role's change within moments; plan until the plan sees the right.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let (status, applied) = loop {
        let (status, body) = plan_and_apply(&app, &ns, json!([{"op": "deleteProject"}])).await;
        if status == StatusCode::OK || tokio::time::Instant::now() >= deadline {
            break (status, body);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    assert_eq!(status, StatusCode::OK, "{applied}");
    let kinds: Vec<&str> = applied["applied"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["target"]["kind"].as_str())
        .collect();
    assert_eq!(kinds, ["Cluster", "Namespace"], "{applied}");
    assert!(
        pods.get_opt("db-1").await.unwrap().is_none(),
        "the Namespace was deleted before the database pod was gone"
    );
    gone(&namespaces, &ns, PROMPT).await;
}

/// Waits until `namespace` is gone, for at most `wait`.
async fn gone(namespaces: &Api<Namespace>, namespace: &str, wait: Duration) {
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        match namespaces.get_opt(namespace).await.unwrap() {
            None => return,
            Some(left) => assert!(
                tokio::time::Instant::now() < deadline,
                "the namespace is still there: {:?}",
                left.status
            ),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[tokio::test]
#[ignore = "needs a disposable cluster"]
async fn a_project_held_by_a_finalizer_without_a_controller_is_force_deleted() {
    let person = Sandbox::new(
        "zeabur-force-person",
        json!([{"apiGroups": [""], "resources": ["configmaps"], "verbs": ["get"]}]),
    )
    .await;
    let project = Sandbox::new(
        "zeabur-force",
        json!([{"apiGroups": [""], "resources": ["configmaps"], "verbs": ["get"]}]),
    )
    .await;
    let (ns, admin) = (project.namespace().to_owned(), project.admin().clone());
    let role = person.namespace().to_owned();
    person.delete_cluster_role(&role);
    create(
        &Api::<ClusterRole>::all(admin.clone()),
        json!({"metadata": {"name": role}, "rules": [
            {"apiGroups": ["", "apps", "batch", "networking.k8s.io", "events.k8s.io"],
             "resources": ["*"], "verbs": ["get", "list"]},
            {"apiGroups": [""], "resources": ["configmaps"], "verbs": ["patch"]}
        ]}),
    )
    .await;
    create(
        &Api::<ClusterRoleBinding>::all(admin.clone()),
        json!({"metadata": {"name": role},
               "roleRef": {"apiGroup": "rbac.authorization.k8s.io", "kind": "ClusterRole", "name": role},
               "subjects": [{"kind": "ServiceAccount", "name": PERSON, "namespace": role}]}),
    )
    .await;
    let namespaces = Api::<Namespace>::all(admin.clone());
    namespaces
        .patch(
            &ns,
            &PatchParams::default(),
            &Patch::Merge(json!({"metadata": {"labels": {"zeabur.com/project": "true"}}})),
        )
        .await
        .unwrap();
    create(
        &Api::<ConfigMap>::namespaced(admin.clone(), &ns),
        json!({"metadata": {"name": "held", "finalizers": ["example.com/no-controller"]}}),
    )
    .await;
    let _deleting = namespaces.delete(&ns, &DeleteParams::background()).await;

    // The namespace controller names the finalizer in a condition once it has deleted the rest.
    let app = app(person.user());
    let deadline = tokio::time::Instant::now() + WAIT;
    let (status, applied) = loop {
        let (status, body) = plan_and_apply(&app, &ns, json!([{"op": "forceDeleteProject"}])).await;
        if status == StatusCode::OK || tokio::time::Instant::now() >= deadline {
            break (status, body);
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    };
    assert_eq!(status, StatusCode::OK, "{applied}");
    let written: Vec<(&str, &str)> = applied["applied"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| Some((a["target"]["kind"].as_str()?, a["target"]["name"].as_str()?)))
        .collect();
    assert_eq!(written, [("ConfigMap", "held")], "{applied}");
    gone(&namespaces, &ns, WAIT).await;
}
