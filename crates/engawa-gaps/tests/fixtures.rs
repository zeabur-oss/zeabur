//! Runs every case of the `gaps` fixture area against the diagnoses and the gap rules.

#[cfg(test)]
mod tests {
    use engawa_gaps::{
        BUILTIN, ClusterTraits, Event, GatewayRef, Input, Rule, diagnose, evaluate, snippet,
    };
    use engawa_graph::{Snapshot, project};
    use engawa_test_support::fixtures::{Fixture, compare, load};
    use k8s_openapi::jiff::Timestamp;
    use serde_json::{Value, from_value, to_value};

    /// The snapshot a cluster reader would build, the events and the `HTTPRoutes`, from the
    /// case's objects.
    fn read(fixture: &Fixture) -> (Snapshot, Vec<Event>, Vec<Value>) {
        let mut snapshot = Snapshot {
            namespace: fixture.case.params["namespace"]
                .as_str()
                .unwrap_or("demo")
                .to_owned(),
            now: fixture.case.params["now"]
                .as_str()
                .map(|t| t.parse::<Timestamp>().unwrap()),
            ..Snapshot::default()
        };
        let mut events = Vec::new();
        let mut http_routes = Vec::new();
        for object in &fixture.input {
            if snapshot.insert(object.clone()).unwrap() {
                continue;
            }
            match object["kind"].as_str().unwrap() {
                "Event" => events.push(Event::from_kube(&from_value(object.clone()).unwrap())),
                "HTTPRoute" => http_routes.push(object.clone()),
                kind => panic!("{}: unsupported kind {kind}", fixture.name),
            }
        }
        (snapshot, events, http_routes)
    }

    fn run(fixture: &Fixture) -> Value {
        let (snapshot, events, http_routes) = read(fixture);
        let params = &fixture.case.params;
        match fixture.case.operation.as_str() {
            "gaps.diagnose" => {
                let project = project(&snapshot);
                // The workload `service` names, or the only one.
                let name = params["service"].as_str();
                if name.is_none() {
                    assert_eq!(project.services.len(), 1, "{}: one workload", fixture.name);
                }
                let service = project
                    .services
                    .iter()
                    .find(|s| name.is_none_or(|name| s.workload.name == name))
                    .unwrap_or_else(|| panic!("{}: no workload {name:?}", fixture.name));
                to_value(diagnose(service, &snapshot, &events, &http_routes)).unwrap()
            }
            "gaps.rules" => {
                let project = project(&snapshot);
                let wanted: Vec<&str> = params["rules"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| r.as_str().unwrap())
                    .collect();
                let rules: Vec<&dyn Rule> = BUILTIN
                    .iter()
                    .copied()
                    .filter(|rule| wanted.contains(&rule.id()))
                    .collect();
                assert_eq!(rules.len(), wanted.len(), "{}: unknown rule", fixture.name);
                let cluster = ClusterTraits {
                    gateway_api: params["gatewayApi"].as_bool().unwrap_or(false),
                    retired_ingress_classes: params["retiredIngressClasses"]
                        .as_array()
                        .map_or_else(Default::default, |a| {
                            a.iter().map(|c| c.as_str().unwrap().to_owned()).collect()
                        }),
                };
                to_value(evaluate(
                    &rules,
                    &Input {
                        project: &project,
                        cluster: &cluster,
                    },
                ))
                .unwrap()
            }
            "gaps.snippets" => {
                let project = project(&snapshot);
                let cluster = ClusterTraits {
                    gateway_api: true,
                    retired_ingress_classes: std::collections::BTreeSet::new(),
                };
                let gateways: Vec<GatewayRef> =
                    params["gateways"].as_array().map_or_else(Vec::new, |a| {
                        a.iter()
                            .map(|g| GatewayRef {
                                namespace: g["namespace"].as_str().unwrap().to_owned(),
                                name: g["name"].as_str().unwrap().to_owned(),
                            })
                            .collect()
                    });
                let gaps = evaluate(
                    &BUILTIN,
                    &Input {
                        project: &project,
                        cluster: &cluster,
                    },
                );
                Value::Array(
                    gaps.iter()
                        .map(|gap| {
                            serde_json::json!({
                                "rule": gap.rule,
                                "fix": gap.fix.id,
                                "snippet": snippet(gap, &project, &gateways),
                            })
                        })
                        .collect(),
                )
            }
            other => panic!("{}: no runner for operation {other}", fixture.name),
        }
    }

    #[test]
    fn gaps_fixtures_pass() {
        let mut failures = Vec::new();
        let fixtures = load("gaps").unwrap();
        assert!(!fixtures.is_empty(), "no fixtures found");
        for fixture in &fixtures {
            let actual = run(fixture);
            match compare(&fixture.expected, &actual) {
                Ok(()) => println!("pass {}", fixture.name),
                Err(difference) => {
                    println!("FAIL {}: {difference}\n{actual:#}", fixture.name);
                    failures.push(format!("{}: {difference}", fixture.name));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {} failed:\n{}",
            failures.len(),
            fixtures.len(),
            failures.join("\n")
        );
    }
}
