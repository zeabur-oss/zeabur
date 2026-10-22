//! A service's route trace against an in-memory cluster and fixed DNS answers: every hop from DNS
//! to a ready pod, the first failure as a diagnosis, and the plan that fixes a wrong target port.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use axum::Router;
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_api::access::SingleCluster;
    use zeabur_api::resolve::Fixed;
    use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
    use zeabur_kube::k8s_openapi::api::core::v1::{Namespace, Pod, Secret, Service};
    use zeabur_kube::k8s_openapi::api::networking::v1::Ingress;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{DynamicObject, NamespaceObjects, SecretSummary, ServedResource};

    use crate::common::send;

    const TRACE: &str = "/api/v1/projects/shop/services/deployment/web/trace";

    fn from<T: serde::de::DeserializeOwned>(value: Value) -> T {
        serde_json::from_value(value).unwrap()
    }

    fn shop() -> NamespaceObjects {
        NamespaceObjects {
            namespace: "shop".to_owned(),
            deployments: vec![from(json!({
                "metadata": {"name": "web", "namespace": "shop"},
                "spec": {"replicas": 1, "selector": {"matchLabels": {"app": "web"}},
                    "template": {"metadata": {"labels": {"app": "web"}},
                        "spec": {"containers": [{"name": "web", "image": "nginx:1.29",
                            "ports": [{"containerPort": 8080}]}]}}},
                "status": {"replicas": 1, "readyReplicas": 1}
            }))],
            pods: vec![from::<Pod>(json!({
                "metadata": {"name": "web-1", "namespace": "shop", "labels": {"app": "web"}},
                "spec": {"nodeName": "node-a", "containers": [{"name": "web", "image": "nginx:1.29"}]},
                "status": {"phase": "Running", "conditions": [{"type": "Ready", "status": "True"}],
                    "containerStatuses": [{"name": "web", "image": "nginx:1.29", "imageID": "",
                        "ready": true, "restartCount": 0, "state": {"running": {}}}]}
            }))],
            services: vec![from::<Service>(json!({
                "metadata": {"name": "web", "namespace": "shop", "resourceVersion": "7"},
                "spec": {"selector": {"app": "web"}, "ports": [{"port": 80, "targetPort": 3000}]}
            }))],
            secrets: vec![SecretSummary {
                secret: from::<Secret>(
                    json!({"metadata": {"name": "web-tls", "namespace": "shop"},
                    "type": "kubernetes.io/tls"}),
                ),
                key_count: 2,
            }],
            ingresses: vec![from::<Ingress>(json!({
                "metadata": {"name": "web", "namespace": "shop"},
                "spec": {"ingressClassName": "traefik",
                    "tls": [{"hosts": ["shop.example.test"], "secretName": "web-tls"}],
                    "rules": [{"host": "shop.example.test", "http": {"paths": [{"path": "/",
                        "pathType": "Prefix",
                        "backend": {"service": {"name": "web", "port": {"number": 80}}}}]}}]},
                "status": {"loadBalancer": {"ingress": [{"ip": "203.0.113.10"}]}}
            }))],
            ..NamespaceObjects::default()
        }
    }

    fn system(ready: i32) -> NamespaceObjects {
        NamespaceObjects {
            namespace: "kube-system".to_owned(),
            deployments: vec![from::<Deployment>(json!({
                "metadata": {"name": "traefik", "namespace": "kube-system",
                    "labels": {"app.kubernetes.io/name": "traefik"}},
                "spec": {"replicas": 1, "selector": {}, "template": {}},
                "status": {"replicas": 1, "readyReplicas": ready}
            }))],
            ..NamespaceObjects::default()
        }
    }

    fn cluster(ready: i32) -> MemoryCluster {
        let class: DynamicObject = from(json!({
            "apiVersion": "networking.k8s.io/v1", "kind": "IngressClass",
            "metadata": {"name": "traefik"},
            "spec": {"controller": "traefik.io/ingress-controller"}
        }));
        MemoryCluster::new(State {
            namespaces: vec![from::<Namespace>(json!({"metadata": {"name": "shop",
                "labels": {"zeabur.com/project": "true"}}}))],
            objects: BTreeMap::from([
                ("shop".to_owned(), shop()),
                ("kube-system".to_owned(), system(ready)),
            ]),
            dynamic: vec![class],
            ..State::default()
        })
    }

    fn app(cluster: &MemoryCluster, dns: &[(&str, &str)]) -> Router {
        let answers = dns
            .iter()
            .map(|(name, address)| ((*name).to_owned(), vec![(*address).to_owned()]))
            .collect();
        zeabur_api::router_with_resolver(
            Arc::new(SingleCluster(Arc::new(cluster.clone()))),
            Arc::new(Fixed(answers)),
        )
    }

    fn hops(route: &Value) -> Vec<(String, String)> {
        route["hops"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| {
                (
                    h["step"].as_str().unwrap().to_owned(),
                    h["status"].as_str().unwrap().to_owned(),
                )
            })
            .collect()
    }

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect()
    }

    #[tokio::test]
    async fn a_wrong_target_port_is_found_on_every_way_and_its_plan_fixes_it() {
        let cluster = cluster(1);
        let app = app(&cluster, &[("shop.example.test", "203.0.113.10")]);
        let (status, trace) = send(&app, "GET", TRACE, None).await;
        assert_eq!(status, StatusCode::OK, "{trace}");
        assert_eq!(trace["id"], "projects/shop/services/deployment/web/trace");

        let domain = &trace["routes"][0];
        assert_eq!(domain["kind"], "domain");
        assert_eq!(domain["address"], "shop.example.test");
        assert_eq!(domain["status"], "failed");
        assert_eq!(
            hops(domain),
            pairs(&[
                ("dns", "ok"),
                ("address", "ok"),
                ("controller", "ok"),
                ("certificate", "unknown"),
                ("route", "ok"),
                ("service", "ok"),
                ("targetPort", "failed"),
                ("ready", "ok"),
            ])
        );
        let target = &domain["hops"][6];
        assert_eq!(target["expected"], "8080");
        assert_eq!(target["saw"], "3000");

        let private = &trace["routes"][1];
        assert_eq!(private["kind"], "private");
        assert_eq!(private["address"], "web.shop.svc.cluster.local");
        assert_eq!(
            hops(private),
            pairs(&[("service", "ok"), ("targetPort", "failed"), ("ready", "ok")])
        );

        assert_eq!(trace["diagnoses"].as_array().unwrap().len(), 2, "{trace}");
        assert_eq!(
            trace["diagnoses"][0]["cause"]["key"],
            "diagnosis.unreachable.cause.targetPort"
        );

        let fix = &domain["fix"];
        assert_eq!(fix["changes"][0]["op"], "edit");
        let (status, planned) = send(&app, "POST", "/api/v1/projects/shop/plan", Some(fix)).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        let after = &planned["objects"][0]["after"];
        assert_eq!(after["spec"]["ports"][0]["targetPort"], 8080, "{planned}");
        let (status, applied) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let (_, again) = send(&app, "GET", TRACE, None).await;
        assert_eq!(again["routes"][0]["status"], "unknown", "{again}");
        assert_eq!(again["diagnoses"], json!([]));
    }

    #[tokio::test]
    async fn a_domain_that_does_not_resolve_here_is_unknown_and_a_controller_that_is_down_fails() {
        let cluster = cluster(0);
        let app = app(&cluster, &[]);
        let (status, trace) = send(&app, "GET", TRACE, None).await;
        assert_eq!(status, StatusCode::OK, "{trace}");
        let domain = &trace["routes"][0];
        assert_eq!(domain["hops"][0]["status"], "unknown");
        assert_eq!(domain["hops"][2]["status"], "failed");
        assert_eq!(domain["hops"][2]["saw"], "0/1");
        let causes: Vec<&str> = trace["diagnoses"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["cause"]["key"].as_str().unwrap())
            .collect();
        assert_eq!(
            causes,
            [
                "diagnosis.unreachable.cause.controllerDown",
                "diagnosis.unreachable.cause.targetPort"
            ]
        );
        assert_eq!(trace["diagnoses"][0]["fault"]["origin"], "component");
    }

    #[tokio::test]
    async fn a_refused_list_leaves_its_hop_unknown_and_is_named() {
        let cluster = cluster(1);
        cluster.update(|s| {
            s.forbidden_resources.insert("deployments.apps".to_owned());
        });
        let app = app(&cluster, &[("shop.example.test", "198.51.100.7")]);
        let (status, trace) = send(&app, "GET", TRACE, None).await;
        assert_eq!(status, StatusCode::OK, "{trace}");
        let domain = &trace["routes"][0];
        assert_eq!(domain["hops"][1]["status"], "unknown");
        assert_eq!(
            domain["hops"][1]["message"]["key"],
            "trace.address.elsewhere"
        );
        assert_eq!(domain["hops"][2]["status"], "unknown");
        assert_eq!(trace["unreadable"][0]["resource"], "deployments.apps");
    }

    #[tokio::test]
    async fn a_domain_names_its_dns_target_and_what_its_certificate_waits_for() {
        let cluster = cluster(1);
        cluster.update(|s| {
            s.served.push(ServedResource::readable(
                "cert-manager.io",
                "v1",
                "certificates",
                "Certificate",
                true,
            ));
            s.served.push(ServedResource::readable(
                "acme.cert-manager.io",
                "v1",
                "challenges",
                "Challenge",
                true,
            ));
            s.dynamic.push(from(json!({
                "apiVersion": "cert-manager.io/v1", "kind": "Certificate",
                "metadata": {"name": "web-tls", "namespace": "shop"},
                "spec": {"secretName": "web-tls", "dnsNames": ["shop.example.test"]},
                "status": {"conditions": [
                    {"type": "Ready", "status": "False", "reason": "DoesNotExist"},
                    {"type": "Issuing", "status": "True", "reason": "DoesNotExist"}]}
            })));
            s.dynamic.push(from(json!({
                "apiVersion": "acme.cert-manager.io/v1", "kind": "Challenge",
                "metadata": {"name": "web-tls-1-2-3", "namespace": "shop"},
                "spec": {"dnsName": "shop.example.test", "type": "HTTP-01"},
                "status": {"state": "pending",
                    "reason": "Waiting for HTTP-01 challenge propagation: connection refused"}
            })));
        });
        let app = app(&cluster, &[("shop.example.test", "198.51.100.7")]);
        let (status, trace) = send(&app, "GET", TRACE, None).await;
        assert_eq!(status, StatusCode::OK, "{trace}");
        assert_eq!(trace["ingressAddresses"], json!(["203.0.113.10"]));
        let domain = &trace["routes"][0]["domain"];
        assert_eq!(domain["target"], json!(["203.0.113.10"]), "{trace}");
        assert_eq!(domain["dns"], "elsewhere");
        assert_eq!(domain["resolved"], json!(["198.51.100.7"]));
        assert_eq!(domain["certificate"]["state"], "issuing");
        assert_eq!(domain["certificate"]["name"], "web-tls");
        assert_eq!(
            domain["certificate"]["message"],
            "Waiting for HTTP-01 challenge propagation: connection refused"
        );
        assert!(trace["routes"][1].get("domain").is_none());
    }
}
