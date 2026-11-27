//! CloudNativePG update settings use the existing reviewed object edit.

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use zeabur_graph::Snapshot;
    use zeabur_plan::checks::ResourceRef;
    use zeabur_plan::json::apply_patch;
    use zeabur_plan::{Change, ErrorKind, Fetched, Input, PlanRequest, Write, plan};

    fn cluster() -> Value {
        json!({
            "apiVersion": "postgresql.cnpg.io/v1", "kind": "Cluster",
            "metadata": {"name": "db", "namespace": "demo", "uid": "original",
                         "resourceVersion": "2", "generation": 1},
            "spec": {"imageName": "ghcr.io/cloudnative-pg/postgresql:18-standard-trixie@sha256:pin",
                     "instances": 2, "storage": {"size": "8Gi"}, "plugins": [{"name": "backup"}]}
        })
    }

    fn planned(object: Value, edited: &Value) -> Result<zeabur_plan::Plan, zeabur_plan::PlanError> {
        let snapshot = Snapshot {
            namespace: "demo".to_owned(),
            ..Snapshot::default()
        };
        let resource = ResourceRef {
            group: "postgresql.cnpg.io".to_owned(),
            version: "v1".to_owned(),
            resource: "clusters".to_owned(),
        };
        let objects = [Fetched {
            resource: resource.clone(),
            name: "db".to_owned(),
            object,
        }];
        plan(
            Input {
                objects: &objects,
                ..Input::new(&snapshot, "2026-11-27T10:00:00Z")
            },
            &PlanRequest {
                changes: vec![Change::Edit {
                    resource,
                    name: "db".to_owned(),
                    yaml: edited.to_string(),
                    spec_only: false,
                }],
            },
        )
    }

    #[test]
    fn updates_only_change_the_reviewed_annotations() {
        let object = cluster();
        let mut edited = object.clone();
        edited
            .pointer_mut("/metadata")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(
                "annotations".to_owned(),
                json!({"keel.sh/policy": "force", "keel.sh/match-tag": "true",
                "keel.sh/trigger": "poll", "keel.sh/pollSchedule": "0 0 3 * * 0"}),
            );
        let result = planned(object.clone(), &edited).unwrap();
        let Write::Patch { operations, .. } = &result.objects.first().unwrap().write else {
            panic!("patch")
        };
        let patched = apply_patch(&object, operations).unwrap();
        assert_eq!(patched.get("spec"), object.get("spec"));
        assert_eq!(
            patched.pointer("/metadata/annotations"),
            edited.pointer("/metadata/annotations")
        );
    }

    #[test]
    fn a_stale_form_cannot_revert_a_manual_image_or_storage_change() {
        let mut stale = cluster();
        stale
            .pointer_mut("/metadata")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("resourceVersion".to_owned(), json!("1"));
        let error = planned(cluster(), &stale).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Conflict);
        assert_eq!(error.code, "object_changed");
    }

    #[test]
    fn an_edit_without_a_client_version_keeps_its_existing_behavior() {
        let mut edited = cluster();
        edited
            .pointer_mut("/metadata")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("resourceVersion");
        edited
            .pointer_mut("/metadata")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("annotations".to_owned(), json!({"keel.sh/policy": "force"}));
        assert!(planned(cluster(), &edited).is_ok());
    }
}
