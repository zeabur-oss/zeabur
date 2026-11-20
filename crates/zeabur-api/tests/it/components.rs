//! Installing and removing a catalog component through a plan: cert-manager's chart, then its
//! `ClusterIssuers` once its CRDs are served, each step written as the person with server-side
//! apply, readiness from helm-controller's Job and the health checks, and removal in reverse;
//! and the refusals: someone who is not a cluster administrator, a plan of an older catalog
//! release, a release someone else installed, a component another one needs (not its leftover
//! CRDs), a cluster without
//! helm-controller and a catalog Zeabur cannot reach. Manifest releases: the System Upgrade
//! Controller from its files, Falco without the policy report CRDs another tool installed, and a
//! file that does not match its digest. A component that adds values to K3s's packaged Traefik
//! writes its `HelmChartConfig`, unless the person wrote one, and writes it again when its values
//! differ.

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::Router;
    use axum::http::StatusCode;
    use futures_util::future::BoxFuture;
    use serde_json::{Value, json};
    use sha2::{Digest as _, Sha256};
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
            "falco",
            include_bytes!("../../../zeabur-catalog/tests/api/falco.json"),
        ),
        (
            "system-upgrade-controller",
            include_bytes!("../../../zeabur-catalog/tests/api/system-upgrade-controller.json"),
        ),
        (
            "traefik",
            include_bytes!("../../../zeabur-catalog/tests/api/traefik.json"),
        ),
        (
            "velero",
            include_bytes!("../../../zeabur-catalog/tests/api/velero.json"),
        ),
    ];

    const SUC: &str =
        "https://github.com/rancher/system-upgrade-controller/releases/download/v0.20.2";
    const POLICY_REPORTS: &str = "https://raw.githubusercontent.com/kubernetes-retired/\
        wg-policy-prototypes/ed0ca5a1c7788055074c7500915b76fc4518ed33/policy-report/crd/\
        wgpolicyk8s.io/v1alpha2/wgpolicyk8s.io_";

    fn crd(name: &str) -> String {
        format!(
            "apiVersion: apiextensions.k8s.io/v1\nkind: CustomResourceDefinition\nmetadata:\n  name: {name}\n"
        )
    }

    /// Short stand-ins for the files of the manifest releases; the catalog's digests are
    /// replaced with theirs.
    fn files() -> Vec<(String, String)> {
        vec![
            (format!("{SUC}/crd.yaml"), crd("plans.upgrade.cattle.io")),
            (
                format!("{SUC}/system-upgrade-controller.yaml"),
                "apiVersion: v1\nkind: Namespace\nmetadata:\n  name: system-upgrade\n---\n\
                 apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  \
                 name: system-upgrade-controller\n  namespace: system-upgrade\nspec:\n  \
                 selector: {}\n  template: {}\n"
                    .to_owned(),
            ),
            (
                format!("{POLICY_REPORTS}policyreports.yaml"),
                crd("policyreports.wgpolicyk8s.io"),
            ),
            (
                format!("{POLICY_REPORTS}clusterpolicyreports.yaml"),
                crd("clusterpolicyreports.wgpolicyk8s.io"),
            ),
            (
                "https://github.com/kubernetes-sigs/gateway-api/releases/download/v1.6.3/\
                 standard-install.yaml"
                    .to_owned(),
                crd("httproutes.gateway.networking.k8s.io"),
            ),
        ]
    }

    /// Zeabur Catalog v0.5.3 with the components of `FIXTURES`, or no answer at all; `tampered`
    /// answers each manifest file with other bytes than the catalog's digest. With
    /// `contributes`, cert-manager adds [`BOUNCER`] to Traefik.
    #[derive(Default)]
    struct Api {
        down: bool,
        tampered: bool,
        contributes: bool,
    }

    const BOUNCER: &str = "ports:\n  web:\n    middlewares:\n    - bouncer\n";

    impl Fetch for Api {
        #[expect(
            clippy::literal_string_with_formatting_args,
            reason = "`{version}` is the catalog's URL placeholder, not a format argument"
        )]
        fn get(
            &self,
            url: String,
            _etag: Option<String>,
        ) -> BoxFuture<'_, Result<Fetched, String>> {
            Box::pin(async move {
                if self.down {
                    return Err("dns error: no such host".to_owned());
                }
                if let Some((_, text)) = files().into_iter().find(|(u, _)| *u == url) {
                    let mut bytes = text.into_bytes();
                    if self.tampered {
                        bytes.extend(b"# changed");
                    }
                    return Ok(Fetched::Body { bytes, etag: None });
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
                    let bytes = FIXTURES.iter().find(|(f, _)| *f == id).unwrap().1;
                    let mut file: Value = serde_json::from_slice(bytes).unwrap();
                    for release in file["declaration"]["releases"].as_array_mut().unwrap() {
                        let version = release["version"].as_str().unwrap_or_default().to_owned();
                        let Some(urls) = release["urls"].as_array() else {
                            continue;
                        };
                        let digests: Vec<String> = urls
                            .iter()
                            .map(|u| {
                                let u = u.as_str().unwrap().replace("{version}", &version);
                                let (_, text) = files().into_iter().find(|(f, _)| *f == u).unwrap();
                                format!("sha256:{}", hex::encode(Sha256::digest(text)))
                            })
                            .collect();
                        release["urlDigests"] = json!(digests);
                    }
                    if self.contributes && id == "cert-manager" {
                        file["declaration"]["contributes"] = json!([{
                            "target": {"component": "traefik", "release": "traefik"},
                            "values": BOUNCER,
                        }]);
                    }
                    serde_json::to_vec(&file).unwrap()
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
            .with_catalog(Arc::new(Api {
                down,
                ..Api::default()
            }))
            .router()
    }

    async fn call(app: &Router, verb: &str, action: &str, version: &str) -> (StatusCode, Value) {
        call_for(app, "cert-manager", verb, action, version).await
    }

    async fn call_for(
        app: &Router,
        id: &str,
        verb: &str,
        action: &str,
        version: &str,
    ) -> (StatusCode, Value) {
        send(
            app,
            "POST",
            &format!("/api/v1/cluster/components/{id}/{verb}"),
            Some(&json!({"action": action, "catalogVersion": version})),
        )
        .await
    }

    /// An install of Velero with `values`.
    async fn velero(app: &Router, verb: &str, values: &Value) -> (StatusCode, Value) {
        send(
            app,
            "POST",
            &format!("/api/v1/cluster/components/velero/{verb}"),
            Some(&json!({"action": "install", "catalogVersion": "0.5.3", "values": values})),
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

        // The first step: the namespace and the chart, dry run first; the answer names what
        // each write left.
        let (status, applied) = call(&app, "apply", "install", "0.5.3").await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert_eq!(
            states(&applied["plan"]),
            ["cert-manager waiting", "issuers pending"]
        );
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        let written: Vec<&str> = writes.iter().map(|w| w.target.kind.as_str()).collect();
        assert_eq!(written, ["Namespace", "HelmChart"]);
        assert!(writes.iter().all(|w| matches!(w.body, WriteBody::Apply(_))));
        assert_eq!(cluster.inspect(|s| s.dry_runs), Some(2));
        let chart = dynamic(&cluster, "HelmChart", "cert-manager").unwrap();
        let answered = &applied["written"][1];
        assert_eq!(answered["kind"], "HelmChart");
        assert_eq!(answered["namespace"], "kube-system");
        assert_eq!(answered["uid"], chart["metadata"]["uid"]);
        assert_eq!(
            answered["resourceVersion"],
            chart["metadata"]["resourceVersion"]
        );
        assert_eq!(applied["written"].as_array().unwrap().len(), writes.len());

        // Nothing new is written while helm-controller works.
        call(&app, "apply", "install", "0.5.3").await;
        let (_, applied) = call(&app, "apply", "install", "0.5.3").await;
        assert_eq!(
            states(&applied["plan"]),
            ["cert-manager waiting", "issuers pending"]
        );

        chart_installed(&cluster);
        let (_, applied) = call(&app, "apply", "install", "0.5.3").await;
        assert_eq!(
            states(&applied["plan"]),
            ["cert-manager ready", "issuers ready"]
        );
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
        let (_, applied) = call(&app, "apply", "remove", "0.5.3").await;
        assert_eq!(
            states(&applied["plan"]),
            ["issuers ready", "cert-manager pending"]
        );
        let (_, applied) = call(&app, "apply", "remove", "0.5.3").await;
        assert_eq!(applied["plan"]["done"], true, "{applied}");
        assert!(dynamic(&cluster, "HelmChart", "cert-manager").is_none());
        let namespaces = cluster.inspect(|s| s.namespaces.len()).unwrap();
        assert_eq!(namespaces, 1, "the namespace stays");
    }

    #[tokio::test]
    async fn an_install_asks_for_values_and_keeps_the_ones_zeabur_made() {
        let cluster = cluster();
        let app = app(&cluster, false);
        let backup_key = || {
            cluster
                .inspect(|s| s.secret_values.get("velero/velero-backup-key").cloned())
                .flatten()
        };

        let (status, plan) = velero(&app, "plan", &json!({})).await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        let bucket = plan["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["name"] == "bucket")
            .unwrap();
        assert_eq!(bucket["state"], "missing");
        let (status, refused) = velero(&app, "apply", &json!({})).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
        assert_eq!(cluster.inspect(|s| s.writes.len()), Some(0));

        let values = json!({"velero": {
            "bucket": "backups",
            "s3Url": "http://s3.test:9000",
            "credentials": "[default]\naws_access_key_id=a\naws_secret_access_key=b\n",
        }});
        let (status, applied) = velero(&app, "apply", &values).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert_eq!(states(&applied["plan"]), ["velero waiting"]);
        let made = backup_key().expect("Zeabur makes the SSE-C key");

        // The next apply reads the key back instead of making another, so the step goes on.
        let (_, applied) = velero(&app, "apply", &values).await;
        assert_eq!(states(&applied["plan"]), ["velero waiting"], "{applied}");
        assert_eq!(backup_key(), Some(made));
        let key = applied["plan"]["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["name"] == "backupKey")
            .unwrap();
        assert_eq!(key["state"], "kept");
        assert!(
            key.get("value").is_none(),
            "a Secret's value stays on the server"
        );
    }

    #[tokio::test]
    async fn changed_values_are_written_after_a_failed_release() {
        let cluster = cluster();
        let app = app(&cluster, false);
        let credentials = || {
            cluster
                .inspect(|s| s.secret_values.get("velero/velero-credentials").cloned())
                .flatten()
        };
        let values = |secret: &str| {
            json!({"velero": {
                "bucket": "backups",
                "s3Url": "https://s3.test",
                "credentials": format!("[default]\naws_access_key_id=a\naws_secret_access_key={secret}\n"),
            }})
        };
        let (status, applied) = velero(&app, "apply", &values("old")).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let mut chart = dynamic(&cluster, "HelmChart", "velero").unwrap();
        chart["status"] = json!({"conditions": [
            {"type": "Failed", "status": "True", "message": "the release failed"},
        ]});
        put(&cluster, chart);
        let (_, planned) = velero(&app, "plan", &values("old")).await;
        assert_eq!(states(&planned), ["velero failed"], "{planned}");

        let (status, applied) = velero(&app, "apply", &values("new")).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let written = credentials().expect("the credentials Secret");
        assert!(
            written
                .values()
                .any(|v| String::from_utf8_lossy(v).contains("=new")),
            "{written:?}"
        );
    }

    #[tokio::test]
    async fn a_removal_deletes_only_the_objects_it_found_unchanged() {
        let cluster = cluster();
        let app = app(&cluster, false);
        call(&app, "apply", "install", "0.5.3").await;
        chart_installed(&cluster);
        call(&app, "apply", "install", "0.5.3").await;
        call(&app, "apply", "remove", "0.5.3").await;
        let (_, applied) = call(&app, "apply", "remove", "0.5.3").await;
        assert_eq!(applied["plan"]["done"], true, "{applied}");
        let deletions: Vec<_> = cluster
            .inspect(|s| s.writes.clone())
            .unwrap()
            .into_iter()
            .filter_map(|w| {
                if let WriteBody::Delete(preconditions) = w.body {
                    Some(preconditions)
                } else {
                    None
                }
            })
            .collect();
        assert!(dynamic(&cluster, "HelmChart", "cert-manager").is_none());
        assert!(
            deletions
                .iter()
                .all(|p| p.uid.is_some() && p.resource_version.is_some()),
            "every deletion names the uid and resourceVersion it found: {deletions:?}"
        );
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
    async fn a_component_whose_crds_outlived_its_removal_installs_again() {
        let cluster = cluster();
        cluster.update(|s| {
            s.facts
                .crds
                .insert("certificates.cert-manager.io".to_owned());
        });
        let (status, plan) = call(&app(&cluster, false), "plan", "install", "").await;
        assert_eq!(status, StatusCode::OK, "{plan}");
    }

    #[tokio::test]
    async fn crds_a_removal_left_do_not_stand_in_for_a_needed_component() {
        let cluster = cluster();
        cluster.update(|s| {
            s.facts
                .crds
                .insert("certificates.cert-manager.io".to_owned());
        });
        let (status, plan) = send(
            &app(&cluster, false),
            "POST",
            "/api/v1/cluster/components/cloudnative-pg/plan",
            Some(&json!({"action": "install"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        assert_eq!(
            plan["components"],
            json!(["cert-manager", "cloudnative-pg"])
        );
    }

    #[tokio::test]
    async fn a_component_helm_installed_is_not_installed_again() {
        let cluster = cluster();
        cluster.update(|s| {
            s.facts.workload_labels.push((
                "tools/cert-manager".to_owned(),
                [(
                    "helm.sh/chart".to_owned(),
                    "cert-manager-v1.20.0".to_owned(),
                )]
                .into(),
            ));
        });
        let (status, problem) = call(&app(&cluster, false), "plan", "install", "").await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
        assert!(
            problem["detail"]
                .as_str()
                .unwrap()
                .contains("did not install")
        );
    }

    #[tokio::test]
    async fn a_chart_zeabur_did_not_write_is_not_taken_over() {
        let cluster = cluster();
        put(
            &cluster,
            json!({
                "apiVersion": "helm.cattle.io/v1",
                "kind": "HelmChart",
                "metadata": {"name": "cert-manager", "namespace": "kube-system"},
                "spec": {"chart": "cert-manager"},
            }),
        );
        let (status, problem) = call(&app(&cluster, false), "plan", "install", "").await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
        assert!(
            problem["detail"]
                .as_str()
                .unwrap()
                .contains("HelmChart cert-manager")
        );
    }

    #[tokio::test]
    async fn a_shared_namespace_keeps_its_looser_pod_security_level() {
        let cluster = cluster();
        put(
            &cluster,
            json!({
                "apiVersion": "v1",
                "kind": "Namespace",
                "metadata": {"name": "cert-manager",
                             "labels": {"pod-security.kubernetes.io/enforce": "privileged"}},
            }),
        );
        call(&app(&cluster, false), "apply", "install", "0.5.3").await;
        let namespace = dynamic(&cluster, "Namespace", "cert-manager").unwrap();
        assert_eq!(
            namespace["metadata"]["labels"]["pod-security.kubernetes.io/enforce"],
            "privileged"
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
    async fn an_object_zeabur_wrote_for_another_component_keeps_it_in_any_namespace() {
        let cluster = cluster();
        let app = app(&cluster, false);
        call(&app, "apply", "install", "0.5.3").await;
        chart_installed(&cluster);
        put(
            &cluster,
            json!({
                "apiVersion": "cert-manager.io/v1",
                "kind": "Certificate",
                "metadata": {
                    "name": "webhook-tls",
                    "namespace": "cert-manager",
                    "labels": {"catalog.zeabur.com/component": "cloudnative-pg"},
                },
            }),
        );
        let (status, problem) = call(&app, "plan", "remove", "").await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
        assert!(
            problem["detail"]
                .as_str()
                .unwrap()
                .contains("Certificate cert-manager/webhook-tls")
        );
    }

    #[tokio::test]
    async fn a_component_needing_it_keeps_it_but_its_leftover_crds_do_not() {
        let cluster = cluster();
        let app = app(&cluster, false);
        call(&app, "apply", "install", "0.5.3").await;
        chart_installed(&cluster);
        cluster.update(|s| {
            s.facts
                .crds
                .insert("clusters.postgresql.cnpg.io".to_owned());
        });
        let (status, plan) = call(&app, "plan", "remove", "").await;
        assert_eq!(status, StatusCode::OK, "{plan}");

        cluster.update(|s| {
            s.facts.workload_labels.push((
                "cnpg-system/cloudnative-pg".to_owned(),
                [(
                    "helm.sh/chart".to_owned(),
                    "cloudnative-pg-0.26.0".to_owned(),
                )]
                .into(),
            ));
        });
        let (status, problem) = call(&app, "plan", "remove", "").await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
        assert!(
            problem["detail"]
                .as_str()
                .unwrap()
                .contains("cloudnative-pg needs cert-manager")
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
        assert!(
            problem["detail"]
                .as_str()
                .unwrap()
                .contains("zeaburctl components export")
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

    /// The cluster of [`cluster`], serving CRDs, Deployments and ClusterRoles.
    fn cluster_with_crds() -> MemoryCluster {
        let cluster = cluster();
        cluster.update(|s| {
            s.served.push(served(
                "apiextensions.k8s.io",
                "customresourcedefinitions",
                "CustomResourceDefinition",
                false,
            ));
            s.served
                .push(served("apps", "deployments", "Deployment", true));
            s.served.push(served(
                "rbac.authorization.k8s.io",
                "clusterroles",
                "ClusterRole",
                false,
            ));
        });
        cluster
    }

    /// What the API server does with a CRD once it is written: it serves it.
    fn established(cluster: &MemoryCluster, name: &str) {
        let mut crd = dynamic(cluster, "CustomResourceDefinition", name).unwrap();
        crd["status"] = json!({"conditions": [
            {"type": "NamesAccepted", "status": "True"},
            {"type": "Established", "status": "True"},
        ]});
        put(cluster, crd);
    }

    #[tokio::test]
    async fn a_component_installs_from_manifest_files_and_its_removal_keeps_the_crds() {
        let cluster = cluster_with_crds();
        let app = app(&cluster, false);
        let suc = "system-upgrade-controller";

        let (status, plan) = call_for(&app, suc, "plan", "install", "").await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        assert_eq!(states(&plan), ["crds pending", "controller pending"]);
        let kinds: Vec<&str> = plan["steps"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|s| s["writes"].as_array().unwrap())
            .map(|w| w["kind"].as_str().unwrap())
            .collect();
        assert_eq!(
            kinds,
            ["Namespace", "CustomResourceDefinition", "Deployment"],
            "the controller file's Namespace is written once, by the first step"
        );

        let (_, applied) = call_for(&app, suc, "apply", "install", "0.5.3").await;
        assert_eq!(
            states(&applied["plan"]),
            ["crds waiting", "controller pending"]
        );
        let crd = dynamic(
            &cluster,
            "CustomResourceDefinition",
            "plans.upgrade.cattle.io",
        )
        .unwrap();
        assert_eq!(
            crd["metadata"]["labels"]["catalog.zeabur.com/component"],
            suc
        );
        established(&cluster, "plans.upgrade.cattle.io");
        let (_, applied) = call_for(&app, suc, "apply", "install", "0.5.3").await;
        assert_eq!(
            states(&applied["plan"]),
            ["crds ready", "controller waiting"]
        );
        let deployments = cluster
            .inspect(|s| s.objects.get("system-upgrade").map(|n| n.deployments.len()))
            .flatten();
        assert_eq!(deployments, Some(1));

        // Removal deletes the controller; the CRD stays, as deleting it would delete every Plan.
        let (status, plan) = call_for(&app, suc, "plan", "remove", "").await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        assert_eq!(states(&plan), ["controller pending"]);
        assert_eq!(
            plan["leftovers"],
            json!(["customresourcedefinitions/plans.upgrade.cattle.io"])
        );
        let (_, applied) = call_for(&app, suc, "apply", "remove", "0.5.3").await;
        assert_eq!(applied["plan"]["done"], true, "{applied}");
        assert!(
            dynamic(
                &cluster,
                "CustomResourceDefinition",
                "plans.upgrade.cattle.io"
            )
            .is_some()
        );
    }

    #[tokio::test]
    async fn a_crd_whose_labels_its_controller_replaced_stays_zeaburs() {
        let cluster = cluster_with_crds();
        let app = app(&cluster, false);
        let suc = "system-upgrade-controller";
        call_for(&app, suc, "apply", "install", "0.5.3").await;
        // The controller updates its embedded CRD on start: Zeabur's labels go, its fields stay.
        let mut crd = dynamic(
            &cluster,
            "CustomResourceDefinition",
            "plans.upgrade.cattle.io",
        )
        .unwrap();
        crd["metadata"]["labels"] = json!({});
        crd["metadata"]["managedFields"] = json!([
            {"manager": "zeabur", "operation": "Apply", "apiVersion": "apiextensions.k8s.io/v1"},
        ]);
        put(&cluster, crd);
        established(&cluster, "plans.upgrade.cattle.io");
        let (status, plan) = call_for(&app, suc, "plan", "install", "").await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        assert_eq!(states(&plan), ["crds ready", "controller pending"]);
    }

    #[tokio::test]
    async fn a_release_whose_crds_another_tool_installed_is_left_out() {
        let cluster = cluster_with_crds();
        put(
            &cluster,
            json!({
                "apiVersion": "apiextensions.k8s.io/v1",
                "kind": "CustomResourceDefinition",
                "metadata": {"name": "policyreports.wgpolicyk8s.io",
                             "labels": {"app.kubernetes.io/managed-by": "kyverno"}},
            }),
        );
        let (status, plan) = call_for(&app(&cluster, false), "falco", "plan", "install", "").await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        assert_eq!(
            states(&plan),
            [
                "policy-report-crds pending",
                "alert-readers pending",
                "falco pending"
            ]
        );
        let kept: Vec<(&str, &str)> = plan["steps"][0]["writes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| (w["kind"].as_str().unwrap(), w["name"].as_str().unwrap()))
            .collect();
        assert_eq!(
            kept,
            [("Namespace", "falco")],
            "the release's CRDs are left out, the component's namespace is not"
        );

        // Once Zeabur wrote them, the release stays in the plan.
        let cluster = cluster_with_crds();
        let app = app(&cluster, false);
        let (_, applied) = call_for(&app, "falco", "apply", "install", "0.5.3").await;
        assert_eq!(
            states(&applied["plan"])[0],
            "policy-report-crds waiting",
            "{applied}"
        );
        let (_, plan) = call_for(&app, "falco", "plan", "install", "").await;
        assert_eq!(states(&plan)[0], "policy-report-crds waiting");
    }

    #[tokio::test]
    async fn a_manifest_file_unlike_its_digest_is_refused_with_its_url() {
        let cluster = cluster_with_crds();
        let app = TestApi::new(&cluster)
            .signed_in(TOKEN)
            .with_catalog(Arc::new(Api {
                tampered: true,
                ..Api::default()
            }))
            .router();
        let (status, problem) =
            call_for(&app, "system-upgrade-controller", "plan", "install", "").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");
        assert!(
            problem["detail"]
                .as_str()
                .unwrap()
                .contains(&format!("{SUC}/crd.yaml")),
            "{problem}"
        );
        assert_eq!(cluster.inspect(|s| s.writes.len()), Some(0));
    }

    /// The cluster with Traefik that K3s packages, which takes a `HelmChartConfig`, and an app
    /// whose cert-manager adds values to it.
    fn packaged() -> (MemoryCluster, Router) {
        let cluster = cluster();
        cluster.update(|s| {
            s.facts.k3s_helm_charts.insert(
                "kube-system/traefik".to_owned(),
                "https://%{KUBERNETES_API}%/static/charts/traefik-41.6.101+up41.6.1.tgz".to_owned(),
            );
            s.served.push(served(
                "helm.cattle.io",
                "helmchartconfigs",
                "HelmChartConfig",
                true,
            ));
        });
        let app = TestApi::new(&cluster)
            .signed_in(TOKEN)
            .with_catalog(Arc::new(Api {
                contributes: true,
                ..Api::default()
            }))
            .router();
        (cluster, app)
    }

    /// helm-controller installs the packaged Traefik with its config, after Zeabur's writes.
    fn traefik_installed(cluster: &MemoryCluster) {
        put(
            cluster,
            json!({
                "apiVersion": "helm.cattle.io/v1",
                "kind": "HelmChart",
                "metadata": {"name": "traefik", "namespace": "kube-system"},
                "spec": {"chart": "traefik"},
                "status": {"jobName": "helm-install-traefik"},
            }),
        );
        cluster.update(|s| {
            let job = json!({
                "apiVersion": "batch/v1",
                "kind": "Job",
                "metadata": {"name": "helm-install-traefik", "namespace": "kube-system",
                             "creationTimestamp": "2030-01-01T00:00:00Z"},
                "status": {"conditions": [
                    {"type": "SuccessCriteriaMet", "status": "True"},
                    {"type": "Complete", "status": "True"},
                ]},
            });
            let kube_system = s.objects.get_mut("kube-system").unwrap();
            kube_system.jobs.push(serde_json::from_value(job).unwrap());
        });
    }

    #[tokio::test]
    async fn values_for_packaged_traefik_go_in_its_config_and_leave_with_the_component() {
        let (cluster, app) = packaged();
        let (status, plan) = call(&app, "plan", "install", "").await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        assert_eq!(
            states(&plan),
            ["cert-manager pending", "issuers pending", "traefik pending"]
        );
        assert_eq!(plan["steps"][2]["writes"][0]["kind"], "HelmChartConfig");

        call(&app, "apply", "install", "0.5.3").await;
        chart_installed(&cluster);
        call(&app, "apply", "install", "0.5.3").await;
        let (_, applied) = call(&app, "apply", "install", "0.5.3").await;
        assert_eq!(
            states(&applied["plan"]),
            ["cert-manager ready", "issuers ready", "traefik waiting"]
        );
        let config = dynamic(&cluster, "HelmChartConfig", "traefik").unwrap();
        assert_eq!(config["metadata"]["namespace"], "kube-system");
        assert_eq!(config["spec"]["valuesContent"], BOUNCER);
        assert_eq!(
            config["metadata"]["labels"]["app.kubernetes.io/managed-by"],
            "zeabur"
        );

        // Removal takes the values out of Traefik first, then removes cert-manager.
        let (status, plan) = call(&app, "plan", "remove", "").await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        assert_eq!(
            states(&plan),
            [
                "traefik pending",
                "issuers pending",
                "cert-manager pending",
                "traefik pending"
            ]
        );
        // The config stays, empty, and cert-manager waits until Traefik is installed without
        // the values: its middleware is still in Traefik until then.
        let (_, applied) = call(&app, "apply", "remove", "0.5.3").await;
        let config = dynamic(&cluster, "HelmChartConfig", "traefik").unwrap();
        assert_eq!(config["spec"]["valuesContent"], "");
        assert_eq!(
            states(&applied["plan"]),
            [
                "traefik waiting",
                "issuers pending",
                "cert-manager pending",
                "traefik pending"
            ]
        );
        // helm-controller installs Traefik without the values: cert-manager goes, then the
        // empty config.
        traefik_installed(&cluster);
        let (_, plan) = call(&app, "plan", "remove", "").await;
        assert_eq!(
            states(&plan),
            [
                "traefik ready",
                "issuers pending",
                "cert-manager pending",
                "traefik pending"
            ]
        );
        let (_, applied) = call(&app, "apply", "remove", "0.5.3").await;
        assert_eq!(
            states(&applied["plan"]),
            [
                "traefik ready",
                "issuers ready",
                "cert-manager pending",
                "traefik pending"
            ]
        );
        assert!(dynamic(&cluster, "HelmChartConfig", "traefik").is_some());
        let (_, applied) = call(&app, "apply", "remove", "0.5.3").await;
        assert_eq!(
            states(&applied["plan"]),
            [
                "traefik ready",
                "issuers ready",
                "cert-manager ready",
                "traefik pending"
            ]
        );
        let (status, applied) = call(&app, "apply", "remove", "0.5.3").await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert_eq!(applied["plan"]["done"], true, "{applied}");
        assert!(dynamic(&cluster, "HelmChartConfig", "traefik").is_none());
    }

    #[tokio::test]
    async fn an_installed_config_with_other_values_is_written_again() {
        let (cluster, app) = packaged();
        call(&app, "apply", "install", "0.5.3").await;
        chart_installed(&cluster);
        call(&app, "apply", "install", "0.5.3").await;
        call(&app, "apply", "install", "0.5.3").await;
        // helm-controller installs Traefik with the config.
        traefik_installed(&cluster);
        let (_, plan) = call(&app, "plan", "install", "").await;
        assert_eq!(
            states(&plan),
            ["cert-manager ready", "issuers ready", "traefik ready"]
        );

        // An earlier catalog release added other values.
        let mut config = dynamic(&cluster, "HelmChartConfig", "traefik").unwrap();
        config["spec"]["valuesContent"] = json!("ports: {}\n");
        put(&cluster, config);
        let (_, plan) = call(&app, "plan", "install", "").await;
        assert_eq!(
            states(&plan),
            ["cert-manager ready", "issuers ready", "traefik pending"]
        );
        call(&app, "apply", "install", "0.5.3").await;
        let config = dynamic(&cluster, "HelmChartConfig", "traefik").unwrap();
        assert_eq!(config["spec"]["valuesContent"], BOUNCER);
    }

    #[tokio::test]
    async fn a_helm_chart_config_the_person_wrote_is_left_as_it_is() {
        let (cluster, app) = packaged();
        let theirs = json!({
            "apiVersion": "helm.cattle.io/v1",
            "kind": "HelmChartConfig",
            "metadata": {"name": "traefik", "namespace": "kube-system"},
            "spec": {"valuesContent": "logs:\n  access:\n    enabled: true\n"},
        });
        put(&cluster, theirs.clone());
        let (status, plan) = call(&app, "plan", "install", "").await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        assert_eq!(states(&plan), ["cert-manager pending", "issuers pending"]);
        assert!(plan["warnings"].as_array().unwrap().contains(&json!({
            "kind": "unchanged",
            "component": "traefik",
            "values": BOUNCER,
        })));

        call(&app, "apply", "install", "0.5.3").await;
        chart_installed(&cluster);
        call(&app, "apply", "install", "0.5.3").await;
        let config = dynamic(&cluster, "HelmChartConfig", "traefik").unwrap();
        assert_eq!(config["spec"], theirs["spec"]);
        let (_, plan) = call(&app, "plan", "remove", "").await;
        assert_eq!(states(&plan), ["issuers pending", "cert-manager pending"]);
    }
}
