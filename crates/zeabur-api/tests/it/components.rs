//! Installing and removing a catalog component through a plan: cert-manager's chart, then its
//! `ClusterIssuers` once its CRDs are served, each step written as the person with server-side
//! apply, readiness from helm-controller's Job and the health checks, and removal in reverse;
//! and the refusals: someone who is not a cluster administrator, a plan of an older catalog
//! release, a release someone else installed, a component another one needs, a cluster without
//! helm-controller and a catalog Zeabur cannot reach.

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::Router;
    use axum::http::StatusCode;
    use futures_util::future::BoxFuture;
    use serde_json::{Value, json};
    use zeabur_api::cluster::catalog::{Fetch, Fetched};
    use zeabur_kube::k8s_openapi::api::core::v1::Node;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{ClusterFacts, DynamicObject, Identity, ServedResource, WriteBody};

    use crate::common::send;
    use crate::support::TestApi;

    const TOKEN: &str = "eyJhbGciOi.token-of-alice.sig";
    const FIXTURES: &[(&str, &[u8])] = &[
        (
            "cert-manager",
            include_bytes!("../../../zeabur-catalog/tests/api/cert-manager.json"),
        ),
        (
            "cloudnative-pg",
            include_bytes!("../../../zeabur-catalog/tests/api/cloudnative-pg.json"),
        ),
        (
            "traefik",
            include_bytes!("../../../zeabur-catalog/tests/api/traefik.json"),
        ),
    ];

    /// Zeabur Catalog v0.5.3 with the components of `FIXTURES`, or no answer at all.
    struct Api {
        down: bool,
    }

    impl Fetch for Api {
        fn get(
            &self,
            url: String,
            _etag: Option<String>,
        ) -> BoxFuture<'_, Result<Fetched, String>> {
            Box::pin(async move {
                if self.down {
                    return Err("dns error: no such host".to_owned());
                }
                let bytes = if url.ends_with("/catalog.json") {
                    let mut index: Value = serde_json::from_slice(include_bytes!(
                        "../../../zeabur-catalog/tests/api/catalog.json"
                    ))
                    .unwrap();
                    index["components"]
                        .as_array_mut()
                        .unwrap()
                        .retain(|c| FIXTURES.iter().any(|(id, _)| c["id"] == *id));
                    serde_json::to_vec(&index).unwrap()
                } else {
                    let id = url.rsplit('/').next().unwrap().trim_end_matches(".json");
                    FIXTURES.iter().find(|(f, _)| *f == id).unwrap().1.to_vec()
                };
                Ok(Fetched::Body { bytes, etag: None })
            })
        }
    }

    fn served(group: &str, resource: &str, kind: &str, namespaced: bool) -> ServedResource {
        ServedResource {
            group: group.to_owned(),
            version: "v1".to_owned(),
            resource: resource.to_owned(),
            kind: kind.to_owned(),
            namespaced,
            verbs: vec!["get".to_owned(), "list".to_owned(), "patch".to_owned()],
        }
    }

    /// A one-node K3s cluster with its packaged Traefik and helm-controller.
    fn cluster() -> MemoryCluster {
        let node: Node = serde_json::from_value(json!({
            "metadata": {"name": "n1", "labels": {"kubernetes.io/arch": "amd64"}},
        }))
        .unwrap();
        MemoryCluster::new(State {
            identity: Identity {
                username: "alice".to_owned(),
                groups: vec!["system:masters".to_owned()],
            },
            facts: ClusterFacts {
                version: "v1.37.2+k3s1".to_owned(),
                nodes: Some(1),
                ingress_controllers: ["traefik.io/ingress-controller".to_owned()].into(),
                k3s_helm_charts: [("kube-system/traefik".to_owned(), "traefik".to_owned())].into(),
                ..ClusterFacts::default()
            },
            nodes: vec![node],
            served: vec![served("helm.cattle.io", "helmcharts", "HelmChart", true)],
            ..State::default()
        })
    }

    fn app(cluster: &MemoryCluster, down: bool) -> Router {
        TestApi::new(cluster)
            .signed_in(TOKEN)
            .with_catalog(Arc::new(Api { down }))
            .router()
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

    fn states(plan: &Value) -> Vec<String> {
        plan["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                format!(
                    "{} {}",
                    s["release"].as_str().unwrap(),
                    s["state"].as_str().unwrap()
                )
            })
            .collect()
    }

    fn dynamic(cluster: &MemoryCluster, kind: &str, name: &str) -> Option<Value> {
        cluster
            .inspect(|s| {
                s.dynamic
                    .iter()
                    .find(|o| {
                        o.types.as_ref().is_some_and(|t| t.kind == kind)
                            && o.metadata.name.as_deref() == Some(name)
                    })
                    .map(|o| serde_json::to_value(o).unwrap())
            })
            .flatten()
    }

    fn put(cluster: &MemoryCluster, object: Value) {
        let object: DynamicObject = serde_json::from_value(object).unwrap();
        cluster.update(|s| {
            s.dynamic
                .retain(|o| o.metadata.name != object.metadata.name || o.types != object.types);
            s.dynamic.push(object);
        });
    }

    /// What helm-controller and cert-manager do once the chart is written: the Job completes,
    /// the CRDs and the Deployments come.
    fn chart_installed(cluster: &MemoryCluster) {
        let mut chart = dynamic(cluster, "HelmChart", "cert-manager").unwrap();
        chart["status"] = json!({"jobName": "helm-install-cert-manager"});
        put(cluster, chart);
        let job = json!({
            "apiVersion": "batch/v1",
            "kind": "Job",
            "metadata": {"name": "helm-install-cert-manager", "namespace": "kube-system",
                         "creationTimestamp": "2030-01-01T00:00:00Z"},
            "status": {"conditions": [
                {"type": "SuccessCriteriaMet", "status": "True"},
                {"type": "Complete", "status": "True"},
            ]},
        });
        let deployment = |name: &str| {
            json!({
                "apiVersion": "apps/v1",
                "kind": "Deployment",
                "metadata": {"name": name, "namespace": "cert-manager", "generation": 1},
                "spec": {"replicas": 1, "selector": {}, "template": {}},
                "status": {"observedGeneration": 1, "replicas": 1, "updatedReplicas": 1,
                           "readyReplicas": 1, "availableReplicas": 1,
                           "conditions": [{"type": "Available", "status": "True"}]},
            })
        };
        cluster.update(|s| {
            let kube_system = s.objects.entry("kube-system".to_owned()).or_default();
            kube_system.namespace = "kube-system".to_owned();
            kube_system.jobs.push(serde_json::from_value(job).unwrap());
            let ns = s.objects.entry("cert-manager".to_owned()).or_default();
            ns.namespace = "cert-manager".to_owned();
            for name in [
                "cert-manager",
                "cert-manager-webhook",
                "cert-manager-cainjector",
            ] {
                ns.deployments
                    .push(serde_json::from_value(deployment(name)).unwrap());
            }
            s.served.push(served(
                "cert-manager.io",
                "clusterissuers",
                "ClusterIssuer",
                false,
            ));
            s.served.push(served(
                "cert-manager.io",
                "certificates",
                "Certificate",
                true,
            ));
        });
    }

    #[tokio::test]
    async fn an_administrator_installs_cert_manager_step_by_step_and_removes_it() {
        let cluster = cluster();
        let app = app(&cluster, false);

        let (status, plan) = call(&app, "plan", "install", "").await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        assert_eq!(plan["catalogVersion"], "0.5.3");
        assert_eq!(plan["components"], json!(["cert-manager"]));
        assert_eq!(states(&plan), ["cert-manager pending", "issuers pending"]);
        let chart = &plan["steps"][0]["writes"][1];
        assert_eq!(chart["kind"], "HelmChart");
        assert_eq!(chart["namespace"], "kube-system");
        assert!(
            chart["yaml"]
                .as_str()
                .unwrap()
                .contains("oci://quay.io/jetstack/charts/cert-manager")
        );
        assert_eq!(plan["checks"][0]["state"], "pending");
        assert_eq!(plan["done"], false);
        assert_eq!(
            cluster.inspect(|s| s.writes.len()),
            Some(0),
            "a plan writes nothing"
        );

        // The first step: the namespace and the chart, dry run first.
        let (status, plan) = call(&app, "apply", "install", "0.5.3").await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        assert_eq!(states(&plan), ["cert-manager waiting", "issuers pending"]);
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        let written: Vec<&str> = writes.iter().map(|w| w.target.kind.as_str()).collect();
        assert_eq!(written, ["Namespace", "HelmChart"]);
        assert!(writes.iter().all(|w| matches!(w.body, WriteBody::Apply(_))));
        assert_eq!(cluster.inspect(|s| s.dry_runs), Some(2));

        // Nothing new is written while helm-controller works.
        call(&app, "apply", "install", "0.5.3").await;
        let (_, plan) = call(&app, "apply", "install", "0.5.3").await;
        assert_eq!(states(&plan), ["cert-manager waiting", "issuers pending"]);

        chart_installed(&cluster);
        let (_, plan) = call(&app, "apply", "install", "0.5.3").await;
        assert_eq!(states(&plan), ["cert-manager ready", "issuers ready"]);
        assert!(dynamic(&cluster, "ClusterIssuer", "letsencrypt").is_some());

        // An issuer is ready once cert-manager says so.
        let mut issuer = dynamic(&cluster, "ClusterIssuer", "letsencrypt").unwrap();
        issuer["status"] = json!({"conditions": [{"type": "Ready", "status": "False"}]});
        put(&cluster, issuer.clone());
        let (_, plan) = call(&app, "plan", "install", "").await;
        assert_eq!(states(&plan), ["cert-manager ready", "issuers waiting"]);
        issuer["status"] = json!({"conditions": [{"type": "Ready", "status": "True"}]});
        put(&cluster, issuer);
        let (_, plan) = call(&app, "plan", "install", "").await;
        assert_eq!(plan["done"], true, "{plan}");
        assert!(
            plan["checks"]
                .as_array()
                .unwrap()
                .iter()
                .all(|c| c["state"] == "ready")
        );

        // Removal: the issuers, then the chart; the namespace and leftovers stay.
        let (status, plan) = call(&app, "plan", "remove", "").await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        assert_eq!(states(&plan), ["issuers pending", "cert-manager pending"]);
        assert!(
            plan["leftovers"]
                .to_string()
                .contains("letsencrypt-account-key")
        );
        let (_, plan) = call(&app, "apply", "remove", "0.5.3").await;
        assert_eq!(states(&plan), ["issuers ready", "cert-manager pending"]);
        let (_, plan) = call(&app, "apply", "remove", "0.5.3").await;
        assert_eq!(plan["done"], true, "{plan}");
        assert!(dynamic(&cluster, "HelmChart", "cert-manager").is_none());
        let namespaces = cluster.inspect(|s| s.namespaces.len()).unwrap();
        assert_eq!(namespaces, 1, "the namespace stays");
    }

    #[tokio::test]
    async fn an_apply_of_an_older_catalog_release_is_refused() {
        let cluster = cluster();
        let (status, problem) = call(&app(&cluster, false), "apply", "install", "0.5.2").await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
        assert_eq!(cluster.inspect(|s| s.writes.len()), Some(0));
    }

    #[tokio::test]
    async fn only_cluster_administrators_install() {
        let cluster = cluster();
        cluster.update(|s| s.forbid_everything = true);
        let (status, problem) = call(&app(&cluster, false), "plan", "install", "").await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
    }

    #[tokio::test]
    async fn a_release_someone_else_installed_is_not_taken_over() {
        let cluster = cluster();
        cluster.update(|s| {
            s.facts
                .helm_releases
                .insert("cert-manager/cert-manager".to_owned());
        });
        let (status, problem) = call(&app(&cluster, false), "plan", "install", "").await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
        assert!(
            problem["detail"]
                .as_str()
                .unwrap()
                .contains("not by Zeabur")
        );

        let (status, problem) = call(&app(&cluster, false), "plan", "remove", "").await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
        assert!(
            problem["detail"]
                .as_str()
                .unwrap()
                .contains("did not install")
        );
    }

    #[tokio::test]
    async fn a_certificate_outside_cert_managers_namespace_keeps_it() {
        let cluster = cluster();
        let app = app(&cluster, false);
        call(&app, "apply", "install", "0.5.3").await;
        chart_installed(&cluster);
        put(
            &cluster,
            json!({
                "apiVersion": "cert-manager.io/v1",
                "kind": "Certificate",
                "metadata": {"name": "shop-tls", "namespace": "shop"},
            }),
        );
        let (status, problem) = call(&app, "plan", "remove", "").await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
        assert!(
            problem["detail"]
                .as_str()
                .unwrap()
                .contains("Certificate shop/shop-tls")
        );
    }

    #[tokio::test]
    async fn a_cluster_without_helm_controller_is_told_so() {
        let cluster = cluster();
        cluster.update(|s| s.served.clear());
        let (status, problem) = call(&app(&cluster, false), "plan", "install", "").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");
        assert!(
            problem["detail"]
                .as_str()
                .unwrap()
                .contains("helm-controller")
        );
    }

    #[tokio::test]
    async fn an_unreachable_catalog_is_named() {
        let cluster = cluster();
        let app = app(&cluster, true);
        let (status, problem) = call(&app, "plan", "install", "").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{problem}");
        assert!(
            problem["detail"]
                .as_str()
                .unwrap()
                .contains("oss-catalog.zeabur.com")
        );

        let (status, catalog) = send(&app, "GET", "/api/v1/cluster/catalog", None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(catalog["version"].is_null());
        assert_eq!(catalog["error"], "dns error: no such host");
    }
}
