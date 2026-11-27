//! Database update settings preserve everything except the Keel keys they manage.

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use zeabur_graph::Snapshot;
    use zeabur_model::{ImagePolicy, UpdateWindow};
    use zeabur_plan::builds::cloudnative_pg_clusters;
    use zeabur_plan::json::apply_patch;
    use zeabur_plan::{
        AutoUpdateClusterKind, AutoUpdateTarget, Change, ErrorKind, Fetched, Input, Plan,
        PlanError, PlanRequest, Update, Write, plan,
    };

    fn cluster() -> Value {
        json!({
            "apiVersion": "postgresql.cnpg.io/v1", "kind": "Cluster",
            "metadata": {"name": "db", "namespace": "demo", "uid": "original",
                "resourceVersion": "2", "generation": 1,
                "annotations": {"kubectl.kubernetes.io/last-applied-configuration": "saved manifest"}},
            "spec": {"imageName": "ghcr.io/cloudnative-pg/postgresql:17-standard-trixie@sha256:824cf6bf883d72783dcd834b0674da66cad000cda28191887aa2941134a24c8b",
                "instances": 2, "storage": {"size": "8Gi"}, "plugins": [{"name": "backup"}]}
        })
    }

    fn image_update() -> Update {
        Update::Image {
            policy: ImagePolicy::SameTag,
            window: None,
        }
    }

    fn planned(object: Value, changes: Vec<Change>) -> Result<Plan, PlanError> {
        let snapshot = Snapshot {
            namespace: "demo".to_owned(),
            ..Snapshot::default()
        };
        let objects = [Fetched {
            resource: cloudnative_pg_clusters(),
            name: "db".to_owned(),
            object,
        }];
        plan(
            Input {
                objects: &objects,
                ..Input::new(&snapshot, "2026-11-27T10:00:00Z")
            },
            &PlanRequest { changes },
        )
    }

    fn update(object: Value, update: Update) -> Result<Plan, PlanError> {
        planned(
            object,
            vec![Change::AutoUpdate {
                workload: AutoUpdateTarget::Cluster {
                    kind: AutoUpdateClusterKind::Cluster,
                    name: "db".to_owned(),
                },
                update,
            }],
        )
    }

    fn applied(object: &Value, plan: &Plan) -> Value {
        assert_eq!(plan.objects.len(), 1);
        let Write::Patch { operations } = &plan.objects.first().unwrap().write else {
            panic!("update settings are a JSON Patch");
        };
        apply_patch(object, operations).unwrap()
    }

    #[test]
    fn enable_and_off_keep_a_sole_last_applied_annotation_and_the_complete_spec() {
        let object = cluster();
        let enabled = applied(&object, &update(object.clone(), image_update()).unwrap());
        let expected = apply_patch(&object, &[
            json!({"op": "add", "path": "/metadata/annotations/keel.sh~1policy", "value": "force"}),
            json!({"op": "add", "path": "/metadata/annotations/keel.sh~1trigger", "value": "poll"}),
            json!({"op": "add", "path": "/metadata/annotations/keel.sh~1pollSchedule", "value": "@hourly"}),
            json!({"op": "add", "path": "/metadata/annotations/keel.sh~1match-tag", "value": "true"}),
        ]).unwrap();
        assert_eq!(enabled, expected);
        assert_eq!(
            applied(&enabled, &update(enabled.clone(), Update::Off).unwrap()),
            object
        );
    }

    #[test]
    fn status_churn_and_unrelated_annotation_changes_survive_a_planned_update() {
        let object = cluster();
        let plan = update(object.clone(), image_update()).unwrap();
        let current = apply_patch(&object, &[
            json!({"op": "replace", "path": "/metadata/resourceVersion", "value": "57"}),
            json!({"op": "add", "path": "/status", "value": {"phaseReason": "Updating replicas"}}),
            json!({"op": "add", "path": "/metadata/annotations/cnpg.io~1fencedInstances", "value": "[\"db-2\"]"}),
        ]).unwrap();
        let enabled = applied(&current, &plan);
        assert_eq!(enabled.get("status"), current.get("status"));
        assert_eq!(enabled.get("spec"), current.get("spec"));
        assert_eq!(
            enabled.pointer("/metadata/resourceVersion"),
            Some(&json!("57"))
        );
        assert_eq!(
            enabled.pointer("/metadata/annotations/cnpg.io~1fencedInstances"),
            Some(&json!("[\"db-2\"]"))
        );
        assert_eq!(
            applied(&enabled, &update(enabled.clone(), Update::Off).unwrap()),
            current
        );
    }

    #[test]
    fn a_fresh_plan_uses_the_current_image_and_storage() {
        let current = apply_patch(&cluster(), &[
            json!({"op": "replace", "path": "/spec/imageName", "value": "registry.example/db:17-standard-trixie"}),
            json!({"op": "replace", "path": "/spec/storage/size", "value": "16Gi"}),
            json!({"op": "replace", "path": "/metadata/generation", "value": 2}),
        ]).unwrap();
        let enabled = applied(&current, &update(current.clone(), image_update()).unwrap());
        assert_eq!(enabled.get("spec"), current.get("spec"));
    }

    #[test]
    fn only_same_tag_updates_of_explicit_major_images_without_a_catalog_are_enabled() {
        for image in [
            "postgres:latest",
            "postgres",
            "postgres@sha256:824cf6bf883d72783dcd834b0674da66cad000cda28191887aa2941134a24c8b",
            "registry.example:5000/postgres",
            "postgres:017",
            "postgres:+17",
            "postgres:17wrong",
        ] {
            let object = apply_patch(
                &cluster(),
                &[json!({"op": "replace", "path": "/spec/imageName", "value": image})],
            )
            .unwrap();
            assert_eq!(
                update(object, image_update()).unwrap_err().code,
                "policy_not_allowed",
                "{image}"
            );
        }
        let catalog = apply_patch(&cluster(), &[json!({"op": "add", "path": "/spec/imageCatalogRef", "value": {"name": "postgres", "major": 17}})]).unwrap();
        assert_eq!(
            update(catalog, image_update()).unwrap_err().code,
            "policy_not_allowed"
        );
        for policy in [ImagePolicy::Patch, ImagePolicy::Minor, ImagePolicy::Major] {
            assert_eq!(
                update(
                    cluster(),
                    Update::Image {
                        policy,
                        window: None
                    }
                )
                .unwrap_err()
                .code,
                "policy_not_allowed"
            );
        }
        assert_eq!(
            update(
                cluster(),
                Update::NewBuilds {
                    build: "db".to_owned(),
                    webhook: "https://keel.example".to_owned()
                }
            )
            .unwrap_err()
            .code,
            "policy_not_allowed"
        );
        assert_eq!(
            update(
                cluster(),
                Update::Image {
                    policy: ImagePolicy::SameTag,
                    window: Some(UpdateWindow {
                        day: None,
                        hour: 24
                    })
                }
            )
            .unwrap_err()
            .code,
            "invalid_window"
        );
    }

    #[test]
    fn off_remains_available_after_the_image_becomes_ineligible() {
        let enabled = applied(&cluster(), &update(cluster(), image_update()).unwrap());
        let current = apply_patch(&enabled, &[
            json!({"op": "replace", "path": "/spec/imageName", "value": "postgres:latest"}),
            json!({"op": "add", "path": "/spec/imageCatalogRef", "value": {"name": "postgres", "major": 17}}),
        ]).unwrap();
        let off = applied(&current, &update(current.clone(), Update::Off).unwrap());
        assert_eq!(off.get("spec"), current.get("spec"));
        assert_eq!(
            off.pointer("/metadata/annotations"),
            cluster().pointer("/metadata/annotations")
        );
    }

    #[test]
    fn approval_requirements_refuse_enable_but_preserve_off() {
        for field in ["labels", "annotations"] {
            let mut current = applied(&cluster(), &update(cluster(), image_update()).unwrap());
            current["metadata"][field]["keel.sh/approvals"] = json!("1");
            assert_eq!(
                update(current.clone(), image_update()).unwrap_err().code,
                "policy_not_allowed"
            );
            let off = applied(&current, &update(current.clone(), Update::Off).unwrap());
            assert_eq!(off["metadata"][field]["keel.sh/approvals"], "1");
            assert_eq!(off.get("spec"), current.get("spec"));
        }
    }

    #[test]
    fn legacy_label_settings_and_match_tag_alias_do_not_override_off() {
        let object = apply_patch(&cluster(), &[
            json!({"op": "add", "path": "/metadata/labels", "value": {"app": "db", "keel.sh/policy": "force", "keel.sh/trigger": "poll", "keel.sh/matchTag": "true"}}),
            json!({"op": "add", "path": "/metadata/annotations/keel.sh~1matchTag", "value": "true"}),
        ]).unwrap();
        let off = applied(&object, &update(object.clone(), Update::Off).unwrap());
        assert_eq!(off.pointer("/metadata/labels"), Some(&json!({"app": "db"})));
        assert_eq!(
            off.pointer("/metadata/annotations"),
            cluster().pointer("/metadata/annotations")
        );
    }

    #[test]
    fn managed_clusters_cannot_be_taken_over() {
        let object = apply_patch(&cluster(), &[json!({"op": "add", "path": "/metadata/labels", "value": {"app.kubernetes.io/managed-by": "Helm"}})]).unwrap();
        assert_eq!(
            update(object, image_update()).unwrap_err().kind,
            ErrorKind::Conflict
        );
        let owned = apply_patch(&cluster(), &[json!({"op": "add", "path": "/metadata/ownerReferences", "value": [{"apiVersion": "example.com/v1", "kind": "Database", "name": "db", "uid": "owner"}]})]).unwrap();
        assert_eq!(
            update(owned, image_update()).unwrap_err().code,
            "policy_not_allowed"
        );
        let field_owned = apply_patch(&cluster(), &[json!({"op": "add", "path": "/metadata/managedFields", "value": [{
            "manager": "argocd-controller", "operation": "Apply", "apiVersion": "postgresql.cnpg.io/v1",
            "fieldsType": "FieldsV1", "fieldsV1": {"f:metadata": {"f:annotations": {"f:keel.sh/policy": {}}}}
        }]})]).unwrap();
        assert_eq!(
            update(field_owned, image_update()).unwrap_err().kind,
            ErrorKind::Conflict
        );
    }

    #[test]
    fn generic_edit_replans_from_the_current_object_even_with_an_old_client_version() {
        let edited = apply_patch(
            &cluster(),
            &[
                json!({"op": "replace", "path": "/metadata/resourceVersion", "value": "1"}),
                json!({"op": "replace", "path": "/spec/storage/size", "value": "16Gi"}),
            ],
        )
        .unwrap();
        let plan = planned(
            cluster(),
            vec![Change::Edit {
                resource: cloudnative_pg_clusters(),
                name: "db".to_owned(),
                yaml: edited.to_string(),
                spec_only: false,
            }],
        )
        .unwrap();
        let current = applied(&cluster(), &plan);
        assert_eq!(
            current.pointer("/metadata/resourceVersion"),
            Some(&json!("2"))
        );
        assert_eq!(current.pointer("/spec/storage/size"), Some(&json!("16Gi")));
    }
}
