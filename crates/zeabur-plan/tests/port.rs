//! A `port` change on a service that imports `PORT` from a Secret: the Variables page still
//! shows one `PORT`, and editing it there still reaches the container.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use serde_json::{Value, from_value, json};
    use zeabur_graph::Snapshot;
    use zeabur_model::{WorkloadKind, WorkloadRef};
    use zeabur_plan::{Change, DataEdit, DataKind, Input, Plan, PlanRequest, plan};

    type Values = BTreeMap<String, BTreeMap<String, Vec<u8>>>;

    fn web() -> Value {
        json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {"name": "web", "namespace": "shop", "resourceVersion": "3", "generation": 1},
            "spec": {"selector": {"matchLabels": {"app": "web"}}, "template": {
                "metadata": {"labels": {"app": "web"}},
                "spec": {"containers": [{"name": "web", "image": "example/web:1",
                    "ports": [{"containerPort": 8080}],
                    "envFrom": [{"secretRef": {"name": "web-env"}}]}]}}}
        })
    }

    fn web_env() -> Value {
        json!({"apiVersion": "v1", "kind": "Secret", "type": "Opaque",
            "metadata": {"name": "web-env", "namespace": "shop", "resourceVersion": "5"}})
    }

    fn values(port: &str) -> Values {
        BTreeMap::from([(
            "web-env".to_owned(),
            BTreeMap::from([("PORT".to_owned(), port.as_bytes().to_vec())]),
        )])
    }

    fn planned(deployment: &Value, values: &Values, change: Change) -> Plan {
        let snapshot = Snapshot {
            namespace: "shop".to_owned(),
            deployments: vec![from_value(deployment.clone()).unwrap()],
            secrets: vec![from_value(web_env()).unwrap()],
            ..Snapshot::default()
        };
        planned_in(&snapshot, values, change)
    }

    fn planned_in(snapshot: &Snapshot, values: &Values, change: Change) -> Plan {
        let input = Input {
            secret_values: values,
            ..Input::new(snapshot, "2026-11-06T00:00:00Z")
        };
        plan(
            input,
            &PlanRequest {
                changes: vec![change],
            },
        )
        .unwrap()
    }

    fn after(plan: &Plan, kind: &str) -> Option<Value> {
        let object = plan.objects.iter().find(|o| o.target.kind == kind)?;
        Some(object.raw_after.clone())
    }

    /// The values the Variables page lists for `PORT`: the container's own, then the Secrets it
    /// imports.
    fn port_rows(deployment: &Value, values: &Values) -> Vec<String> {
        let container = &deployment["spec"]["template"]["spec"]["containers"][0];
        let mut rows = Vec::new();
        for env in container["env"].as_array().into_iter().flatten() {
            if env["name"] == "PORT" {
                rows.push(env["value"].as_str().unwrap().to_owned());
            }
        }
        for from in container["envFrom"].as_array().into_iter().flatten() {
            let name = from["secretRef"]["name"].as_str().unwrap();
            if let Some(port) = values.get(name).and_then(|v| v.get("PORT")) {
                rows.push(String::from_utf8(port.clone()).unwrap());
            }
        }
        rows
    }

    /// The `PORT` the container gets: its own `env` wins over what it imports.
    fn port_seen(deployment: &Value, values: &Values) -> String {
        port_rows(deployment, values).swap_remove(0)
    }

    /// The values of `web-env` after `plan`: `before` when it does not write it.
    fn written_values(plan: &Plan, before: &Values) -> Values {
        let Some(secret) = after(plan, "Secret") else {
            return before.clone();
        };
        let mut keys = BTreeMap::new();
        for (key, value) in secret["data"].as_object().unwrap() {
            keys.insert(
                key.clone(),
                STANDARD.decode(value.as_str().unwrap()).unwrap(),
            );
        }
        BTreeMap::from([("web-env".to_owned(), keys)])
    }

    fn move_port() -> Change {
        Change::Port {
            workload: WorkloadRef {
                kind: WorkloadKind::Deployment,
                name: "web".to_owned(),
            },
            container: None,
            from: 8080,
            to: 3000,
        }
    }

    fn set_port(value: &str) -> Change {
        Change::Data {
            kind: DataKind::Secret,
            name: "web-env".to_owned(),
            changes: vec![DataEdit {
                op: "set".to_owned(),
                key: "PORT".to_owned(),
                value: Some(value.to_owned()),
            }],
        }
    }

    #[test]
    fn the_variables_page_shows_port_once_after_a_port_change() {
        let stored = values("8080");
        let moved = planned(&web(), &stored, move_port());
        let deployment = after(&moved, "Deployment").unwrap();
        assert_eq!(
            port_rows(&deployment, &written_values(&moved, &stored)),
            ["3000"]
        );
    }

    #[test]
    fn editing_port_on_the_variables_page_after_a_port_change_reaches_the_container() {
        let stored = values("8080");
        let moved = planned(&web(), &stored, move_port());
        let deployment = after(&moved, "Deployment").unwrap();
        let stored = written_values(&moved, &stored);
        let edited = planned(&deployment, &stored, set_port("4000"));
        assert_eq!(
            port_seen(&deployment, &written_values(&edited, &stored)),
            "4000"
        );
    }

    fn container(deployment: &Value) -> &Value {
        &deployment["spec"]["template"]["spec"]["containers"][0]
    }

    #[test]
    fn a_port_change_sets_port_in_a_config_map_imported_after_the_secret() {
        let mut deployment = web();
        deployment["spec"]["template"]["spec"]["containers"][0]["envFrom"] = json!([
            {"secretRef": {"name": "web-env"}},
            {"configMapRef": {"name": "web-config"}}
        ]);
        let config: Value = json!({"apiVersion": "v1", "kind": "ConfigMap",
            "metadata": {"name": "web-config", "namespace": "shop", "resourceVersion": "6"},
            "data": {"PORT": "8080"}});
        let snapshot = Snapshot {
            namespace: "shop".to_owned(),
            deployments: vec![from_value(deployment).unwrap()],
            secrets: vec![from_value(web_env()).unwrap()],
            config_maps: vec![from_value(config).unwrap()],
            ..Snapshot::default()
        };
        let moved = planned_in(&snapshot, &values("8080"), move_port());
        let config = after(&moved, "ConfigMap").unwrap();
        assert_eq!(config["data"]["PORT"], "3000");
        assert!(after(&moved, "Secret").is_none());
    }

    fn with_later_import(import: Value, config: Option<Value>) -> Plan {
        let mut deployment = web();
        deployment["spec"]["template"]["spec"]["containers"][0]["envFrom"] =
            json!([{"secretRef": {"name": "web-env"}}, import]);
        let snapshot = Snapshot {
            namespace: "shop".to_owned(),
            deployments: vec![from_value(deployment).unwrap()],
            secrets: vec![from_value(web_env()).unwrap()],
            config_maps: config.into_iter().map(|c| from_value(c).unwrap()).collect(),
            ..Snapshot::default()
        };
        planned_in(&snapshot, &values("8080"), move_port())
    }

    fn sets_port_in_env(plan: &Plan) {
        let deployment = after(plan, "Deployment").unwrap();
        let env = &container(&deployment)["env"];
        assert_eq!(env, &json!([{"name": "PORT", "value": "3000"}]));
        assert!(after(plan, "Secret").is_none());
    }

    #[test]
    fn a_port_change_sets_port_in_env_when_a_later_config_map_cannot_be_read() {
        let moved = with_later_import(json!({"configMapRef": {"name": "web-config"}}), None);
        sets_port_in_env(&moved);
    }

    #[test]
    fn a_port_change_sets_port_in_env_when_a_later_prefixed_import_forms_port() {
        let config = json!({"apiVersion": "v1", "kind": "ConfigMap",
            "metadata": {"name": "web-config", "namespace": "shop", "resourceVersion": "6"},
            "data": {"ORT": "8080"}});
        let import = json!({"prefix": "P", "configMapRef": {"name": "web-config"}});
        sets_port_in_env(&with_later_import(import, Some(config)));
    }

    #[test]
    fn a_port_change_moves_what_names_the_old_port_when_the_new_one_is_declared() {
        let mut deployment = web();
        let spec = &mut deployment["spec"]["template"]["spec"]["containers"][0];
        spec["ports"] = json!([
            {"name": "http", "containerPort": 8080},
            {"name": "metrics", "containerPort": 3000}
        ]);
        spec["readinessProbe"] = json!({"httpGet": {"path": "/", "port": "http"}});
        let service: Value = json!({"apiVersion": "v1", "kind": "Service",
            "metadata": {"name": "web", "namespace": "shop", "resourceVersion": "7"},
            "spec": {"selector": {"app": "web"},
                "ports": [{"port": 80, "targetPort": "http"}]}});
        let snapshot = Snapshot {
            namespace: "shop".to_owned(),
            deployments: vec![from_value(deployment).unwrap()],
            secrets: vec![from_value(web_env()).unwrap()],
            services: vec![from_value(service).unwrap()],
            ..Snapshot::default()
        };
        let moved = planned_in(&snapshot, &values("8080"), move_port());
        let deployment = after(&moved, "Deployment").unwrap();
        let spec = container(&deployment);
        assert_eq!(spec["readinessProbe"]["httpGet"]["port"], 3000);
        assert_eq!(
            spec["ports"],
            json!([{"name": "metrics", "containerPort": 3000}])
        );
        let service = after(&moved, "Service").unwrap();
        assert_eq!(service["spec"]["ports"][0]["targetPort"], 3000);
    }

    #[test]
    fn a_port_change_restarts_the_pods_when_only_the_imported_port_changes() {
        let mut deployment = web();
        deployment["spec"]["template"]["spec"]["containers"][0]["ports"] =
            json!([{"containerPort": 3000}]);
        let moved = planned(&deployment, &values("8080"), move_port());
        let deployment = after(&moved, "Deployment").unwrap();
        let annotations = &deployment["spec"]["template"]["metadata"]["annotations"];
        assert!(annotations[zeabur_plan::RESTARTED_AT].is_string());
    }
}
