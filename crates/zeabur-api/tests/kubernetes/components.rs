//! cert-manager installed from Zeabur Catalog's API by a cluster administrator, step by step
//! through helm-controller, until its `ClusterIssuers` are Ready and its health checks pass; then
//! removed, leaving only what the catalog lists as leftovers. Needs K3s (helm-controller) and the
//! network: the chart, the images and Let's Encrypt.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::http::StatusCode;
use axum::http::request::Parts;
use kube::api::{Api, DynamicObject};
use kube::core::{ApiResource, GroupVersionKind};
use serde_json::{Value, json};
use zeabur_api::Options;
use zeabur_api::access::ClusterAccess;
use zeabur_core::problem::Problem;
use zeabur_kube::Cluster;
use zeabur_kube::k8s_openapi::api::rbac::v1::ClusterRoleBinding;
use zeabur_test_support::sandbox::{PERSON, Sandbox, create};

use crate::common::send;

/// Long enough for helm-controller to pull the chart and the nodes the images.
const WAIT: Duration = Duration::from_secs(600);

struct Admin(Arc<dyn Cluster>);

impl ClusterAccess for Admin {
    fn for_request(&self, _parts: &Parts) -> Result<Arc<dyn Cluster>, Box<Problem>> {
        Ok(Arc::clone(&self.0))
    }
}

/// A person whom a `ClusterRoleBinding` to `cluster-admin` makes a cluster administrator.
async fn administrator() -> Sandbox {
    let sandbox = Sandbox::new(
        "zeabur-components",
        json!([{"apiGroups": [""], "resources": ["configmaps"], "verbs": ["get"]}]),
    )
    .await;
    let name = sandbox.namespace().to_owned();
    sandbox.delete_cluster_role(&name);
    create(
        &Api::<ClusterRoleBinding>::all(sandbox.admin().clone()),
        json!({"metadata": {"name": name},
               "roleRef": {"apiGroup": "rbac.authorization.k8s.io", "kind": "ClusterRole",
                           "name": "cluster-admin"},
               "subjects": [{"kind": "ServiceAccount", "name": PERSON, "namespace": name}]}),
    )
    .await;
    sandbox
}

async fn call(app: &Router, verb: &str, action: &str, version: &str) -> (StatusCode, Value) {
    send(
        app,
        "POST",
        &format!("/api/v1/cluster/components/cert-manager/{verb}"),
        Some(&json!({"action": action, "catalogVersion": version})),
    )
    .await
}

/// Applies the plan until it is done; any failed step fails the test.
async fn apply_all(app: &Router, action: &str) -> Value {
    let started = Instant::now();
    // The ClusterRoleBinding takes a moment to take effect.
    let (mut status, mut plan) = call(app, "plan", action, "").await;
    while status == StatusCode::FORBIDDEN && started.elapsed() < Duration::from_secs(30) {
        tokio::time::sleep(Duration::from_secs(1)).await;
        (status, plan) = call(app, "plan", action, "").await;
    }
    assert_eq!(status, StatusCode::OK, "{plan}");
    let version = plan["catalogVersion"].as_str().unwrap().to_owned();
    loop {
        let (status, applied) = call(app, "apply", action, &version).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let plan = &applied["plan"];
        let failed = plan["steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["state"] == "failed");
        assert!(!failed, "{plan}");
        if plan["done"] == true {
            return plan.clone();
        }
        assert!(started.elapsed() < WAIT, "not done in time: {plan}");
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

fn kind(group: &str, kind: &str, plural: &str) -> ApiResource {
    ApiResource::from_gvk_with_plural(&GroupVersionKind::gvk(group, "v1", kind), plural)
}

#[tokio::test]
#[ignore = "needs a disposable K3s cluster with network"]
async fn an_administrator_installs_cert_manager_and_removes_it() {
    let sandbox = administrator().await;
    let app = zeabur_api::router_with(
        Arc::new(Admin(Arc::new(sandbox.user().clone()))),
        Options::default(),
    );
    let admin = sandbox.admin().clone();

    let installed = apply_all(&app, "install").await;
    let issuers: Api<DynamicObject> = Api::all_with(
        admin.clone(),
        &kind("cert-manager.io", "ClusterIssuer", "clusterissuers"),
    );
    let names: Vec<&str> = installed["steps"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|s| s["writes"].as_array().unwrap())
        .filter(|w| w["kind"] == "ClusterIssuer")
        .map(|w| w["name"].as_str().unwrap())
        .collect();
    assert!(!names.is_empty(), "{installed}");
    // The plan needs an issuer to exist; cert-manager reports it Ready a moment later.
    for name in &names {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let issuer = issuers.get(name).await.unwrap();
            let ready = issuer.data["status"]["conditions"]
                .as_array()
                .is_some_and(|c| {
                    c.iter()
                        .any(|c| c["type"] == "Ready" && c["status"] == "True")
                });
            if ready {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "{name} is not Ready: {}",
                issuer.data
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    apply_all(&app, "remove").await;
    let charts: Api<DynamicObject> = Api::namespaced_with(
        admin.clone(),
        "kube-system",
        &kind("helm.cattle.io", "HelmChart", "helmcharts"),
    );
    assert!(charts.get_opt("cert-manager").await.unwrap().is_none());
    for name in &names {
        assert!(
            issuers.get_opt(name).await.unwrap().is_none(),
            "{name} stays"
        );
    }
    // helm-controller uninstalls the release once the chart is gone; the CRDs stay.
    let deployments: Api<zeabur_kube::k8s_openapi::api::apps::v1::Deployment> =
        Api::namespaced(admin.clone(), "cert-manager");
    let started = Instant::now();
    while !deployments
        .list(&kube::api::ListParams::default())
        .await
        .unwrap()
        .items
        .is_empty()
    {
        assert!(started.elapsed() < WAIT, "cert-manager's Deployments stay");
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    let crds: Api<zeabur_kube::k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition> =
        Api::all(admin);
    assert!(
        crds.get_opt("clusterissuers.cert-manager.io")
            .await
            .unwrap()
            .is_some()
    );
}
