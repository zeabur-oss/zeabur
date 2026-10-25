//! Objects written as YAML, against an in-memory cluster: the plan dry-runs each object as the
//! person and says which document a refusal is about; the apply creates them in dependency order
//! and, refused half-way, names what it created; a Secret's values never come back.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::Router;
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_api::settings::{DomainSettings, Settings};
    use zeabur_core::problem::KubernetesStatus;
    use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
    use zeabur_kube::k8s_openapi::api::core::v1::{Namespace, Service};
    use zeabur_kube::k8s_openapi::api::networking::v1::Ingress;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects, ServedResource, WriteBody};

    use crate::common::{app, app_with, send};

    const DEPLOYMENT: &str = "apiVersion: apps/v1
kind: Deployment
metadata:
  name: api
spec:
  selector:
    matchLabels: {app: api}
  template:
    metadata:
      labels: {app: api}
    spec:
      containers:
        - name: api
          image: example/api:1
          envFrom:
            - secretRef: {name: api-env}
";
    const SERVICE: &str = "apiVersion: v1
kind: Service
metadata:
  name: api
spec:
  selector: {app: api}
  ports:
    - port: 80
";
    const SECRET: &str = "apiVersion: v1
kind: Secret
metadata:
  name: api-env
stringData:
  TOKEN: s3cr3t-t0ken
