//! Runs every case of the `modules` fixture area.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::{Value, from_value, to_value};
    use zeabur_kube::DynamicObject;
    use zeabur_modules::{backups, cloudnative_pg, k8up, network_policy, velero};
    use zeabur_test_support::fixtures::{Fixture, compare};
    use zeabur_test_support::load;

    fn objects(fixture: &Fixture, kind: &str, api_group: &str) -> Vec<DynamicObject> {
        fixture
            .input
            .iter()
            .filter(|o| {
                o["kind"] == kind
                    && o["apiVersion"]
                        .as_str()
                        .is_some_and(|v| v.starts_with(api_group))
            })
            .map(|o| from_value(o.clone()).unwrap())
            .collect()
    }

    fn strings(value: &Value) -> BTreeMap<String, String> {
        from_value(value.clone()).unwrap_or_default()
    }

    fn reachability(fixture: &Fixture) -> Value {
        let (_, _, reach) = reach(fixture);
        to_value(reach).unwrap()
    }

    fn network_gaps(fixture: &Fixture) -> Value {
        let (namespace, workloads, reach) = reach(fixture);
        to_value(network_policy::gaps(&namespace, &reach, &workloads)).unwrap()
    }

    fn reach(
        fixture: &Fixture,
    ) -> (
        String,
        Vec<network_policy::Workload>,
        network_policy::Reachability,
    ) {
        let namespace = fixture.case.params["namespace"].as_str().unwrap();
        let id = |name: &str| format!("projects/{namespace}/services/deployment/{name}");
        let workloads: Vec<network_policy::Workload> = fixture
            .objects("Deployment")
            .map(|d| network_policy::Workload {
                id: id(d["metadata"]["name"].as_str().unwrap()),
                labels: strings(&d["spec"]["template"]["metadata"]["labels"]),
            })
            .collect();
        let connections: Vec<(String, String)> = fixture.case.params["connections"]
            .as_array()
            .map_or_else(Vec::new, |pairs| {
                pairs
                    .iter()
                    .map(|p| (id(p[0].as_str().unwrap()), id(p[1].as_str().unwrap())))
                    .collect()
            });
        let policies = objects(fixture, "NetworkPolicy", "networking.k8s.io");
        let reach = network_policy::reachability(network_policy::Input {
            namespace,
            workloads: &workloads,
            policies: &policies,
            connections: &connections,
        });
        (namespace.to_owned(), workloads, reach)
    }

    fn coverage(fixture: &Fixture) -> Value {
        to_value(covered(fixture)).unwrap()
    }

    fn backup_gaps(fixture: &Fixture) -> Value {
        let installed = &fixture.case.params["installed"];
        let installed = backups::Installed {
            k8up: installed["k8up"].as_bool().unwrap_or(false),
            velero: installed["velero"].as_bool().unwrap_or(false),
        };
        to_value(backups::gaps(&covered(fixture), installed)).unwrap()
    }

    fn covered(fixture: &Fixture) -> Vec<backups::Coverage> {
        let namespace = fixture.case.params["namespace"].as_str().unwrap();
        let claims: Vec<k8up::Volume> = fixture
            .objects("PersistentVolumeClaim")
            .map(|c| k8up::Volume {
                name: c["metadata"]["name"].as_str().unwrap().to_owned(),
                labels: strings(&c["metadata"]["labels"]),
                annotations: strings(&c["metadata"]["annotations"]),
            })
            .collect();
        let names: Vec<String> = claims.iter().map(|c| c.name.clone()).collect();
        let databases =
            cloudnative_pg::databases(&objects(fixture, "Cluster", "postgresql.cnpg.io"));
        let findings = [
            k8up::findings(
                namespace,
                &objects(fixture, "Schedule", "k8up.io"),
                &objects(fixture, "Backup", "k8up.io"),
                &claims,
            ),
            velero::findings(
                namespace,
                &objects(fixture, "Schedule", "velero.io"),
                &objects(fixture, "Backup", "velero.io"),
                &names,
            ),
            cloudnative_pg::findings(
                namespace,
                &objects(fixture, "ScheduledBackup", "postgresql.cnpg.io"),
                &objects(fixture, "Backup", "postgresql.cnpg.io"),
            ),
        ];
        let targets = backups::targets(namespace, &names, &databases);
        backups::coverage(&targets, &findings)
    }

    #[test]
    fn module_fixtures_pass() {
        let mut failures = Vec::new();
        let cases = load!("modules").expect("fixtures load");
        assert!(!cases.is_empty(), "no modules fixtures");
        for fixture in &cases {
            let actual = match fixture.case.operation.as_str() {
                "modules.networkReachability" => reachability(fixture),
                "modules.backups" => coverage(fixture),
                "modules.networkGaps" => network_gaps(fixture),
                "modules.backupGaps" => backup_gaps(fixture),
                other => panic!("{}: unknown operation {other}", fixture.name),
            };
            if let Err(difference) = compare(&fixture.expected, &actual) {
                failures.push(format!("{}: {difference}", fixture.name));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
