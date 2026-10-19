//! The diagnoses of real charts, the corpus in `fixtures/charts`, read as installed: no chart
//! there leaves a workload that something sends to without a Service, so `noService` must not
//! fire on any. Some of their workloads declare ports and have no Service (the Vertical Pod
//! Autoscaler's recommender, Falco, Velero's node agent: metrics and health ports read on the
//! pod); a declared port alone is no sign one is wanted.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use engawa_gaps::diagnose;
    use engawa_graph::{Snapshot, Workload, project, workloads};
    use engawa_model::WorkloadKind;
    use serde_json::{Value, from_value};

    fn snapshot(namespace: &str, objects: &[Value]) -> Snapshot {
        let mut snapshot = Snapshot {
            namespace: namespace.to_owned(),
            ..Snapshot::default()
        };
        for object in objects {
            let value = object.clone();
            match object["kind"].as_str().unwrap_or_default() {
                "Deployment" => snapshot.deployments.push(from_value(value).unwrap()),
                "StatefulSet" => snapshot.stateful_sets.push(from_value(value).unwrap()),
                "DaemonSet" => snapshot.daemon_sets.push(from_value(value).unwrap()),
                "Job" => snapshot.jobs.push(from_value(value).unwrap()),
                "CronJob" => snapshot.cron_jobs.push(from_value(value).unwrap()),
                "Service" => snapshot.services.push(from_value(value).unwrap()),
                "Ingress" => snapshot.ingresses.push(from_value(value).unwrap()),
                _ => {}
            }
        }
        snapshot
    }

    #[test]
    fn no_chart_of_the_corpus_is_missing_a_service() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/charts");
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "yaml"))
            .collect();
        files.sort();
        assert!(!files.is_empty(), "no charts in {}", dir.display());

        let (mut services, mut ports_without_service, mut fired) = (0, 0, Vec::new());
        for file in &files {
            let text = std::fs::read_to_string(file).unwrap();
            let documents: Vec<Value> = serde_saphyr::from_multiple(&text).unwrap();
            let mut by_namespace: BTreeMap<String, Vec<Value>> = BTreeMap::new();
            for object in documents.into_iter().filter(|o| !o.is_null()) {
                let namespace = object
                    .pointer("/metadata/namespace")
                    .and_then(Value::as_str)
                    .unwrap_or("default")
                    .to_owned();
                by_namespace.entry(namespace).or_default().push(object);
            }
            for (namespace, objects) in by_namespace {
                let snapshot = snapshot(&namespace, &objects);
                let http_routes: Vec<Value> = objects
                    .iter()
                    .filter(|o| o["kind"] == "HTTPRoute")
                    .cloned()
                    .collect();
                for service in &project(&snapshot).services {
                    services += 1;
                    let runs = matches!(
                        service.workload.kind,
                        WorkloadKind::Job | WorkloadKind::CronJob
                    );
                    let declares_ports = workloads(&snapshot)
                        .into_iter()
                        .filter(|w| {
                            w.kind() == service.workload.kind && w.name() == service.workload.name
                        })
                        .filter_map(Workload::pod_spec)
                        .flat_map(|spec| &spec.containers)
                        .any(|c| c.ports.as_ref().is_some_and(|p| !p.is_empty()));
                    if !runs && service.endpoint_names.is_empty() && declares_ports {
                        ports_without_service += 1;
                    }
                    if diagnose(service, &snapshot, &[], &http_routes)
                        .iter()
                        .any(|d| d.cause.key == "diagnosis.unreachable.cause.noService")
                    {
                        fired.push(format!("{namespace}/{}", service.workload.name));
                    }
                }
            }
        }
        let report = format!(
            "{services} workloads, {ports_without_service} declare ports without a Service, \
             noService on {}: {fired:?}",
            fired.len()
        );
        println!("{report}");
        assert!(fired.is_empty(), "{report}");
    }
}