";

    fn cluster() -> MemoryCluster {
        let namespace: Namespace = serde_json::from_value(json!({"metadata": {"name": "shop",
            "labels": {"zeabur.com/project": "true"}}}))
        .unwrap();
        let web: Deployment = serde_json::from_value(json!({
            "metadata": {"name": "web", "namespace": "shop", "resourceVersion": "7"},
            "spec": {"selector": {"matchLabels": {"app": "web"}},
                     "template": {"metadata": {"labels": {"app": "web"}},
                                  "spec": {"containers": [{"name": "web", "image": "web:1"}]}}}
        }))
        .unwrap();
        MemoryCluster::new(State {
            identity: Identity {
                username: "alice".to_owned(),
                groups: Vec::new(),
            },
            namespaces: vec![namespace],
            objects: BTreeMap::from([(
                "shop".to_owned(),
                NamespaceObjects {
                    namespace: "shop".to_owned(),
                    deployments: vec![web],
                    ..NamespaceObjects::default()
                },
            )]),
            ..State::default()
        })
    }

    fn creates(documents: &[&str]) -> Value {
        json!({"changes": documents.iter().map(|y| json!({"op": "create", "yaml": y})).collect::<Vec<_>>()})
    }

    async fn plan(app: &Router, request: &Value) -> (StatusCode, Value) {
        send(app, "POST", "/api/v1/projects/shop/plan", Some(request)).await
    }

    async fn apply(app: &Router, planned: &Value) -> (StatusCode, Value) {
        send(
            app,
            "POST",
            "/api/v1/projects/shop/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await
    }

    fn refuse(code: u16, reason: &str, message: &str) -> KubernetesStatus {
        KubernetesStatus::new(code, reason, message)
    }

    /// Given a Deployment, a Service and a Secret written in that order, the plan dry-runs each
    /// as the person, lists them Secret first, each naming its document, hides the Secret's value
    /// and says what the Deployment misses; the apply creates them in that order with the value.
    #[tokio::test]
    async fn objects_are_planned_by_dry_run_and_created_in_dependency_order() {
        let cluster = cluster();
        let app = app(&cluster);
        let (status, planned) = plan(&app, &creates(&[DEPLOYMENT, SERVICE, SECRET])).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        assert_eq!(
            cluster.inspect(|s| (s.writes.len(), s.dry_runs)),
            Some((0, 3))
        );
        let order: Vec<(&str, u64)> = planned["objects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| {
                (
                    o["target"]["kind"].as_str().unwrap(),
                    o["change"].as_u64().unwrap(),
                )
            })
            .collect();
        assert_eq!(order, [("Secret", 2), ("Service", 1), ("Deployment", 0)]);
        assert!(!planned.to_string().contains("s3cr3t"), "{planned}");
        assert_eq!(
            planned["objects"][0]["write"]["body"]["stringData"]["TOKEN"],
            "(hidden)"
        );
        let rules: Vec<&str> = planned["gaps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g["rule"].as_str().unwrap())
            .collect();
        assert!(rules.contains(&"resource-limits"), "{rules:?}");
        assert!(rules.contains(&"health-checks"), "{rules:?}");

        let (status, applied) = apply(&app, &planned).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        let names: Vec<&str> = writes.iter().map(|w| w.target.kind.as_str()).collect();
        assert_eq!(names, ["Secret", "Service", "Deployment"]);
        let WriteBody::Create(secret) = &writes[0].body else {
            panic!("a creation is a POST");
        };
        assert_eq!(secret["stringData"]["TOKEN"], "s3cr3t-t0ken");
        assert_eq!(secret["metadata"]["namespace"], "shop");
    }

    /// The "Docker image" form sends objects, not YAML, so nothing asks discovery: a Deployment,
    /// its Service, an Ingress asking cert-manager for HTTPS, and a volume claim are planned and
    /// created claim first, Ingress last, with the annotation and `tls` block as sent.
    #[tokio::test]
    async fn a_service_from_an_image_is_created_without_discovery() {
        let cluster = cluster();
        let app = app(&cluster);
        let labels = json!({"app.kubernetes.io/name": "site"});
        let objects = [
            json!({"apiVersion": "apps/v1", "kind": "Deployment",
                   "metadata": {"name": "site", "labels": labels},
                   "spec": {"replicas": 1, "strategy": {"type": "Recreate"},
                            "selector": {"matchLabels": labels},
                            "template": {"metadata": {"labels": labels},
                                         "spec": {"containers": [{"name": "site", "image": "ghcr.io/acme/site:1.4",
                                                                  "ports": [{"containerPort": 8080}]}]}}}}),
            json!({"apiVersion": "v1", "kind": "Service", "metadata": {"name": "site", "labels": labels},
                   "spec": {"selector": labels, "ports": [{"name": "http", "port": 8080, "targetPort": 8080}]}}),
            json!({"apiVersion": "networking.k8s.io/v1", "kind": "Ingress",
                   "metadata": {"name": "site", "labels": labels,
                                "annotations": {"cert-manager.io/cluster-issuer": "letsencrypt"}},
                   "spec": {"tls": [{"hosts": ["site-shop.apps.example.com"], "secretName": "site-tls"}],
                            "rules": [{"host": "site-shop.apps.example.com", "http": {"paths": [{
                                "path": "/", "pathType": "Prefix",
                                "backend": {"service": {"name": "site", "port": {"number": 8080}}}}]}}]}}),
            json!({"apiVersion": "v1", "kind": "PersistentVolumeClaim", "metadata": {"name": "site-data", "labels": labels},
                   "spec": {"accessModes": ["ReadWriteOnce"], "resources": {"requests": {"storage": "1Gi"}}}}),
        ];
        let request = json!({"changes": objects.iter().map(|o| json!({"op": "create", "object": o})).collect::<Vec<_>>()});
        let (status, planned) = plan(&app, &request).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        let (status, applied) = apply(&app, &planned).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        let kinds: Vec<&str> = writes.iter().map(|w| w.target.kind.as_str()).collect();
        assert_eq!(
            kinds,
            ["PersistentVolumeClaim", "Service", "Deployment", "Ingress"]
        );
        let WriteBody::Create(ingress) = &writes[3].body else {
            panic!("a creation is a POST");
        };
        assert_eq!(
            ingress["metadata"]["annotations"]["cert-manager.io/cluster-issuer"],
            "letsencrypt"
        );
        assert_eq!(ingress["spec"]["tls"][0]["secretName"], "site-tls");
        assert_eq!(ingress["metadata"]["namespace"], "shop");
    }

    /// A hostname added to a service whose Ingress has no HTTPS yet gets it from the chart's
    /// `ClusterIssuer`: the apply patches in cert-manager's annotation and a `tls` entry whose
    /// Secret is named after the Ingress, as a new service's is, and the plan says so.
    #[tokio::test]
    async fn a_domain_added_to_an_ingress_gets_https_from_the_charts_issuer() {
        let cluster = cluster();
        let service: Service = serde_json::from_value(json!({
            "metadata": {"name": "web", "namespace": "shop"},
            "spec": {"selector": {"app": "web"}, "ports": [{"port": 8080}]}
        }))
        .unwrap();
        let ingress: Ingress = serde_json::from_value(json!({
            "metadata": {"name": "web", "namespace": "shop", "resourceVersion": "3"},
            "spec": {"rules": [{"host": "web.example.com", "http": {"paths": [{
                "path": "/", "pathType": "Prefix",
                "backend": {"service": {"name": "web", "port": {"number": 8080}}}}]}}]}
        }))
        .unwrap();
        cluster.update(|s| {
            let objects = s.objects.get_mut("shop").unwrap();
            objects.services.push(service);
            objects.ingresses.push(ingress);
        });
        let settings = Settings {
            domains: DomainSettings {
                suffixes: Vec::new(),
                cluster_issuer: Some("letsencrypt".to_owned()),
            },
            ..Settings::default()
        };
        let app = app_with(&cluster, settings);
        let request = json!({"changes": [{"op": "addDomain",
            "workload": {"kind": "Deployment", "name": "web"}, "hostname": "www.example.com"}]});
        let (status, planned) = plan(&app, &request).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        assert_eq!(
            planned["warnings"],
            json!([{"code": "httpsCertificate", "hostname": "www.example.com", "issuer": "letsencrypt"}])
        );
        let (status, applied) = apply(&app, &planned).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let ingress = cluster
            .inspect(|s| s.objects["shop"].ingresses[0].clone())
            .unwrap();
        let ingress = serde_json::to_value(ingress).unwrap();
        assert_eq!(
            ingress["metadata"]["annotations"],
            json!({"cert-manager.io/cluster-issuer": "letsencrypt"})
        );
        assert_eq!(
            ingress["spec"]["tls"],
            json!([{"hosts": ["www.example.com"], "secretName": "web-tls"}])
        );
    }

    /// A create the person's rights do not allow is refused at the plan's dry run, at its
    /// document, with what was denied.
    #[tokio::test]
    async fn a_create_the_rights_refuse_is_refused_at_the_plan() {
        let cluster = cluster();
        cluster.update(|s| {
            s.forbidden_writes.insert("Service/api".to_owned());
        });
        let app = app(&cluster);
        let (status, problem) = plan(&app, &creates(&[DEPLOYMENT, SERVICE])).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(problem["at"]["change"], 1, "{problem}");
        assert_eq!(cluster.inspect(|s| s.writes.len()), Some(0));
    }

    /// A quota's refusal at the dry run is `quota-exceeded`, at its document.
    #[tokio::test]
    async fn a_create_over_a_quota_is_refused_at_the_plan() {
        let cluster = cluster();
        cluster.update(|s| {
            s.admission_refusals.insert(
                "Deployment/api".to_owned(),
                refuse(
                    403,
                    "Forbidden",
                    "deployments.apps \"api\" is forbidden: exceeded quota: count, requested: \
                     count/deployments.apps=1, used: count/deployments.apps=1, limited: \
                     count/deployments.apps=1",
                ),
            );
        });
        let app = app(&cluster);
        let (status, problem) = plan(&app, &creates(&[SERVICE, DEPLOYMENT])).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(problem["kind"], "quota-exceeded");
        assert_eq!(problem["at"]["change"], 1);
    }

    /// A policy that quotes a Secret's value in its refusal does not pass the value on, as
    /// written or as base64.
    #[tokio::test]
    async fn a_refusal_never_quotes_a_secret_value() {
        let cluster = cluster();
        cluster.update(|s| {
            s.admission_refusals.insert(
                "Secret/api-env".to_owned(),
                refuse(
                    400,
                    "BadRequest",
                    "admission webhook \"secrets.example.com\" denied the request: TOKEN \
                     s3cr3t-t0ken (czNjcjN0LXQwa2Vu) is a known leaked token",
                ),
            );
        });
        let app = app(&cluster);
        let (status, problem) = plan(&app, &creates(&[SECRET])).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(problem["kind"], "policy-denied");
        assert_eq!(problem["at"]["change"], 0);
        let text = problem.to_string();
        assert!(!text.contains("s3cr3t"), "{text}");
        assert!(!text.contains("czNjcjN0LXQwa2Vu"), "{text}");
    }

    /// Refused after some objects were written, the apply names them and the document refused;
    /// deleting them is a plan like any other.
    #[tokio::test]
    async fn a_partial_apply_names_what_it_created_for_the_cleanup() {
        let cluster = cluster();
        cluster.update(|s| {
            s.write_refusals.insert(
                "Deployment/api".to_owned(),
                refuse(403, "Forbidden", "exceeded quota: count"),
            );
        });
        let app = app(&cluster);
        let (status, planned) = plan(&app, &creates(&[DEPLOYMENT, SERVICE, SECRET])).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        let (status, problem) = apply(&app, &planned).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(problem["at"]["change"], 0);
        let applied: Vec<(&str, &str)> = problem["applied"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| (a["kind"].as_str().unwrap(), a["name"].as_str().unwrap()))
            .collect();
        assert_eq!(applied, [("Secret", "api-env"), ("Service", "api")]);

        let changes: Vec<Value> = problem["applied"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| {
                let api_version = a["apiVersion"].as_str().unwrap();
                let (group, version) = api_version.rsplit_once('/').unwrap_or(("", api_version));
                json!({"op": "delete", "name": a["name"], "resourceVersion": a["resourceVersion"],
                       "resource": {"group": group, "version": version, "resource": a["plural"]}})
            })
            .collect();
        let (status, cleanup) = plan(&app, &json!({"changes": changes})).await;
        assert_eq!(status, StatusCode::OK, "{cleanup}");
        let (status, deleted) = apply(&app, &cleanup).await;
        assert_eq!(status, StatusCode::OK, "{deleted}");
        let (_, again) = plan(&app, &creates(&[SECRET])).await;
        assert!(again["token"].is_string(), "the Secret is gone: {again}");
    }

    /// A name that exists is refused, not updated, at the document's `metadata.name`.
    #[tokio::test]
    async fn a_name_that_exists_is_refused() {
        let cluster = cluster();
        let app = app(&cluster);
        let web = DEPLOYMENT.replace("name: api\n", "name: web\n");
        let (status, problem) = plan(&app, &creates(&[SERVICE, &web])).await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
        assert_eq!(
            problem["at"],
            json!({"change": 1, "field": "metadata.name"})
        );
        assert_eq!(cluster.inspect(|s| s.dry_runs), Some(0));
    }

    /// What the planner refuses names its document, and its field or line.
    #[tokio::test]
    async fn a_refused_document_is_named_with_its_field_or_line() {
        let cluster = cluster();
        let app = app(&cluster);
        for (yaml, kind, at) in [
            (
                "apiVersion: v1\nkind: Namespace\nmetadata:\n  name: other\n",
                "invalid",
                json!({"change": 1, "field": "kind"}),
            ),
            (
                "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: a\n  namespace: prod\n",
                "invalid",
                json!({"change": 1, "field": "metadata.namespace"}),
            ),
            (
                "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: a\n   data: x\n",
                "bad-request",
                json!({"change": 1}),
            ),
        ] {
            let (status, problem) = plan(&app, &creates(&[SERVICE, yaml])).await;
            assert!(status.is_client_error(), "{problem}");
            assert_eq!(problem["kind"], kind, "{problem}");
            assert_eq!(problem["at"]["change"], at["change"], "{problem}");
            assert_eq!(problem["at"].get("field"), at.get("field"), "{problem}");
            if kind == "bad-request" {
                assert!(problem["at"]["line"].as_u64().is_some(), "{problem}");
            }
        }
        assert_eq!(cluster.inspect(|s| s.dry_runs), Some(0));
    }

    fn certificates() -> ServedResource {
        ServedResource {
            verbs: ["create", "delete", "get", "list"]
                .map(str::to_owned)
                .to_vec(),
            ..ServedResource::readable("cert-manager.io", "v1", "certificates", "Certificate", true)
        }
    }

    /// A custom resource the cluster serves is created at the plural discovery names.
    #[tokio::test]
    async fn a_custom_resource_is_created() {
        let cluster = cluster();
        cluster.update(|s| s.served.push(certificates()));
        let app = app(&cluster);
        let yaml = "apiVersion: cert-manager.io/v1\nkind: Certificate\nmetadata:\n  name: shop\n\
                    spec:\n  secretName: shop-tls\n";
        let (status, planned) = plan(&app, &creates(&[yaml])).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        assert_eq!(planned["objects"][0]["target"]["plural"], "certificates");
        let (status, applied) = apply(&app, &planned).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
    }

    /// The kinds a person may create: namespaced, not generated, each with a hint; the cluster's
    /// own kinds named apart.
    #[tokio::test]
    async fn the_creatable_kinds_come_from_discovery() {
        let cluster = cluster();
        cluster.update(|s| s.served.push(certificates()));
        let app = app(&cluster);
        let (status, list) = send(&app, "GET", "/api/v1/projects/shop/creatable", None).await;
        assert_eq!(status, StatusCode::OK, "{list}");
        let kinds: Vec<&str> = list["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k["kind"].as_str().unwrap())
            .collect();
        assert!(kinds.contains(&"Deployment"), "{kinds:?}");
        assert!(kinds.contains(&"Certificate"), "{kinds:?}");
        assert!(!kinds.contains(&"Event"), "{kinds:?}");
        assert!(!kinds.contains(&"Namespace"), "{kinds:?}");
        let certificate = list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|k| k["kind"] == "Certificate")
            .unwrap();
        assert_eq!(certificate["custom"], true);
        assert_eq!(certificate["access"], "allowed");
        assert!(
            list["clusterScoped"]
                .as_array()
                .unwrap()
                .contains(&json!({"group": "", "kind": "Namespace"}))
        );
    }

    /// A template this build carries answers with its id; any other is not found.
    #[tokio::test]
    async fn only_a_carried_template_is_found() {
        let cluster = cluster();
        let app = app(&cluster);
        let (status, template) =
            send(&app, "GET", "/api/v1/cluster/templates/error-pages", None).await;
        assert_eq!(status, StatusCode::OK, "{template}");
        assert_eq!(template["id"], "error-pages");
        let (status, problem) =
            send(&app, "GET", "/api/v1/cluster/templates/no-such-app", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    }

    fn serve(cluster: &MemoryCluster, group: &str, resource: &str, kind: &str, namespace: &str) {
        cluster.update(|s| {
            s.served.push(ServedResource {
                verbs: ["create", "get", "list"].map(str::to_owned).to_vec(),
                ..ServedResource::readable(group, "v1", resource, kind, true)
            });
            s.namespaces
                .push(serde_json::from_value(json!({"metadata": {"name": namespace}})).unwrap());
        });
    }

    fn formats(guide: &Value) -> Vec<&str> {
        guide["formats"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["format"].as_str().unwrap())
            .collect()
    }

    /// An object form is offered only when the cluster serves its controller.
    #[tokio::test]
    async fn an_install_form_needs_its_controller() {
        let cluster = cluster();
        let app = app(&cluster);
        let path = "/api/v1/cluster/components/databases/cloudnative-pg/install";
        let (status, guide) = send(&app, "GET", path, None).await;
        assert_eq!(status, StatusCode::OK, "{guide}");
        assert_eq!(formats(&guide), ["helm"]);

        serve(
            &cluster,
            "helm.toolkit.fluxcd.io",
            "helmreleases",
            "HelmRelease",
            "flux-system",
        );
        let (_, guide) = send(&app, "GET", path, None).await;
        assert_eq!(formats(&guide), ["helm", "flux"]);
        let flux = &guide["formats"][1];
        assert_eq!(flux["namespace"], "flux-system");
        serve(
            &cluster,
            "source.toolkit.fluxcd.io",
            "helmrepositories",
            "HelmRepository",
            "flux-system",
        );
        let creates: Vec<Value> = flux["objects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|y| json!({"op": "create", "yaml": y}))
            .collect();
        let path = "/api/v1/projects/flux-system/plan";
        let (status, planned) = send(&app, "POST", path, Some(&json!({"changes": creates}))).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
    }

    /// A component that is only charts is installed as k3s `HelmChart`s in `kube-system`,
    /// planned there like any other object.
    #[tokio::test]
    async fn a_component_is_installed_as_helm_charts_in_kube_system() {
        let cluster = cluster();
        serve(
            &cluster,
            "helm.cattle.io",
            "helmcharts",
            "HelmChart",
            "kube-system",
        );
        let app = app(&cluster);
        let path = "/api/v1/cluster/components/databases/cloudnative-pg/install";
        let (_, guide) = send(&app, "GET", path, None).await;
        assert_eq!(guide.get("blocked"), None, "{guide}");
        let k3s = &guide["formats"][1];
        assert_eq!(k3s["format"], "k3s");
        let objects = k3s["objects"].as_array().unwrap();
        assert!(
            objects
                .first()
                .and_then(Value::as_str)
                .is_some_and(|y| y.contains("kind: HelmChart"))
        );
        let request = json!({"changes": objects.iter().map(|y| json!({"op": "create", "yaml": y})).collect::<Vec<_>>()});
        let (status, planned) = send(
            &app,
            "POST",
            "/api/v1/projects/kube-system/plan",
            Some(&request),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        let (status, applied) = send(
            &app,
            "POST",
            "/api/v1/projects/kube-system/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert_eq!(cluster.inspect(|s| s.writes.len()), Some(objects.len()));
    }
}
