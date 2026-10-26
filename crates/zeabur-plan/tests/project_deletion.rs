//! Deleting a project: the database objects go first, then the Namespace, each pinned to the
//! object as read; the plan says what goes and what happens to each volume's data; Zeabur's own
//! and the system's namespaces are refused.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::{Value, json};
    use zeabur_graph::Snapshot;
    use zeabur_model::{Tool, WorkloadKind, WorkloadRef};
    use zeabur_plan::{
        Change, DatabaseObject, DeletionInput, ObjectName, Plan, PlanError, PlanRequest,
        ResourceRef, VolumeDeletion, Warning, Write, plan,
    };

    fn namespace(name: &str, labels: &Value) -> Value {
        json!({"apiVersion": "v1", "kind": "Namespace",
            "metadata": {"name": name, "resourceVersion": "7", "uid": "ns-uid", "labels": labels},
            "status": {"phase": "Active"}})
    }

    fn shop() -> Value {
        namespace("shop", &json!({"zeabur.com/project": "true"}))
    }

    fn cnpg() -> ResourceRef {
        ResourceRef {
            group: "postgresql.cnpg.io".to_owned(),
            version: "v1".to_owned(),
            resource: "clusters".to_owned(),
        }
    }

    fn database(name: &str) -> DatabaseObject {
        DatabaseObject {
            resource: cnpg(),
            object: json!({"apiVersion": "postgresql.cnpg.io/v1", "kind": "Cluster",
                "metadata": {"name": name, "uid": format!("{name}-uid"), "resourceVersion": "40",
                    "finalizers": ["cnpg.io/cleanup"]}}),
        }
    }

    fn snapshot() -> Snapshot {
        fn from<T: serde::de::DeserializeOwned>(v: Value) -> T {
            serde_json::from_value(v).unwrap()
        }
        Snapshot {
            namespace: "shop".to_owned(),
            deployments: vec![from(
                json!({"metadata": {"name": "web", "namespace": "shop"},
                "spec": {"selector": {}, "template": {"spec": {"containers": []}}}}),
            )],
            stateful_sets: vec![from(
                json!({"metadata": {"name": "db-1", "namespace": "shop",
                "ownerReferences": [{"apiVersion": "postgresql.cnpg.io/v1", "kind": "Cluster",
                    "name": "db", "uid": "db-uid", "controller": true}]},
                "spec": {"selector": {}, "serviceName": "db",
                    "template": {"spec": {"containers": []}}}}),
            )],
            persistent_volume_claims: vec![
                from(json!({"metadata": {"name": "uploads"},
                    "spec": {"volumeName": "pv-uploads", "storageClassName": "local-path"}})),
                from(json!({"metadata": {"name": "cache"},
                    "spec": {"volumeName": "pv-cache", "storageClassName": "local-path"}})),
                from(json!({"metadata": {"name": "pending"}, "spec": {}})),
            ],
            ..Snapshot::default()
        }
    }

    fn deletion() -> DeletionInput {
        DeletionInput {
            zeabur_namespaces: vec!["zeabur".to_owned(), "zeabur-terminals".to_owned()],
            databases: vec![database("db")],
            class_reclaim: BTreeMap::from([("local-path".to_owned(), "Delete".to_owned())]),
            default_class: Some("local-path".to_owned()),
            tools_namespace: "zeabur-terminals".to_owned(),
            tools_pods: vec!["zeabur-tools-alice-1".to_owned()],
        }
    }

    fn planned(
        namespace: &Value,
        snapshot: &Snapshot,
        deletion: &DeletionInput,
    ) -> Result<Plan, PlanError> {
        let reclaim = BTreeMap::from([("pv-cache".to_owned(), "Retain".to_owned())]);
        plan(
            zeabur_plan::Input {
                namespace: Some(namespace),
                deletion: Some(deletion),
                reclaim_policies: &reclaim,
                ..zeabur_plan::Input::new(snapshot, "2026-10-26T00:00:00Z")
            },
            &PlanRequest {
                changes: vec![Change::DeleteProject],
            },
        )
    }

    #[test]
    fn the_databases_go_before_the_namespace_each_pinned_to_the_object_read() {
        let plan = planned(&shop(), &snapshot(), &deletion()).unwrap();
        let writes: Vec<(&str, &str, &Write)> = plan
            .objects
            .iter()
            .map(|o| (o.target.kind.as_str(), o.target.name.as_str(), &o.write))
            .collect();
        assert_eq!(
            writes,
            [
                (
                    "Cluster",
                    "db",
                    &Write::Delete {
                        uid: Some("db-uid".to_owned()),
                        resource_version: "40".to_owned(),
                    }
                ),
                (
                    "Namespace",
                    "shop",
                    &Write::Delete {
                        uid: Some("ns-uid".to_owned()),
                        resource_version: "7".to_owned(),
                    }
                ),
            ]
        );
        assert!(plan.objects[0].target.namespaced());
        assert!(!plan.objects[1].target.namespaced());
    }

    #[test]
    fn the_plan_says_what_goes_and_what_each_volume_does_with_its_data() {
        let plan = planned(&shop(), &snapshot(), &deletion()).unwrap();
        let Some(Warning::DeletesProject(gone)) = plan
            .warnings
            .iter()
            .find(|w| matches!(w, Warning::DeletesProject(_)))
        else {
            panic!("no deletesProject in {:?}", plan.warnings);
        };
        // The database's StatefulSet is the database's, not a service of its own.
        assert_eq!(
            gone.services,
            [WorkloadRef {
                kind: WorkloadKind::Deployment,
                name: "web".to_owned()
            }]
        );
        assert_eq!(gone.databases[0].name, "db");
        let volume = |claim: &str| gone.volumes.iter().find(|v| v.claim == claim).unwrap();
        // The volume's own policy wins over its class's; an unbound claim takes the default
        // class's.
        assert_eq!(volume("cache").reclaim_policy.as_deref(), Some("Retain"));
        assert_eq!(volume("uploads").reclaim_policy.as_deref(), Some("Delete"));
        assert_eq!(
            volume("pending"),
            &VolumeDeletion {
                claim: "pending".to_owned(),
                volume: None,
                storage_class: Some("local-path".to_owned()),
                reclaim_policy: Some("Delete".to_owned()),
            }
        );
        assert_eq!(gone.tools_pods, ["zeabur-tools-alice-1"]);
    }

    #[test]
    fn a_volume_whose_policy_cannot_be_read_says_so() {
        let unread = DeletionInput {
            class_reclaim: BTreeMap::new(),
            ..deletion()
        };
        let plan = planned(&shop(), &snapshot(), &unread).unwrap();
        let Some(Warning::DeletesProject(gone)) = plan.warnings.first() else {
            panic!("{:?}", plan.warnings);
        };
        let uploads = gone.volumes.iter().find(|v| v.claim == "uploads").unwrap();
        assert_eq!(uploads.reclaim_policy, None);
    }

    #[test]
    fn objects_a_tool_manages_are_named_as_coming_back() {
        let mut snapshot = snapshot();
        snapshot.deployments = vec![
            serde_json::from_value(json!({"metadata": {"name": "web", "namespace": "shop",
                "labels": {"app.kubernetes.io/managed-by": "Helm"},
                "annotations": {"meta.helm.sh/release-name": "web"}},
                "spec": {"selector": {}, "template": {"spec": {"containers": []}}}}))
            .unwrap(),
        ];
        let plan = planned(&shop(), &snapshot, &deletion()).unwrap();
        assert!(
            plan.warnings.contains(&Warning::Recreated {
                tool: Tool::Helm,
                objects: vec![ObjectName {
                    kind: "Deployment".to_owned(),
                    name: "web".to_owned()
                }],
            }),
            "{:?}",
            plan.warnings
        );
        let unmanaged = planned(&shop(), &self::snapshot(), &deletion()).unwrap();
        assert!(
            !unmanaged
                .warnings
                .iter()
                .any(|w| matches!(w, Warning::Recreated { .. }))
        );
    }

    #[test]
    fn namespaces_that_are_not_the_persons_project_are_refused() {
        let terminating = {
            let mut ns = shop();
            ns["metadata"]["deletionTimestamp"] = json!("2026-10-26T00:00:00Z");
            ns
        };
        let cases = [
            (
                "kube-system",
                namespace("kube-system", &json!({"zeabur.com/project": "true"})),
                "system_namespace",
            ),
            (
                "Zeabur's own",
                namespace("zeabur", &json!({"zeabur.com/project": "true"})),
                "zeabur_namespace",
            ),
            (
                "the terminals'",
                namespace("zeabur-terminals", &json!({"zeabur.com/project": "true"})),
                "zeabur_namespace",
            ),
            (
                "node shells'",
                namespace("zeabur-node-shell", &json!({"zeabur.com/project": "true"})),
                "zeabur_namespace",
            ),
            (
                "a component's",
                namespace(
                    "traefik",
                    &json!({"zeabur.com/project": "true", "zeabur.com/platform": "true"}),
                ),
                "component_namespace",
            ),
            (
                "not a project",
                namespace("shop", &json!({})),
                "not_adopted",
            ),
            ("already going", terminating, "already_deleting"),
        ];
        for (case, ns, code) in cases {
            let error = planned(&ns, &snapshot(), &deletion()).unwrap_err();
            assert_eq!(error.code, code, "{case}");
        }
    }

    #[test]
    fn deleting_a_project_is_planned_on_its_own() {
        let snapshot = snapshot();
        let input = deletion();
        let ns = shop();
        let error = plan(
            zeabur_plan::Input {
                namespace: Some(&ns),
                deletion: Some(&input),
                ..zeabur_plan::Input::new(&snapshot, "2026-10-26T00:00:00Z")
            },
            &PlanRequest {
                changes: vec![
                    Change::DeleteProject,
                    Change::Restart {
                        workload: WorkloadRef {
                            kind: WorkloadKind::Deployment,
                            name: "web".to_owned(),
                        },
                    },
                ],
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "delete_project_alone");
    }
}
