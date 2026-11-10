//! A value shown masked in place of a Secret's (`(hidden)`, or a run of `•`) is never written,
//! whichever change would write it and whether it comes plain or base64-encoded. The planner
//! refuses the plan with `masked_value`; values that only contain a bullet are written.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use base64::Engine as _;
    use serde_json::{Value, from_value, json};
    use zeabur_graph::Snapshot;
    use zeabur_plan::checks::ResourceRef;
    use zeabur_plan::{Fetched, Input, Plan, PlanError, PlanRequest, Write, plan};

    const NOW: &str = "2026-10-15T00:00:00Z";

    fn encode(text: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(text)
    }

    /// A Deployment `web`, a ConfigMap `settings` and a Secret `credentials` whose values
    /// (`password`, `token`) the person could read.
    fn objects() -> Vec<Value> {
        vec![
            json!({
                "apiVersion": "apps/v1",
                "kind": "Deployment",
                "metadata": {"name": "web", "namespace": "demo", "resourceVersion": "3", "generation": 1},
                "spec": {
                    "selector": {"matchLabels": {"app": "web"}},
                    "template": {
                        "metadata": {"labels": {"app": "web"}},
                        "spec": {"containers": [{"name": "app", "image": "example/web:1"}]}
                    }
                }
            }),
            json!({
                "apiVersion": "v1",
                "kind": "ConfigMap",
                "metadata": {"name": "settings", "namespace": "demo", "resourceVersion": "4"},
                "data": {"mode": "fast"}
            }),
            json!({
                "apiVersion": "v1",
                "kind": "Secret",
                "metadata": {"name": "credentials", "namespace": "demo", "resourceVersion": "5"},
                "type": "Opaque"
            }),
        ]
    }

    /// The snapshot and the objects as read, from [`objects`].
    fn read() -> (Snapshot, Vec<Fetched>) {
        let input = objects();
        let mut snapshot = Snapshot {
            namespace: "demo".to_owned(),
            ..Snapshot::default()
        };
        let mut fetched = Vec::new();
        for object in &input {
            snapshot.insert(object.clone()).unwrap();
            let api_version = object["apiVersion"].as_str().unwrap();
            let (group, version) = api_version.split_once('/').unwrap_or(("", api_version));
            fetched.push(Fetched {
                resource: ResourceRef {
                    group: group.to_owned(),
                    version: version.to_owned(),
                    resource: format!("{}s", object["kind"].as_str().unwrap().to_lowercase()),
                },
                name: object["metadata"]["name"].as_str().unwrap().to_owned(),
                object: object.clone(),
            });
        }
        (snapshot, fetched)
    }

    fn planned(changes: &Value) -> Result<Plan, PlanError> {
        let (snapshot, fetched) = read();
        let request = request(changes);
        let secret_values = BTreeMap::from([(
            "credentials".to_owned(),
            BTreeMap::from([
                ("password".to_owned(), b"hunter2".to_vec()),
                ("token".to_owned(), b"t0k".to_vec()),
            ]),
        )]);
        plan(
            Input {
                objects: &fetched,
                secret_values: &secret_values,
                ..Input::new(&snapshot, NOW)
            },
            &request,
        )
    }

    fn request(changes: &Value) -> PlanRequest {
        from_value(json!({ "changes": changes })).unwrap()
    }

    fn refused(changes: &Value) {
        let error = planned(changes).unwrap_err();
        assert_eq!(error.code, "masked_value", "{changes}: {error:?}");
    }

    /// The patch or body the plan writes for its only object, as text.
    fn written(changes: &Value) -> String {
        let plan = planned(changes).unwrap();
        let [object] = plan.objects.as_slice() else {
            panic!("one object: {plan:?}");
        };
        match &object.write {
            Write::Create { body } => body.to_string(),
            Write::Patch { operations } => Value::from(operations.clone()).to_string(),
            other @ (Write::Delete { .. } | Write::EphemeralContainers { .. }) => {
                panic!("{other:?}")
            }
        }
    }

    fn create_secret(field: &str, value: &str) -> Value {
        json!([{"op": "create", "object": {
            "apiVersion": "v1",
            "kind": "Secret",
            "metadata": {"name": "api-env"},
            field: {"TOKEN": value}
        }}])
    }

    fn set_key(kind: &str, name: &str, value: &str) -> Value {
        json!([{"op": "data", "kind": kind, "name": name, "changes": [
            {"op": "set", "key": "password", "value": value}
        ]}])
    }

    fn edit_secret(data: &Value) -> Value {
        let yaml = format!(
            "apiVersion: v1\nkind: Secret\nmetadata: {{name: credentials}}\ntype: Opaque\ndata: {data}\n"
        );
        json!([{"op": "edit", "resource": {"group": "", "version": "v1", "resource": "secrets"},
            "name": "credentials", "yaml": yaml}])
    }

    #[test]
    fn a_created_secret_with_a_mask_is_refused_plain_or_encoded() {
        refused(&create_secret("stringData", "(hidden)"));
        refused(&create_secret("stringData", "••••••••"));
        refused(&create_secret("data", &encode("(hidden)")));
        refused(&create_secret("data", &encode("••••")));
        assert!(written(&create_secret("stringData", "s3cr3t")).contains("s3cr3t"));
    }

    #[test]
    fn a_key_set_to_a_mask_is_refused() {
        refused(&set_key("Secret", "credentials", "(hidden)"));
        refused(&set_key("Secret", "credentials", "••••••••"));
        refused(&set_key("ConfigMap", "settings", "(hidden)"));
    }

    #[test]
    fn a_variable_set_to_a_mask_is_refused() {
        refused(
            &json!([{"op": "env", "workload": {"kind": "Deployment", "name": "web"},
            "changes": [{"op": "set", "name": "TOKEN", "value": "••••••••"}]}]),
        );
    }

    #[test]
    fn a_yaml_edit_with_an_encoded_mask_is_refused() {
        refused(&edit_secret(
            &json!({"password": encode("(hidden)"), "token": "(hidden)"}),
        ));
        refused(&edit_secret(
            &json!({"password": encode("••••••••"), "token": "(hidden)"}),
        ));
    }

    #[test]
    fn a_yaml_edit_keeps_the_keys_left_masked_and_writes_the_rest() {
        let write = written(&edit_secret(
            &json!({"password": "(hidden)", "token": "••••", "user": encode("alice")}),
        ));
        assert!(write.contains(&encode("alice")), "{write}");
        assert!(!write.contains("/data/password"), "{write}");
        assert!(!write.contains("/data/token"), "{write}");
    }

    #[test]
    fn a_value_that_only_contains_a_bullet_is_written() {
        for value in ["p•ss", "• first", "a • b", "(hidden) for now"] {
            let secret = written(&set_key("Secret", "credentials", value));
            assert!(secret.contains(&encode(value)), "{value}: {secret}");
            let config = written(&set_key("ConfigMap", "settings", value));
            assert!(config.contains(value), "{value}: {config}");
        }
        let created = written(&create_secret("data", &encode("p•ss")));
        assert!(created.contains(&encode("p•ss")), "{created}");
    }
}
