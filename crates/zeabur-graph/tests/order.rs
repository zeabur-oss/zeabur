//! The projection is a function of the objects, not of the order kube-apiserver lists them in,
//! checked on generated namespaces, and so is the ownership graph. The round trip through a change
//! (project, plan, project again) is `zeabur-plan`'s property test.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use k8s_openapi::api::apps::v1::Deployment;
    use k8s_openapi::api::batch::v1::Job;
    use k8s_openapi::api::core::v1::{ConfigMap, Service};
    use proptest::prelude::*;
    use serde_json::json;
    use zeabur_graph::{OwnerGraph, Snapshot, project};

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

        #[test]
        fn ownership_ends_and_does_not_depend_on_object_order(
            jobs in owned_jobs().prop_flat_map(|jobs| {
                let shuffled = Just(jobs.clone()).prop_shuffle();
                (Just(jobs), shuffled)
            })
        ) {
            let (listed, shuffled) = jobs;
            prop_assert_eq!(groups(&listed), groups(&shuffled));
        }
    }

    /// Jobs `j0`, `j1`, … each controlled by an optional other: by index, which may close a
    /// cycle, or by a CloudNativePG `Cluster` that was not read.
    fn owned_jobs() -> impl Strategy<Value = Vec<Job>> {
        prop::collection::vec(prop::option::of(0..8_usize), 1..8).prop_map(|owners| {
            let count = owners.len();
            owners
                .into_iter()
                .enumerate()
                .map(|(index, owner)| {
                    let reference = owner.map(|owner| {
                        if owner < count {
                            json!({"apiVersion": "batch/v1", "kind": "Job",
                                   "name": format!("j{owner}"), "uid": format!("u{owner}"),
                                   "controller": true})
                        } else {
                            json!({"apiVersion": "postgresql.cnpg.io/v1", "kind": "Cluster",
                                   "name": "pg", "uid": "pg", "controller": true})
                        }
                    });
                    serde_json::from_value(json!({
                        "metadata": {"name": format!("j{index}"), "namespace": "generated",
                                     "uid": format!("u{index}"),
                                     "ownerReferences": reference.into_iter().collect::<Vec<_>>()}
                    }))
                    .unwrap()
                })
                .collect()
        })
    }

    /// Each root, whether it is a database, and its members.
    fn groups(jobs: &[Job]) -> Vec<(String, bool, Vec<String>)> {
        let snapshot = Snapshot {
            namespace: "generated".to_owned(),
            jobs: jobs.to_vec(),
            ..Snapshot::default()
        };
        let graph = OwnerGraph::new(&snapshot);
        graph
            .roots()
            .map(|root| {
                (
                    root.uid.clone(),
                    graph.database(root).is_some(),
                    graph.members(root).map(|key| key.uid.clone()).collect(),
                )
            })
            .collect()
    }
}
