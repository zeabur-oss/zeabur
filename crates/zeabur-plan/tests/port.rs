//! A `port` change on a service whose `PORT` comes from an import: the container sets `PORT`
//! itself, so it wins whatever the imports hold, and no import is written.

#[cfg(test)]
mod tests {
    use serde_json::{Value, from_value, json};
    use zeabur_graph::Snapshot;
    use zeabur_model::{WorkloadKind, WorkloadRef};
    use zeabur_plan::{Change, Input, Plan, PlanRequest, plan};

    fn deployment(ports: &Value) -> Value {
        json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {"name": "web", "namespace": "shop", "resourceVersion": "3", "generation": 1},
            "spec": {"selector": {"matchLabels": {"app": "web"}}, "template": {
                "metadata": {"labels": {"app": "web"}},
                "spec": {"containers": [{"name": "web", "image": "example/web:1",
                    "ports": ports,
                    "envFrom": [{"secretRef": {"name": "web-env"}},
                        {"configMapRef": {"name": "web-config"}}]}]}}}
        })
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

    fn planned(deployment: Value) -> Result<Plan, zeabur_plan::PlanError> {
        #[expect(clippy::unwrap_in_result, reason = "a test fixture")]
        let snapshot = Snapshot {
            namespace: "shop".to_owned(),
            deployments: vec![from_value(deployment).unwrap()],
            ..Snapshot::default()
        };
        plan(
            Input::new(&snapshot, "2026-11-06T00:00:00Z"),
            &PlanRequest {
                changes: vec![move_port()],
            },
        )
    }

    #[test]
    fn a_port_change_sets_port_in_the_container_and_writes_no_import() {
        let moved = planned(deployment(&json!([{"containerPort": 8080}]))).unwrap();
        let kinds: Vec<&str> = moved
            .objects
            .iter()
            .map(|o| o.target.kind.as_str())
            .collect();
        assert_eq!(kinds, ["Deployment"]);
        let after = &moved.objects[0].raw_after;
        let container = &after["spec"]["template"]["spec"]["containers"][0];
        assert_eq!(container["env"], json!([{"name": "PORT", "value": "3000"}]));
        assert_eq!(container["ports"], json!([{"containerPort": 3000}]));
    }

    #[test]
    fn a_port_change_to_a_port_the_container_declares_moves_what_checks_the_old_one() {
        let ports = json!([{"name": "http", "containerPort": 8080}, {"containerPort": 3000}]);
        let mut web = deployment(&ports);
        web["spec"]["template"]["spec"]["containers"][0]["readinessProbe"] =
            json!({"httpGet": {"path": "/", "port": "http"}});
        let moved = planned(web).unwrap();
        let container = &moved.objects[0].raw_after["spec"]["template"]["spec"]["containers"][0];
        assert_eq!(container["readinessProbe"]["httpGet"]["port"], 3000);
        assert_eq!(container["ports"], ports);
    }
}
