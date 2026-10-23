//! cert-manager's `ClusterIssuers`: a DNS-01 issuer is planned and applied with its token in a
//! Secret of cert-manager's namespace and in no answer, and without cert-manager's controller
//! the plan says why it cannot.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{NamespaceObjects, ServedResource};

    use crate::common::{app, send};

    const TOKEN: &str = "cf-very-secret-token";

    fn cluster(controller: bool) -> MemoryCluster {
        let deployment: Deployment = serde_json::from_value(json!({
            "metadata": {"name": "cert-manager", "namespace": "cert-manager"},
            "spec": {"selector": {"matchLabels": {"app": "cert-manager"}},
                     "template": {"metadata": {"labels": {"app": "cert-manager"}},
                                  "spec": {"containers": [{"name": "cert-manager-controller",
                                      "image": "quay.io/jetstack/cert-manager-controller:v1.21.2",
                                      "args": ["--v=2",
                                               "--cluster-resource-namespace=$(POD_NAMESPACE)"]}]}}}
        }))
        .unwrap();
        let objects = if controller {
            BTreeMap::from([(
                "cert-manager".to_owned(),
                NamespaceObjects {
                    namespace: "cert-manager".to_owned(),
                    deployments: vec![deployment],
                    ..NamespaceObjects::default()
                },
            )])
        } else {
            BTreeMap::new()
        };
        MemoryCluster::new(State {
            served: vec![ServedResource::readable(
                "cert-manager.io",
                "v1",
                "clusterissuers",
                "ClusterIssuer",
                false,
            )],
            objects,
            ..State::default()
        })
    }

    fn request() -> Value {
        json!({"name": "letsencrypt-dns", "email": "ops@example.test",
               "dns": {"provider": "cloudflare", "apiToken": TOKEN}})
    }

    #[tokio::test]
    async fn a_dns01_issuer_keeps_its_token_in_a_secret_and_out_of_every_answer() {
        let cluster = cluster(true);
        let app = app(&cluster);
        let (status, planned) = send(
            &app,
            "POST",
            "/api/v1/cluster/issuers/plan",
            Some(&request()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        assert!(!planned.to_string().contains(TOKEN), "{planned}");
        let writes: Vec<(&str, &str, &str)> = planned["writes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| {
                (
                    w["action"].as_str().unwrap(),
                    w["kind"].as_str().unwrap(),
                    w["namespace"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            writes,
            [
                ("create", "Secret", "cert-manager"),
                ("create", "ClusterIssuer", "")
            ]
        );
        let (status, done) = send(
            &app,
            "POST",
            "/api/v1/cluster/issuers/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{done}");
        let token = cluster
            .inspect(|s| {
                s.secret_values["cert-manager/letsencrypt-dns-dns-credentials"]["api-token"].clone()
            })
            .unwrap();
        assert_eq!(token, TOKEN.as_bytes());

        let (status, found) = send(&app, "GET", "/api/v1/cluster/issuers", None).await;
        assert_eq!(status, StatusCode::OK, "{found}");
        assert!(!found.to_string().contains(TOKEN), "{found}");
        assert_eq!(found["issuers"][0]["name"], "letsencrypt-dns");
        assert_eq!(found["issuers"][0]["solvers"], json!(["dns01:cloudflare"]));
        assert_eq!(found["issuers"][0]["wildcards"], true);
        assert_eq!(
            found["issuers"][0]["dns"],
            json!({"provider": "cloudflare", "apiToken": ""})
        );
    }

    #[tokio::test]
    async fn without_the_controller_the_plan_says_why() {
        let app = app(&cluster(false));
        let (status, problem) = send(
            &app,
            "POST",
            "/api/v1/cluster/issuers/plan",
            Some(&request()),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");
        assert!(
            problem["detail"]
                .as_str()
                .unwrap()
                .contains("no cert-manager controller"),
            "{problem}"
        );
    }

    #[tokio::test]
    async fn a_new_issuer_needs_its_token() {
        let app = app(&cluster(true));
        let request = json!({"name": "letsencrypt-dns",
                             "dns": {"provider": "cloudflare", "apiToken": ""}});
        let (status, problem) =
            send(&app, "POST", "/api/v1/cluster/issuers/plan", Some(&request)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");
    }
}
