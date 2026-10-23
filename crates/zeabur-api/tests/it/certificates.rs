//! A domain's own certificate: the plan checks it and hides its key, the apply stores it in a
//! `kubernetes.io/tls` Secret, and the dates endpoint answers without the key.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use serde_json::json;
    use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
    use zeabur_kube::k8s_openapi::api::core::v1::{Namespace, Service};
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects, WriteBody};

    use crate::common::{app, send};

    const CERT: &str = include_str!("../../src/projects/writes/testdata/shop.crt");
    const KEY: &str = include_str!("../../src/projects/writes/testdata/shop.key");

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
        let service: Service = serde_json::from_value(json!({
            "metadata": {"name": "web", "namespace": "shop"},
            "spec": {"selector": {"app": "web"}, "ports": [{"port": 8080}]}
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
                    services: vec![service],
                    ..NamespaceObjects::default()
                },
            )]),
            ..State::default()
        })
    }

    fn key_line() -> &'static str {
        KEY.lines().nth(1).unwrap()
    }

    /// The key goes into the Secret the apply writes and never into an answer.
    #[tokio::test]
    async fn an_own_certificate_is_stored_and_its_key_never_comes_back() {
        let cluster = cluster();
        let app = app(&cluster);
        let request = json!({"changes": [{"op": "addDomain",
            "workload": {"kind": "Deployment", "name": "web"}, "hostname": "shop.example.test",
            "certificate": {"from": "own", "certificate": CERT, "key": KEY}}]});
        let (status, planned) =
            send(&app, "POST", "/api/v1/projects/shop/plan", Some(&request)).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        assert!(!planned.to_string().contains(key_line()));
        assert_eq!(
            planned["warnings"],
            json!([{"code": "ownCertificate", "hostname": "shop.example.test",
                    "notAfter": "2126-01-01T00:00:00Z"}])
        );
        let (status, applied) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert!(!applied.to_string().contains(key_line()));
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        let secret = writes
            .iter()
            .find(|w| w.target.kind == "Secret")
            .and_then(|w| {
                if let WriteBody::Create(body) = &w.body {
                    Some(body.clone())
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(secret["type"], "kubernetes.io/tls");
        assert_eq!(secret["metadata"]["name"], "web-shop-example-test-cert");
        let stored = BTreeMap::from([
            ("tls.crt".to_owned(), CERT.as_bytes().to_vec()),
            ("tls.key".to_owned(), KEY.as_bytes().to_vec()),
        ]);
        cluster.update(|s| {
            s.secret_values
                .insert("shop/web-shop-example-test-cert".to_owned(), stored);
        });

        let (status, dates) = send(
            &app,
            "GET",
            "/api/v1/projects/shop/certificates/web-shop-example-test-cert",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{dates}");
        assert_eq!(
            dates,
            json!({"secret": "web-shop-example-test-cert",
                   "notBefore": "2026-01-01T00:00:00Z", "notAfter": "2126-01-01T00:00:00Z"})
        );
        assert!(!dates.to_string().contains(key_line()));
    }

    /// A key that is not the certificate's is refused at its field, without echoing it.
    #[tokio::test]
    async fn a_mismatched_key_is_refused_at_its_field() {
        let cluster = cluster();
        let app = app(&cluster);
        let other = include_str!("../../src/projects/writes/testdata/other.key");
        let request = json!({"changes": [{"op": "addDomain",
            "workload": {"kind": "Deployment", "name": "web"}, "hostname": "shop.example.test",
            "certificate": {"from": "own", "certificate": CERT, "key": other}}]});
        let (status, problem) =
            send(&app, "POST", "/api/v1/projects/shop/plan", Some(&request)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");
        assert_eq!(problem["at"], json!({"change": 0, "field": "key"}));
        assert!(!problem.to_string().contains(other.lines().nth(1).unwrap()));
    }
}
