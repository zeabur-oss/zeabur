//! Adopting a namespace, letting it go and a project's details: each is one patch of the
//! Namespace that changes the one label or the annotations asked for and nothing else, applies
//! to the Namespace as stored, and is refused for the namespaces Kubernetes keeps for itself.

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use zeabur_graph::Snapshot;
    use zeabur_model::{ProjectIcon, Tool, WorkloadKind, WorkloadRef};
    use zeabur_plan::json::apply_patch;
    use zeabur_plan::{
        Change, Conventions, ErrorKind, Input, Plan, PlanError, PlanRequest, Write, plan, plan_with,
    };

    const LABEL: &str = "zeabur.com/project";

    fn namespace(name: &str, labels: &Value, annotations: Option<Value>) -> Value {
        let mut object = json!({
            "apiVersion": "v1", "kind": "Namespace",
            "metadata": {"name": name, "resourceVersion": "7", "uid": "u-1", "labels": labels},
            "spec": {"finalizers": ["kubernetes"]},
            "status": {"phase": "Active"}
        });
        if let Some(annotations) = annotations {
            object["metadata"]["annotations"] = annotations;
        }
        object
    }

    fn shop(adopted: bool) -> Value {
        let mut labels = json!({"kubernetes.io/metadata.name": "shop", "team": "web"});
        if adopted {
            labels[LABEL] = json!("true");
        }
        namespace("shop", &labels, None)
    }

    fn planned(namespace: Option<&Value>, changes: Vec<Change>) -> Result<Plan, PlanError> {
        let snapshot = Snapshot::default();
        plan(
            Input {
                storage_classes: None,
                namespace,
                ..Input::new(&snapshot, "2026-10-15T00:00:00Z")
            },
            &PlanRequest { changes },
        )
    }

    /// The one patch of the plan, applied to `stored`, and its operations.
    fn applied(plan: &Plan, stored: &Value) -> (Value, Vec<Value>) {
        assert_eq!(plan.objects.len(), 1);
        let object = &plan.objects[0];
        assert_eq!(
            (
                object.target.api_version.as_str(),
                object.target.plural.as_str()
            ),
            ("v1", "namespaces")
        );
        assert!(!object.target.namespaced());
        let Write::Patch { operations } = &object.write else {
            panic!("a Namespace is patched");
        };
        assert_eq!(
            operations[0],
            json!({"op": "test", "path": "/metadata/resourceVersion", "value": "7"})
        );
        (apply_patch(stored, operations).unwrap(), operations.clone())
    }

    /// `after` is `before` with only the label or annotation `key` of `map` changed to `value`.
    fn only(before: &Value, after: &Value, map: &str, key: &str, value: Option<&str>) {
        let mut expected = before.clone();
        let entries = expected["metadata"]
            .as_object_mut()
            .unwrap()
            .entry(map)
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .unwrap();
        match value {
            Some(value) => {
                entries.insert(key.to_owned(), json!(value));
            }
            None => {
                entries.remove(key);
            }
        }
        assert_eq!(after, &expected);
    }

    #[test]
    fn adopting_adds_the_project_label_and_nothing_else() {
        let stored = shop(false);
        let plan = planned(Some(&stored), vec![Change::Adopt]).unwrap();
        let (after, operations) = applied(&plan, &stored);
        only(&stored, &after, "labels", LABEL, Some("true"));
        assert_eq!(
            operations[1..],
            [
                json!({"op": "add", "path": "/metadata/labels/zeabur.com~1project",
                    "value": "true"})
            ]
        );
        assert_eq!(plan.impact, []);
        assert_eq!(
            plan.objects[0]
                .changes
                .iter()
                .map(|c| c.path.as_str())
                .collect::<Vec<_>>(),
            ["metadata.labels.zeabur.com/project"]
        );
    }

    #[test]
    fn a_namespace_without_labels_gets_the_one_label() {
        let mut stored = shop(false);
        stored["metadata"].as_object_mut().unwrap().remove("labels");
        let plan = planned(Some(&stored), vec![Change::Adopt]).unwrap();
        let (after, _) = applied(&plan, &stored);
        only(&stored, &after, "labels", LABEL, Some("true"));
    }

    #[test]
    fn adopting_a_namespace_with_empty_labels_only_adds_the_label() {
        let stored = namespace("shop", &json!({}), None);
        let plan = planned(Some(&stored), vec![Change::Adopt]).unwrap();
        let (after, _) = applied(&plan, &stored);
        only(&stored, &after, "labels", LABEL, Some("true"));
        let changes = &plan.objects[0].changes;
        assert_eq!(changes.len(), 1, "{changes:?}");
        assert_eq!(changes[0].path, "metadata.labels.zeabur.com/project");
        assert_eq!(
            (changes[0].before.as_deref(), changes[0].after.as_deref()),
            (None, Some("true"))
        );
    }

    #[test]
    fn letting_go_removes_the_project_label_and_nothing_else() {
        let stored = shop(true);
        let plan = planned(Some(&stored), vec![Change::Unadopt]).unwrap();
        let (after, operations) = applied(&plan, &stored);
        only(&stored, &after, "labels", LABEL, None);
        // The value removed is tested first.
        assert_eq!(operations[1]["op"], "test");
        assert_eq!(operations[1]["value"], "true");
        // A patch planned against an older Namespace does not apply to a newer one.
        let mut newer = stored;
        newer["metadata"]["resourceVersion"] = json!("8");
        assert!(apply_patch(&newer, &operations).is_err());
    }

    #[test]
    fn the_namespaces_kubernetes_keeps_are_never_adopted() {
        for name in ["kube-system", "kube-public", "kube-node-lease"] {
            let stored = namespace(name, &json!({}), None);
            let error = planned(Some(&stored), vec![Change::Adopt]).unwrap_err();
            assert_eq!(error.kind, ErrorKind::SystemNamespace, "{name}");
            assert_eq!(error.code, "system_namespace");
        }
    }

    #[test]
    fn adopting_twice_or_letting_go_of_a_namespace_that_is_not_a_project_is_refused() {
        let error = planned(Some(&shop(true)), vec![Change::Adopt]).unwrap_err();
        assert_eq!(
            (error.kind, error.code.as_str()),
            (ErrorKind::Conflict, "already_adopted")
        );
        let error = planned(Some(&shop(false)), vec![Change::Unadopt]).unwrap_err();
        assert_eq!(
            (error.kind, error.code.as_str()),
            (ErrorKind::Conflict, "not_adopted")
        );
        let error = planned(None, vec![Change::Adopt]).unwrap_err();
        assert_eq!(error.kind, ErrorKind::NotFound);
    }

    /// A label Argo CD set (the namespace is adopted in Git) would come back on its next sync.
    #[test]
    fn a_project_label_a_tool_owns_is_not_removed() {
        let mut stored = shop(true);
        stored["metadata"]["managedFields"] = json!([{
            "manager": "argocd-controller", "operation": "Apply", "apiVersion": "v1",
            "fieldsType": "FieldsV1",
            "fieldsV1": {"f:metadata": {"f:labels": {"f:zeabur.com/project": {}}}}
        }]);
        let error = planned(Some(&stored), vec![Change::Unadopt]).unwrap_err();
        assert_eq!(error.managed_by, Some(Tool::ArgoCd));
    }

    fn details(name: Option<&str>, description: Option<&str>, icon: Option<ProjectIcon>) -> Change {
        Change::Details {
            display_name: name.map(str::to_owned),
            description: description.map(str::to_owned),
            icon,
        }
    }

    #[test]
    fn details_are_the_three_annotations_and_nothing_else() {
        let stored = shop(true);
        let plan = planned(
            Some(&stored),
            vec![details(
                Some(" Shop "),
                Some("The web shop.\nAnd its jobs."),
                Some(ProjectIcon::Storefront),
            )],
        )
        .unwrap();
        let (after, _) = applied(&plan, &stored);
        let mut expected = stored;
        expected["metadata"]["annotations"] = json!({
            "zeabur.com/display-name": "Shop",
            "zeabur.com/description": "The web shop.\nAnd its jobs.",
            "zeabur.com/icon": "storefront",
        });
        assert_eq!(after, expected);

        // Changing one replaces it (testing the old value); leaving one out removes it; other
        // annotations stay.
        let stored = namespace(
            "shop",
            &json!({LABEL: "true"}),
            Some(json!({
                "zeabur.com/display-name": "Shop",
                "zeabur.com/icon": "storefront",
                "owner": "web-team",
            })),
        );
        let plan = planned(
            Some(&stored),
            vec![details(Some("Store"), None, Some(ProjectIcon::Storefront))],
        )
        .unwrap();
        let (after, operations) = applied(&plan, &stored);
        only(
            &stored,
            &after,
            "annotations",
            "zeabur.com/display-name",
            Some("Store"),
        );
        assert!(operations.contains(
            &json!({"op": "test", "path": "/metadata/annotations/zeabur.com~1display-name", "value": "Shop"})
        ));
        let plan = planned(Some(&stored), vec![details(Some("Shop"), None, None)]).unwrap();
        let (after, _) = applied(&plan, &stored);
        only(&stored, &after, "annotations", "zeabur.com/icon", None);
    }

    #[test]
    fn details_that_are_already_so_too_long_or_of_a_namespace_that_is_not_a_project_are_refused() {
        let error = planned(Some(&shop(true)), vec![details(None, Some("  "), None)]).unwrap_err();
        assert_eq!(error.code, "unchanged");
        let long = "x".repeat(zeabur_model::DISPLAY_NAME_MAX + 1);
        let error = planned(Some(&shop(true)), vec![details(Some(&long), None, None)]).unwrap_err();
        assert_eq!(error.code, "display_name_too_long");
        let error =
            planned(Some(&shop(true)), vec![details(Some("a\nb"), None, None)]).unwrap_err();
        assert_eq!(error.code, "display_name_not_text");
        let error =
            planned(Some(&shop(false)), vec![details(Some("Shop"), None, None)]).unwrap_err();
        assert_eq!(error.code, "not_adopted");
        // Adopting with details in one plan: one patch, the label and the annotation.
        let stored = shop(false);
        let plan = planned(
            Some(&stored),
            vec![Change::Adopt, details(Some("Shop"), None, None)],
        )
        .unwrap();
        let (after, _) = applied(&plan, &stored);
        assert_eq!(after["metadata"]["labels"][LABEL], "true");
        assert_eq!(
            after["metadata"]["annotations"]["zeabur.com/display-name"],
            "Shop"
        );
    }

    #[test]
    fn a_namespace_change_is_not_planned_with_changes_to_objects() {
        let error = planned(
            Some(&shop(false)),
            vec![
                Change::Adopt,
                Change::Restart {
                    workload: WorkloadRef {
                        kind: WorkloadKind::Deployment,
                        name: "web".to_owned(),
                    },
                },
            ],
        )
        .unwrap_err();
        assert_eq!(error.code, "namespace_change_alone");
    }

    #[test]
    fn the_changes_read_back_as_the_api_sends_them() {
        let adopt: Change = serde_json::from_value(json!({"op": "adopt"})).unwrap();
        assert_eq!(adopt, Change::Adopt);
        let set: Change = serde_json::from_value(
            json!({"op": "details", "displayName": "Shop", "icon": "rocket"}),
        )
        .unwrap();
        assert_eq!(set, details(Some("Shop"), None, Some(ProjectIcon::Rocket)));
        assert!(
            serde_json::from_value::<Change>(json!({"op": "details", "icon": "unicorn"})).is_err()
        );
    }

    #[test]
    fn other_conventions_adopt_under_their_own_label() {
        let conventions = Conventions {
            project_label: "example.com/project".into(),
            display_name_annotation: "example.com/name".into(),
            ..Conventions::default()
        };
        let snapshot = Snapshot::default();
        let stored = shop(false);
        let request = |change| PlanRequest {
            changes: vec![change],
        };
        let input = Input {
            storage_classes: None,
            namespace: Some(&stored),
            ..Input::new(&snapshot, "2026-10-15T00:00:00Z")
        };
        let adopted = plan_with(input, &request(Change::Adopt), &conventions).unwrap();
        let (after, _) = applied(&adopted, &stored);
        assert_eq!(after["metadata"]["labels"]["example.com/project"], "true");
        assert!(after["metadata"]["labels"].get(LABEL).is_none());

        let named = Input {
            namespace: Some(&after),
            ..input
        };
        let change = details(Some("Shop"), None, None);
        let set = plan_with(named, &request(change), &conventions).unwrap();
        let (after, _) = applied(&set, &after);
        assert_eq!(after["metadata"]["annotations"]["example.com/name"], "Shop");
        // Zeabur's own label is not a project under these conventions.
        assert_eq!(
            plan_with(
                input,
                &request(details(Some("Shop"), None, None)),
                &conventions
            )
            .unwrap_err()
            .code,
            "not_adopted"
        );
        let zeaburs = shop(true);
        let theirs = Input {
            namespace: Some(&zeaburs),
            ..input
        };
        assert_eq!(
            plan_with(
                theirs,
                &request(details(Some("Shop"), None, None)),
                &conventions
            )
            .unwrap_err()
            .code,
            "not_adopted"
        );
    }
}
