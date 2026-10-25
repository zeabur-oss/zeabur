//! How much of real-world input the projection places: the charts in `fixtures/charts`.
//!
//! An object is *placed* when it appears inside a service: workloads are services themselves,
//! and a ConfigMap, Secret, PVC, Service or Ingress is placed when at least one workload uses it
//! (owned or shared). Objects no workload uses are listed as the project's other resources; the
//! projection does not lose them, but a person sees them outside any service. Coverage is placed
//! objects over all objects of those kinds; it must be at least 80%.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fmt::Write as _;
    use std::path::Path;

    use serde_json::Value;
    use zeabur_graph::{Snapshot, project};
    use zeabur_model::{Ownership, ResourceKind};

    const WORKLOADS: [&str; 5] = ["Deployment", "StatefulSet", "DaemonSet", "Job", "CronJob"];
    const RESOURCES: [&str; 5] = [
        "ConfigMap",
        "Secret",
        "PersistentVolumeClaim",
        "Service",
        "Ingress",
    ];

    fn kind(object: &Value) -> &str {
        object
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default()
    }

    fn snapshot(namespace: &str, objects: &[Value]) -> Snapshot {
        let mut snapshot = Snapshot {
            namespace: namespace.to_owned(),
            ..Snapshot::default()
        };
        for object in objects {
            snapshot.insert(object.clone()).unwrap();
        }
        snapshot
    }

    const fn resource_kind(kind: ResourceKind) -> &'static str {
        match kind {
            ResourceKind::ConfigMap => "ConfigMap",
            ResourceKind::Ingress => "Ingress",
            ResourceKind::PersistentVolumeClaim => "PersistentVolumeClaim",
            ResourceKind::Secret => "Secret",
            ResourceKind::Service => "Service",
        }
    }

    #[derive(Default)]
    struct Count {
        total: usize,
        placed: usize,
        unplaced: Vec<String>,
    }

    fn measure(file: &Path) -> Count {
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
        let mut count = Count::default();
        for (namespace, objects) in by_namespace {
            let projection = project(&snapshot(&namespace, &objects));
            let workloads = objects
                .iter()
                .filter(|o| WORKLOADS.contains(&kind(o)))
                .count();
            count.total += workloads;
            count.placed += projection.services.len().min(workloads);
            let expected = objects
                .iter()
                .filter(|o| RESOURCES.contains(&kind(o)))
                .count();
            count.total += expected;
            for resource in &projection.resources {
                match resource.attribution.ownership {
                    Ownership::Owned | Ownership::Shared => count.placed += 1,
                    Ownership::Orphan => count.unplaced.push(format!(
                        "{namespace}/{} {}",
                        resource_kind(resource.kind),
                        resource.name
                    )),
                }
            }
            // Resources the projection does not list at all (machinery Secrets, for example)
            // count as not placed.
            let listed = projection.resources.len();
            if expected > listed {
                count
                    .unplaced
                    .push(format!("{namespace}: {} not listed", expected - listed));
            }
        }
        count
    }

    #[test]
    fn the_projection_places_at_least_80_percent_of_real_charts() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/charts");
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "yaml"))
            .collect();
        files.sort();
        // One chart per component of the bundled catalogue, named `<capability>__<id>`.
        let mut charts: Vec<String> = files
            .iter()
            .map(|f| f.file_stem().unwrap().to_string_lossy().into_owned())
            .collect();
        let index: Value = serde_json::from_slice(
            &std::fs::read(concat!(env!("ZEABUR_CATALOG_RELEASE"), "/index.json")).unwrap(),
        )
        .unwrap();
        let mut components: Vec<String> = index["components"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_str().unwrap().replace('/', "__"))
            .collect();
        charts.sort();
        components.sort();
        assert_eq!(
            charts, components,
            "the corpus has one chart per catalogue component"
        );

        let mut report = String::from("chart                                   placed/total\n");
        let (mut total, mut placed) = (0, 0);
        for file in &files {
            let count = measure(file);
            let name = file.file_stem().unwrap().to_string_lossy();
            let _ignored = writeln!(report, "{name:<40} {:>3}/{:<3}", count.placed, count.total);
            for item in &count.unplaced {
                let _ignored = writeln!(report, "    not placed: {item}");
            }
            total += count.total;
            placed += count.placed;
        }
        let coverage =
            f64::from(u32::try_from(placed).unwrap()) / f64::from(u32::try_from(total).unwrap());
        let _ignored = writeln!(report, "total {placed}/{total} = {:.1}%", coverage * 100.0);
        println!("{report}");
        assert!(coverage >= 0.80, "coverage below 80%:\n{report}");
    }
}
