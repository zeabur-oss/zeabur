//! Adding a node: what `GET /cluster/join` reads as the person, and the bootstrap token Secret a
//! plan creates and deletes in `kube-system`.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use sha2::{Digest as _, Sha256};
    use zeabur_kube::k8s_openapi::api::core::v1::{ConfigMap, Secret};
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{ClusterFacts, Identity, NamespaceObjects, SecretSummary};

    use crate::common::{app, planned_and_applied, send};

    const CA: &str = include_str!("../fixtures/join/root.pem");
    const SECRET: &str = "0123456789abcdef";

    fn node(
        name: &str,
        labels: &Value,
        ip: &str,
        ready: &str,
    ) -> zeabur_kube::k8s_openapi::api::core::v1::Node {
        serde_json::from_value(json!({
            "metadata": {"name": name, "labels": labels},
            "status": {
                "addresses": [{"type": "ExternalIP", "address": "203.0.113.9"},
                              {"type": "InternalIP", "address": ip}],
                "conditions": [{"type": "Ready", "status": ready}]
            }
        }))
        .unwrap()
    }

    fn token(id: &str, expiration: &str) -> (SecretSummary, BTreeMap<String, Vec<u8>>) {
        let secret: Secret = serde_json::from_value(json!({
            "metadata": {"name": format!("bootstrap-token-{id}"), "namespace": "kube-system",
                         "labels": {"app.kubernetes.io/managed-by": "zeabur-installer"}},
            "type": "bootstrap.kubernetes.io/token"
        }))
        .unwrap();
        let values = [
            ("token-id", id),
            ("token-secret", SECRET),
            ("expiration", expiration),
            ("description", "zeabur-installer node add"),
            ("usage-bootstrap-authentication", "true"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.as_bytes().to_vec()))
        .collect();
        (
            SecretSummary {
                secret,
                key_count: 5,
            },
            values,
        )
    }

    fn cluster(version: &str) -> MemoryCluster {
        let root_ca: ConfigMap = serde_json::from_value(json!({
            "metadata": {"name": "kube-root-ca.crt", "namespace": "kube-system"},
            "data": {"ca.crt": CA}
        }))
        .unwrap();
        let other: Secret = serde_json::from_value(json!({
            "metadata": {"name": "rke2-serving", "namespace": "kube-system"},
            "type": "kubernetes.io/tls"
        }))
        .unwrap();
        let (live, live_values) = token("abc123", "2999-01-01T00:00:00Z");
        let (old, old_values) = token("old000", "2001-01-01T00:00:00Z");
        MemoryCluster::new(State {
            identity: Identity {
                username: "alice".to_owned(),
                groups: vec![],
            },
            namespaces: vec![
                serde_json::from_value(json!({"metadata": {"name": "kube-system"}})).unwrap(),
            ],
            nodes: vec![
                node(
                    "server-1",
                    &json!({"node-role.kubernetes.io/control-plane": "true",
                           "node-role.kubernetes.io/etcd": "true"}),
                    "10.0.0.5",
                    "True",
                ),
                node("worker-1", &json!({}), "10.0.0.6", "True"),
            ],
            objects: BTreeMap::from([(
                "kube-system".to_owned(),
                NamespaceObjects {
                    namespace: "kube-system".to_owned(),
                    config_maps: vec![root_ca],
                    secrets: vec![
                        live,
                        old,
                        SecretSummary {
                            secret: other,
                            key_count: 2,
                        },
                    ],
                    ..NamespaceObjects::default()
                },
            )]),
            secret_values: BTreeMap::from([
                ("kube-system/bootstrap-token-abc123".to_owned(), live_values),
                ("kube-system/bootstrap-token-old000".to_owned(), old_values),
            ]),
            facts: ClusterFacts {
                version: version.to_owned(),
                ..ClusterFacts::default()
            },
            ..State::default()
        })
    }

    async fn join(cluster: &MemoryCluster) -> Value {
        let (status, body) = send(&app(cluster), "GET", "/api/v1/cluster/join", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    #[tokio::test]
    async fn an_rke2_cluster_gives_the_supervisor_ca_hash_and_live_tokens() {
        let cluster = cluster("v1.34.1+rke2r1");
        let body = join(&cluster).await;
        let hash = hex::encode(Sha256::digest(CA.as_bytes()));
        assert_eq!(
            body,
            json!({
                "kubernetesVersion": "v1.34.1+rke2r1",
                "server": "https://10.0.0.5:9345",
                "caHash": hash,
                "tokens": [{"id": "abc123", "secret": "bootstrap-token-abc123",
                            "description": "zeabur-installer node add",
                            "expires": "2999-01-01T00:00:00Z",
                            "managedBy": "zeabur-installer"}],
                "unreadable": []
            })
        );
        assert!(!body.to_string().contains(SECRET));
    }

    #[tokio::test]
    async fn k3s_cannot_take_a_node_from_zeabur() {
        let cluster = cluster("v1.36.5+k3s1");
        let body = join(&cluster).await;
        assert_eq!(body["blocked"], "notRke2");
        assert_eq!(body.get("server"), None);
        assert_eq!(cluster.inspect(|s| s.secret_reads), Some(0));
    }

    #[tokio::test]
    async fn a_refused_read_leaves_its_part_out() {
        let cluster = cluster("v1.34.1+rke2r1");
        cluster.update(|s| {
            s.forbidden_in_namespaces.insert("secrets".to_owned());
            s.forbidden_resources.insert("nodes".to_owned());
        });
        let body = join(&cluster).await;
        assert_eq!(body.get("server"), None);
        assert!(body["caHash"].is_string());
        assert_eq!(body["tokens"], json!([]));
        let refused: Vec<&str> = body["unreadable"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|u| u["resource"].as_str())
            .collect();
        assert_eq!(refused, ["nodes", "secrets"]);
    }

    #[tokio::test]
    async fn without_a_ready_server_the_first_one_is_used_and_ipv6_is_bracketed() {
        let cluster = cluster("v1.34.1+rke2r1");
        cluster.update(|s| {
            s.nodes = vec![
                node(
                    "a",
                    &json!({"node-role.kubernetes.io/control-plane": "true"}),
                    "fd00::1",
                    "False",
                ),
                node(
                    "b",
                    &json!({"node-role.kubernetes.io/control-plane": "true"}),
                    "fd00::2",
                    "True",
                ),
            ];
        });
        let body = join(&cluster).await;
        assert_eq!(body["server"], "https://[fd00::2]:9345");
    }

    /// The plan never shows the token's secret; applied, the token is listed; deleted, it is
    /// gone.
    #[tokio::test]
    async fn a_join_token_is_created_and_revoked_through_plans() {
        let cluster = cluster("v1.34.1+rke2r1");
        let app = app(&cluster);
        let yaml = format!(
            "apiVersion: v1\nkind: Secret\ntype: bootstrap.kubernetes.io/token\nmetadata:\n  \
             name: bootstrap-token-new456\n  namespace: kube-system\n  labels:\n    \
             app.kubernetes.io/managed-by: zeabur\nstringData:\n  token-id: new456\n  \
             token-secret: {SECRET}\n  description: \"Zeabur: add node\"\n  expiration: \
             \"2999-06-01T00:00:00Z\"\n  usage-bootstrap-authentication: \"true\"\n  \
             usage-bootstrap-signing: \"true\"\n  auth-extra-groups: \
             system:bootstrappers:rke2:default-node-token\n"
        );
        let (status, planned) = send(
            &app,
            "POST",
            "/api/v1/projects/kube-system/plan",
            Some(&json!({"changes": [{"op": "create", "yaml": yaml}]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        assert!(!planned.to_string().contains(SECRET), "{planned}");
        let (status, applied) = send(
            &app,
            "POST",
            "/api/v1/projects/kube-system/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let written = cluster
            .inspect(|s| s.secret_values["kube-system/bootstrap-token-new456"].clone())
            .unwrap();
        assert_eq!(written["token-secret"], SECRET.as_bytes());
        assert_eq!(
            written["auth-extra-groups"],
            b"system:bootstrappers:rke2:default-node-token"
        );

        let ids = |body: &Value| -> Vec<String> {
            body["tokens"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["id"].as_str().unwrap().to_owned())
                .collect()
        };
        let body = join(&cluster).await;
        assert_eq!(ids(&body), ["abc123", "new456"]);
        assert_eq!(body["tokens"][1]["managedBy"], "zeabur");

        planned_and_applied(
            &app,
            "kube-system",
            json!([{"op": "delete", "resource": {"group": "", "version": "v1",
                    "resource": "secrets"}, "name": "bootstrap-token-new456"}]),
        )
        .await;
        assert_eq!(ids(&join(&cluster).await), ["abc123"]);
    }

    fn baseline(namespace: &str, cidrs: &[&str]) -> zeabur_kube::DynamicObject {
        let mut from = vec![
            json!({"podSelector": {}}),
            json!({"namespaceSelector": {"matchLabels":
                {"app.kubernetes.io/managed-by": "zeabur-installer"}}}),
        ];
        from.extend(cidrs.iter().map(|c| json!({"ipBlock": {"cidr": c}})));
        serde_json::from_value(json!({
            "apiVersion": "networking.k8s.io/v1",
            "kind": "NetworkPolicy",
            "metadata": {"name": "zeabur-installer-baseline", "namespace": namespace,
                         "resourceVersion": "3",
                         "labels": {"app.kubernetes.io/managed-by": "zeabur-installer"}},
            "spec": {"podSelector": {}, "policyTypes": ["Ingress"], "ingress": [{"from": from}]}
        }))
        .unwrap()
    }

    /// Two nodes, and policies that know only the first, as after `worker-1` joined.
    fn with_policies() -> MemoryCluster {
        let cluster = cluster("v1.34.1+rke2r1");
        cluster.update(|s| {
            s.served.push(zeabur_kube::ServedResource::readable(
                "networking.k8s.io",
                "v1",
                "networkpolicies",
                "NetworkPolicy",
                true,
            ));
            s.dynamic = vec![
                baseline("zeabur", &["10.0.0.5/32"]),
                baseline("observability", &["10.0.0.5/32"]),
            ];
        });
        cluster
    }

    #[tokio::test]
    async fn the_baseline_policies_are_behind_a_new_node_until_a_plan_rewrites_them() {
        let cluster = with_policies();
        let app = app(&cluster);
        let (status, body) = send(&app, "GET", "/api/v1/cluster/node-policies", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body["behind"],
            json!([{"name": "worker-1", "addresses": ["10.0.0.6"]}])
        );
        assert_eq!(body["canUpdate"], "allowed");

        let (status, planned) =
            send(&app, "POST", "/api/v1/cluster/node-policies/plan", None).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        let namespaces: Vec<&str> = planned["writes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w["namespace"].as_str().unwrap())
            .collect();
        assert_eq!(namespaces, ["observability", "zeabur"]);
        assert!(
            planned["writes"][0]["afterYaml"]
                .as_str()
                .unwrap()
                .contains("10.0.0.6/32")
        );
        let (status, applied) = send(
            &app,
            "POST",
            "/api/v1/cluster/node-policies/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let (_, body) = send(&app, "GET", "/api/v1/cluster/node-policies", None).await;
        assert_eq!(body["behind"], json!([]));
        let (_, again) = send(&app, "POST", "/api/v1/cluster/node-policies/plan", None).await;
        assert_eq!(again["writes"], json!([]));
    }

    #[tokio::test]
    async fn a_plan_made_before_another_node_joined_is_refused() {
        let cluster = with_policies();
        let app = app(&cluster);
        let (_, planned) = send(&app, "POST", "/api/v1/cluster/node-policies/plan", None).await;
        cluster.update(|s| {
            s.nodes
                .push(node("worker-2", &json!({}), "10.0.0.7", "True"));
        });
        let (status, _) = send(
            &app,
            "POST",
            "/api/v1/cluster/node-policies/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn policies_the_person_may_not_read_or_patch_are_said_so() {
        let cluster = with_policies();
        cluster.update(|s| {
            s.forbidden_writes
                .insert("NetworkPolicy/zeabur-installer-baseline".to_owned());
        });
        let app = app(&cluster);
        let (_, body) = send(&app, "GET", "/api/v1/cluster/node-policies", None).await;
        assert_eq!(body["canUpdate"], "denied");
        let (_, planned) = send(&app, "POST", "/api/v1/cluster/node-policies/plan", None).await;
        let (status, refused) = send(
            &app,
            "POST",
            "/api/v1/cluster/node-policies/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");

        cluster.update(|s| {
            s.forbidden_resources
                .insert("networkpolicies.networking.k8s.io".to_owned());
        });
        let (_, body) = send(&app, "GET", "/api/v1/cluster/node-policies", None).await;
        assert_eq!(
            body["unreadable"][0]["resource"],
            "networkpolicies.networking.k8s.io"
        );
        assert_eq!(body["policies"], json!([]));
    }
}
