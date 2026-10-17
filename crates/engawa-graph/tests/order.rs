//! The projection is a function of the objects, not of the order kube-apiserver lists them in,
//! checked on generated namespaces. The round trip through a change (project, plan, project
//! again) is `engawa-plan`'s property test.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use engawa_graph::{Snapshot, project};
    use k8s_openapi::api::apps::v1::Deployment;
    use k8s_openapi::api::core::v1::{ConfigMap, Service};
    use proptest::prelude::*;
    use serde_json::json;

    /// One generated workload: a name, a role label and literal env values.
    #[derive(Debug, Clone)]
    struct Spec {
        name: String,
        role: String,
        env: BTreeMap<String, String>,
        config: bool,
        exposed: bool,
    }

    fn spec() -> impl Strategy<Value = Spec> {
        (
            "[a-z]{3,8}",
            prop_oneof!["web", "api", "worker", "db"],
            prop::collection::btree_map("[A-Z][A-Z0-9_]{0,6}", "[a-z0-9]{0,8}", 0..4),
            any::<bool>(),
            any::<bool>(),
        )
            .prop_map(|(name, role, env, config, exposed)| Spec {
                name,
                role,
                env,
                config,
                exposed,
            })
    }

    fn namespace() -> impl Strategy<Value = Vec<Spec>> {
        prop::collection::vec(spec(), 1..5).prop_map(|mut specs| {
            // Workload names are unique within a namespace.
            let mut seen = std::collections::BTreeSet::new();
            specs.retain(|s| seen.insert(s.name.clone()));
            specs
        })
    }

    fn snapshot(specs: &[Spec]) -> Snapshot {
        let mut snapshot = Snapshot {
            namespace: "generated".to_owned(),
            ..Snapshot::default()
        };
        for s in specs {
            let env: Vec<serde_json::Value> = s
                .env
                .iter()
                .map(|(k, v)| json!({"name": k, "value": v}))
                .collect();
            let env_from = if s.config {
                json!([{"configMapRef": {"name": format!("{}-config", s.name)}}])
            } else {
                json!([])
            };
            let deployment: Deployment = serde_json::from_value(json!({
                "metadata": {"name": s.name, "namespace": "generated",
                             "labels": {"app.kubernetes.io/component": s.role}},
                "spec": {
                    "selector": {"matchLabels": {"app": s.name}},
                    "template": {
                        "metadata": {"labels": {"app": s.name}},
                        "spec": {"containers": [{
                            "name": "main", "image": "example:1",
                            "env": env, "envFrom": env_from
                        }]}
                    }
                }
            }))
            .unwrap();
            snapshot.deployments.push(deployment);
            if s.config {
                let config: ConfigMap = serde_json::from_value(json!({
                    "metadata": {"name": format!("{}-config", s.name), "namespace": "generated"},
                    "data": {"A": "1"}
                }))
                .unwrap();
                snapshot.config_maps.push(config);
            }
            if s.exposed {
                let service: Service = serde_json::from_value(json!({
                    "metadata": {"name": s.name, "namespace": "generated"},
                    "spec": {"selector": {"app": s.name}, "ports": [{"name": "http", "port": 80}]}
                }))
                .unwrap();
                snapshot.services.push(service);
            }
        }
        snapshot
    }

    proptest! {
        #[test]
        fn the_projection_does_not_depend_on_object_order(specs in namespace()) {
            let forward = snapshot(&specs);
            let mut reversed = forward.clone();
            reversed.deployments.reverse();
            reversed.config_maps.reverse();
            reversed.services.reverse();
            prop_assert_eq!(project(&forward), project(&reversed));
        }
    }
}
