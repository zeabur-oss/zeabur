//! Project → change → re-project: a planned object, put back in the snapshot, projects to
//! exactly the change asked for, and nothing else in the project moves.
//!
//! The namespaces are generated: several workloads, each with or without a Service, an
//! imported ConfigMap and an Ingress that already routes to it, so the properties cover the
//! branches the planner takes on what is already there [quickcheck].
//!
//! [quickcheck]: https://www.cs.tufts.edu/comp/150FP/archive/john-hughes/quick.pdf

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use proptest::prelude::*;
    use serde_json::{Value, from_value, json};
    use zeabur_graph::{Snapshot, project};
    use zeabur_model::{Project, Resource, Service, WorkloadKind, WorkloadRef};
    use zeabur_plan::rules::EnvEdit;
    use zeabur_plan::{Change, Input, Plan, PlanRequest, ResourceRef, plan};

    /// One generated workload.
    #[derive(Debug, Clone)]
    struct Spec {
        name: String,
        /// Literal variables, in the order the container lists them.
        env: BTreeMap<String, String>,
        /// A Service selects its pods.
        exposed: bool,
        /// It imports a ConfigMap of its own with `envFrom`.
        config: bool,
        /// The hosts of an Ingress that already routes to its Service (exposed workloads only).
        hosts: BTreeSet<String>,
    }

    /// A variable name, from a small alphabet so sets and removals meet existing names.
    fn name() -> impl Strategy<Value = String> {
        prop::sample::select(vec!["A", "B", "C", "D", "LOG_LEVEL", "MODE"]).prop_map(str::to_owned)
    }

    /// A literal value. It starts with a digit, so it never names a generated Service and
    /// cannot add an inferred connection of its own.
    fn value() -> impl Strategy<Value = String> {
        "[0-9][a-z0-9]{0,5}"
    }

    fn host() -> impl Strategy<Value = String> {
        "[a-z]{1,8}".prop_map(|label| format!("{label}.example.test"))
    }

    fn spec() -> impl Strategy<Value = Spec> {
        (
            "[a-z]{3,8}",
            prop::collection::btree_map(name(), value(), 0..4),
            any::<bool>(),
            any::<bool>(),
            prop::collection::btree_set(host(), 0..3),
        )
            .prop_map(|(name, env, exposed, config, hosts)| Spec {
                name,
                env,
                exposed,
                config,
                hosts: if exposed { hosts } else { BTreeSet::new() },
            })
    }

    /// One to four workloads with distinct names and distinct hosts.
    fn namespace() -> impl Strategy<Value = Vec<Spec>> {
        prop::collection::vec(spec(), 1..5).prop_map(|mut specs| {
            let mut names = BTreeSet::new();
            specs.retain(|s| names.insert(s.name.clone()));
            let mut hosts = BTreeSet::new();
            for s in &mut specs {
                s.hosts.retain(|h| hosts.insert(h.clone()));
            }
            specs
        })
    }

    fn build(specs: &[Spec]) -> Snapshot {
        let mut snapshot = Snapshot {
            namespace: "demo".to_owned(),
            ..Snapshot::default()
        };
        for s in specs {
            let env: Vec<Value> = s
                .env
                .iter()
                .map(|(name, value)| json!({"name": name, "value": value}))
                .collect();
            let env_from = if s.config {
                json!([{"configMapRef": {"name": format!("{}-config", s.name)}}])
            } else {
                json!([])
            };
            snapshot.deployments.push(
                from_value(json!({
                    "apiVersion": "apps/v1", "kind": "Deployment",
                    "metadata": {"name": s.name, "namespace": "demo", "resourceVersion": "1"},
                    "spec": {
                        "selector": {"matchLabels": {"app": s.name}},
                        "template": {
                            "metadata": {"labels": {"app": s.name}},
                            "spec": {"containers": [{"name": "app", "image": "example/app:1",
                                                     "env": env, "envFrom": env_from}]}
                        }
                    }
                }))
                .unwrap(),
            );
            if s.config {
                snapshot.config_maps.push(
                    from_value(json!({
                        "metadata": {"name": format!("{}-config", s.name), "namespace": "demo"},
                        "data": {"IMPORTED": "1"}
                    }))
                    .unwrap(),
                );
            }
            if s.exposed {
                snapshot.services.push(
                    from_value(json!({
                        "apiVersion": "v1", "kind": "Service",
                        "metadata": {"name": s.name, "namespace": "demo"},
                        "spec": {"selector": {"app": s.name}, "ports": [{"name": "http", "port": 8080}]}
                    }))
                    .unwrap(),
                );
            }
            if !s.hosts.is_empty() {
                let rules: Vec<Value> = s
                    .hosts
                    .iter()
                    .map(|h| {
                        json!({"host": h, "http": {"paths": [{"path": "/", "pathType": "Prefix",
                            "backend": {"service": {"name": s.name, "port": {"number": 8080}}}}]}})
                    })
                    .collect();
                snapshot.ingresses.push(
                    from_value(json!({
                        "apiVersion": "networking.k8s.io/v1", "kind": "Ingress",
                        "metadata": {"name": format!("{}-routes", s.name), "namespace": "demo",
                                     "resourceVersion": "1"},
                        "spec": {"rules": rules}
                    }))
                    .unwrap(),
                );
            }
        }
        snapshot
    }

    /// Two exposed workloads, `web` with `env` and `other` with `KEEP=1`.
    fn snapshot(env: &[(String, String)]) -> Snapshot {
        let spec = |name: &str, env: BTreeMap<String, String>| Spec {
            name: name.to_owned(),
            env,
            exposed: true,
            config: false,
            hosts: BTreeSet::new(),
        };
        build(&[
            spec("web", env.iter().cloned().collect()),
            spec(
                "other",
                BTreeMap::from([("KEEP".to_owned(), "1".to_owned())]),
            ),
        ])
    }

    fn deployment(name: &str) -> WorkloadRef {
        WorkloadRef {
            kind: WorkloadKind::Deployment,
            name: name.to_owned(),
        }
    }

    fn web() -> WorkloadRef {
        deployment("web")
    }

    fn planned(snapshot: &Snapshot, changes: Vec<Change>) -> Plan {
        let secrets = BTreeMap::new();
        let objects: Vec<zeabur_plan::Fetched> = snapshot
            .deployments
            .iter()
            .map(|d| zeabur_plan::Fetched {
                resource: deployments(),
                name: d.metadata.name.clone().unwrap(),
                object: serde_json::to_value(d).unwrap(),
            })
            .collect();
        plan(
            Input {
                secret_values: &secrets,
                objects: &objects,
                ..Input::new(snapshot, "2026-10-15T00:00:00Z")
            },
            &PlanRequest {
                changes,
                restart_after: false,
            },
        )
        .unwrap()
    }

    /// The snapshot with every planned object put in place of the one it was computed from.
    fn after(snapshot: &Snapshot, plan: &Plan) -> Snapshot {
        let mut next = snapshot.clone();
        for object in &plan.objects {
            let raw = object.raw_after.clone();
            match object.target.kind.as_str() {
                "Deployment" => {
                    next.deployments.retain(|d| {
                        d.metadata.name.as_deref() != Some(object.target.name.as_str())
                    });
                    next.deployments.push(from_value(raw).unwrap());
                }
                "Ingress" => {
                    next.ingresses.retain(|i| {
                        i.metadata.name.as_deref() != Some(object.target.name.as_str())
                    });
                    next.ingresses.push(from_value(raw).unwrap());
                }
                other => panic!("unexpected {other}"),
            }
        }
        next
    }

    fn service<'a>(project: &'a Project, name: &str) -> &'a Service {
        project
            .services
            .iter()
            .find(|s| s.workload.name == name)
            .unwrap()
    }

    fn literal_env(project: &Project, name: &str) -> Vec<(String, String)> {
        service(project, name).containers[0]
            .env
            .iter()
            .map(|e| (e.name.clone(), e.value.clone().unwrap_or_default()))
            .collect()
    }

    fn hosts(project: &Project, name: &str) -> BTreeSet<String> {
        project
            .routes_of(service(project, name))
            .flat_map(|r| r.rules.iter())
            .filter_map(|r| r.host.clone())
            .collect()
    }

    /// Every service but `target` projects exactly as before, and so do the connections
    /// between services and the resources `target` does not own.
    fn nothing_else_moved(
        before: &Project,
        projected: &Project,
        target: &str,
    ) -> Result<(), TestCaseError> {
        for b in before.services.iter().filter(|s| s.workload.name != target) {
            let p = service(projected, &b.workload.name);
            prop_assert_eq!(b, p);
            prop_assert_eq!(
                before.endpoints_of(b).collect::<Vec<_>>(),
                projected.endpoints_of(p).collect::<Vec<_>>()
            );
            prop_assert_eq!(
                before.routes_of(b).collect::<Vec<_>>(),
                projected.routes_of(p).collect::<Vec<_>>()
            );
        }
        let others = |project: &Project| -> Vec<Resource> {
            project
                .resources
                .iter()
                .filter(|r| !r.attribution.owners.iter().any(|o| o.name == target))
                .cloned()
                .collect()
        };
        prop_assert_eq!(others(projected), others(before));
        prop_assert_eq!(&projected.connections, &before.connections);
        Ok(())
    }

    fn deployments() -> ResourceRef {
        ResourceRef {
            group: "apps".to_owned(),
            version: "v1".to_owned(),
            resource: "deployments".to_owned(),
        }
    }

    fn image(project: &Project, name: &str) -> Option<String> {
        project
            .services
            .iter()
            .find(|s| s.workload.name == name)
            .and_then(|s| s.containers.first())
            .and_then(|c| c.image.clone())
    }

    proptest! {
        #[test]
        fn env_changes_project_back_to_exactly_what_was_asked(
            specs in namespace(),
            sets in prop::collection::btree_map(name(), value(), 0..3),
            removals in prop::collection::btree_set(name(), 0..3),
        ) {
            let target = specs[0].name.clone();
            let initial: Vec<(String, String)> = specs[0].env.clone().into_iter().collect();
            let snapshot = build(&specs);
            let removals: Vec<String> = removals
                .into_iter()
                .filter(|n| initial.iter().any(|(e, _)| e == n) && !sets.contains_key(n))
                .collect();
            let mut edits: Vec<EnvEdit> = sets
                .iter()
                .map(|(name, value)| EnvEdit { op: "set".to_owned(), name: name.clone(), value: Some(value.clone()), value_from: None })
                .collect();
            edits.extend(removals.iter().map(|name| EnvEdit { op: "remove".to_owned(), name: name.clone(), value: None, value_from: None }));
            prop_assume!(!edits.is_empty());
            let before = project(&snapshot);
            let plan = planned(&snapshot, vec![Change::Env { workload: deployment(&target), container: None, changes: edits }]);
            prop_assert_eq!(plan.objects.len(), 1, "one write for one object");
            let projected = project(&after(&snapshot, &plan));

            // What was asked: every set variable has its value, every removed one is gone, the
            // others are unchanged and keep their order.
            let mut expected: Vec<(String, String)> = initial
                .iter()
                .filter(|(n, _)| !removals.contains(n))
                .map(|(n, v)| (n.clone(), sets.get(n).cloned().unwrap_or_else(|| v.clone())))
                .collect();
            for (n, v) in &sets {
                if !initial.iter().any(|(e, _)| e == n) {
                    expected.push((n.clone(), v.clone()));
                }
            }
            prop_assert_eq!(literal_env(&projected, &target), expected);
            // Imports are not variables the person edited.
            prop_assert_eq!(&service(&projected, &target).containers[0].imported_env, &service(&before, &target).containers[0].imported_env);
            prop_assert_eq!(&projected.resources, &before.resources);
            nothing_else_moved(&before, &projected, &target)?;
        }

        #[test]
        fn a_new_domain_projects_onto_its_workload_only(specs in namespace(), new in host()) {
            // The target has a Service; whether an Ingress already routes to it decides
            // between extending that Ingress and creating one.
            let mut specs = specs;
            specs[0].exposed = true;
            let target = specs[0].name.clone();
            prop_assume!(specs.iter().all(|s| !s.hosts.contains(&new)));
            let snapshot = build(&specs);
            let before = project(&snapshot);
            let plan = planned(&snapshot, vec![Change::AddDomain { workload: deployment(&target), hostname: new.clone() }]);
            prop_assert_eq!(plan.objects.len(), 1, "one write for one object");
            let projected = project(&after(&snapshot, &plan));
            let mut expected = specs[0].hosts.clone();
            expected.insert(new);
            prop_assert_eq!(hosts(&projected, &target), expected);
            nothing_else_moved(&before, &projected, &target)?;
        }
    }

    proptest! {
        #[test]
        fn compute_changes_project_back(
            replicas in 0_i32..20,
            cpu in 1_u32..4000,
            memory in 16_u32..4096,
            headroom in 0_u32..4096,
        ) {
            let snapshot = snapshot(&[]);
            let before = project(&snapshot);
            let requests = BTreeMap::from([
                ("cpu".to_owned(), format!("{cpu}m")),
                ("memory".to_owned(), format!("{memory}Mi")),
            ]);
            let limits = BTreeMap::from([("memory".to_owned(), format!("{}Mi", memory + headroom))]);
            let plan = planned(&snapshot, vec![
                Change::Scale { workload: web(), replicas },
                Change::Resources { workload: web(), container: None, requests, limits },
            ]);
            prop_assert_eq!(plan.objects.len(), 1, "one write for one object");
            let projected = project(&after(&snapshot, &plan));
            let service = service(&projected, "web");
            prop_assert_eq!(service.workload.replicas.as_ref().map(|r| r.desired), Some(replicas));
            let resources = service.containers[0].resources.clone().unwrap();
            prop_assert_eq!(resources.requests.cpu, Some(format!("{cpu}m")));
            prop_assert_eq!(resources.requests.memory, Some(format!("{memory}Mi")));
            prop_assert_eq!(resources.limits.memory, Some(format!("{}Mi", memory + headroom)));
            prop_assert_eq!(resources.limits.cpu, None);
            prop_assert_eq!(literal_env(&projected, "other"), literal_env(&before, "other"));
            prop_assert_eq!(projected.connections, before.connections);
        }

        #[test]
        fn a_yaml_edit_projects_back_to_the_edit(tag in "[a-z0-9]{1,8}", label in "[a-z]{1,6}") {
            let snapshot = snapshot(&[("A".to_owned(), "1".to_owned())]);
            let before = project(&snapshot);
            let web_object = snapshot.deployments.iter().find(|d| d.metadata.name.as_deref() == Some("web")).unwrap();
            let mut edited = serde_json::to_value(web_object).unwrap();
            edited["spec"]["template"]["spec"]["containers"][0]["image"] = json!(format!("example/app:{tag}"));
            edited["metadata"]["labels"] = json!({"team": label});
            let yaml = serde_json::to_string(&edited).unwrap();
            let plan = planned(&snapshot, vec![Change::Edit { resource: deployments(), name: "web".to_owned(), yaml }]);
            // The YAML the plan dialog shows is the objects as planned.
            let want = format!("image: example/app:{tag}");
            let written = |yaml: &Option<String>| {
                yaml.as_deref().map(|y| y.lines().any(|l| l.trim().trim_start_matches("- ") == want))
            };
            prop_assert_eq!(written(&plan.objects[0].after_yaml), Some(true));
            prop_assert_eq!(written(&plan.objects[0].before_yaml), Some(tag == "1"));
            let next = after(&snapshot, &plan);
            let projected = project(&next);
            prop_assert_eq!(image(&projected, "web"), Some(format!("example/app:{tag}")));
            prop_assert_eq!(literal_env(&projected, "web"), literal_env(&before, "web"));
            prop_assert_eq!(image(&projected, "other"), image(&before, "other"));
            let web = next.deployments.iter().find(|d| d.metadata.name.as_deref() == Some("web")).unwrap();
            prop_assert_eq!(web.metadata.labels.as_ref().and_then(|l| l.get("team")), Some(&label));
            prop_assert_eq!(web.metadata.resource_version.as_deref(), Some("1"), "server fields stay");
        }
    }

    #[test]
    fn a_restart_changes_nothing_the_projection_shows() {
        let snapshot = snapshot(&[("A".to_owned(), "1".to_owned())]);
        let before = project(&snapshot);
        let plan = planned(&snapshot, vec![Change::Restart { workload: web() }]);
        let projected = project(&after(&snapshot, &plan));
        assert_eq!(literal_env(&projected, "web"), literal_env(&before, "web"));
        assert_eq!(projected.connections, before.connections);
        assert_eq!(
            plan.objects[0].raw_after["spec"]["template"]["metadata"]["annotations"]["kubectl.kubernetes.io/restartedAt"],
            json!("2026-10-15T00:00:00Z")
        );
    }
}
