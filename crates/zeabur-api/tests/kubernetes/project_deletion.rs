//! Deleting a project on a real cluster, as the person: a project with a database goes away, the
//! database object first, then the Namespace, as `kubectl delete namespace` does. The person's
//! Role in the namespace names the Namespace itself, which kube-apiserver authorises as a request
//! in that namespace.
//!
//! Where CloudNativePG is not installed (the bare profile), a stand-in for its `Cluster` CRD is
//! made and deleted with the sandbox.

use std::time::Duration;

use axum::http::StatusCode;
use kube::api::{Api, DynamicObject, Patch, PatchParams, PostParams};
use kube::core::GroupVersionKind;
use kube::discovery::ApiResource;
use serde_json::json;
use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
use zeabur_test_support::sandbox::Sandbox;

use crate::common::{app, plan_and_apply};

const CRD: &str = "clusters.postgresql.cnpg.io";
/// Long enough for the namespace controller to empty a namespace.
const WAIT: Duration = Duration::from_secs(180);

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
             "verbs": ["get", "list", "delete"]},
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
    clusters
        .create(&PostParams::default(), &database)
        .await
        .unwrap();

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

    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        match namespaces.get_opt(&ns).await.unwrap() {
            None => break,
            Some(left) => assert!(
                tokio::time::Instant::now() < deadline,
                "the namespace is still there: {:?}",
                left.status
            ),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
