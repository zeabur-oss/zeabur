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

    #[test]
    fn a_port_change_is_refused_for_a_service_that_also_selects_another_workload() {
        let mut snapshot = Snapshot {
            namespace: "shop".to_owned(),
            deployments: vec![from_value(deployment(&json!([{"containerPort": 8080}]))).unwrap()],
            ..Snapshot::default()
        };
        let mut other = deployment(&json!([{"containerPort": 8080}]));
        other["metadata"]["name"] = json!("worker");
        snapshot.deployments.push(from_value(other).unwrap());
        let service = json!({"apiVersion": "v1", "kind": "Service",
            "metadata": {"name": "web", "namespace": "shop", "resourceVersion": "7"},
            "spec": {"selector": {"app": "web"}, "ports": [{"port": 80, "targetPort": 8080}]}});
        snapshot.services.push(from_value(service).unwrap());
        let request = PlanRequest {
            changes: vec![move_port()],
        };
        let error = plan(Input::new(&snapshot, "2026-11-06T00:00:00Z"), &request).unwrap_err();
        assert_eq!(error.code, "shared_service");
    }

    #[test]
    fn a_port_change_leaves_a_udp_port_with_the_same_number_alone() {
        let ports = json!([{"containerPort": 8080}, {"containerPort": 8080, "protocol": "UDP"}]);
        let mut snapshot = Snapshot {
            namespace: "shop".to_owned(),
            deployments: vec![from_value(deployment(&ports)).unwrap()],
            ..Snapshot::default()
        };
        let service = json!({"apiVersion": "v1", "kind": "Service",
            "metadata": {"name": "web", "namespace": "shop", "resourceVersion": "7"},
            "spec": {"selector": {"app": "web"}, "ports": [
                {"name": "tcp", "port": 80, "targetPort": 8080},
                {"name": "dns", "port": 53, "protocol": "UDP", "targetPort": 8080}]}});
        snapshot.services.push(from_value(service).unwrap());
        let request = PlanRequest {
            changes: vec![move_port()],
        };
        let moved = plan(Input::new(&snapshot, "2026-11-06T00:00:00Z"), &request).unwrap();
        let container = &moved.objects[0].raw_after["spec"]["template"]["spec"]["containers"][0];
        assert_eq!(
            container["ports"],
            json!([{"containerPort": 3000}, {"containerPort": 8080, "protocol": "UDP"}])
        );
        let service = moved
            .objects
            .iter()
            .find(|o| o.target.kind == "Service")
            .unwrap();
        let targets: Vec<&Value> = service.raw_after["spec"]["ports"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| &p["targetPort"])
            .collect();
        assert_eq!(targets, [&json!(3000), &json!(8080)]);
    }
}
