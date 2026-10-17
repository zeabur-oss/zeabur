//! Times the in-memory projection on two fixed, generated distributions.
//!
//! Each Deployment has its own ConfigMap, Secret, PVC, Service and Ingress, and imports one
//! ConfigMap they all share, so a case of `n` Deployments holds `6n + 1` objects. Each Ingress has
//! one backend, its own Service. In `one-to-one` a Service's selector matches its own Deployment
//! only; in `overlap` every Service's selector matches every Deployment, so the relations grow
//! with the square of the Deployments.
//!
//! The snapshot is built before timing; no API request or JSON parsing is timed. `attribution`
//! times [`engawa_graph::attribution`], `project` times [`engawa_graph::project`], which includes
//! the attribution. Each is run once to warm up, then in 15 batches of enough runs to last about
//! 50 ms; the table gives the median, fastest and slowest batch per run.
//!
//! ```sh
//! cargo run --release -p engawa-graph --example projection-bench               # every case
//! cargo run --release -p engawa-graph --example projection-bench -- overlap:400 # one case
//! cargo run --release -p engawa-graph --example projection-bench -- --json DIR  # also writes
//!                                                     # each case's project, to compare outputs
//! ```

#![expect(
    clippy::print_stdout,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a development tool that prints a table; fixed generated input and batch count"
)]

use std::time::{Duration, Instant};

use engawa_graph::{Snapshot, attribution, project, relations};
use serde_json::json;

/// The cases run when none is named.
const CASES: [(Distribution, usize); 5] = [
    (Distribution::OneToOne, 100),
    (Distribution::OneToOne, 400),
    (Distribution::OneToOne, 2_000),
    (Distribution::Overlap, 100),
    (Distribution::Overlap, 400),
];

const BATCHES: usize = 15;
const BATCH_TARGET: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy)]
enum Distribution {
    OneToOne,
    Overlap,
}

impl Distribution {
    const fn name(self) -> &'static str {
        match self {
            Self::OneToOne => "one-to-one",
            Self::Overlap => "overlap",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "one-to-one" => Some(Self::OneToOne),
            "overlap" => Some(Self::Overlap),
            _ => None,
        }
    }
}

/// The snapshot of `deployments` Deployments in `distribution`.
fn snapshot(distribution: Distribution, deployments: usize) -> Snapshot {
    let mut snapshot = Snapshot {
        namespace: "bench".to_owned(),
        ..Snapshot::default()
    };
    let mut add = |object: serde_json::Value| {
        assert!(snapshot.insert(object).expect("a valid object"));
    };
    add(json!({
        "apiVersion": "v1", "kind": "ConfigMap",
        "metadata": {"name": "shared", "namespace": "bench"},
        "data": {"REGION": "eu"}
    }));
    for i in 0..deployments {
        let name = format!("app-{i:05}");
        let selector = match distribution {
            Distribution::OneToOne => json!({"app": name}),
            Distribution::Overlap => json!({"tier": "web"}),
        };
        add(json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {"name": name, "namespace": "bench"},
            "spec": {
                "selector": {"matchLabels": {"app": name}},
                "template": {
                    "metadata": {"labels": {"app": name, "tier": "web"}},
                    "spec": {
                        "containers": [{
                            "name": "app",
                            "image": "registry.example/app:1.0",
                            "env": [
                                {"name": "MODE", "value": "production"},
                                {"name": "PASSWORD", "valueFrom": {"secretKeyRef": {"name": format!("{name}-secret"), "key": "password"}}}
                            ],
                            "envFrom": [
                                {"configMapRef": {"name": format!("{name}-config")}},
                                {"configMapRef": {"name": "shared"}}
                            ],
                            "volumeMounts": [{"name": "data", "mountPath": "/data"}]
                        }],
                        "volumes": [{"name": "data", "persistentVolumeClaim": {"claimName": format!("{name}-data")}}]
                    }
                }
            }
        }));
        add(json!({
            "apiVersion": "v1", "kind": "ConfigMap",
            "metadata": {"name": format!("{name}-config"), "namespace": "bench"},
            "data": {"NAME": name}
        }));
        add(json!({
            "apiVersion": "v1", "kind": "Secret",
            "metadata": {"name": format!("{name}-secret"), "namespace": "bench"},
            "type": "Opaque",
            "data": {"password": "c2VjcmV0"}
        }));
        add(json!({
            "apiVersion": "v1", "kind": "PersistentVolumeClaim",
            "metadata": {"name": format!("{name}-data"), "namespace": "bench"},
            "spec": {"accessModes": ["ReadWriteOnce"], "resources": {"requests": {"storage": "1Gi"}}}
        }));
        add(json!({
            "apiVersion": "v1", "kind": "Service",
            "metadata": {"name": name, "namespace": "bench"},
            "spec": {"selector": selector, "ports": [{"name": "http", "port": 80}]}
        }));
        add(json!({
            "apiVersion": "networking.k8s.io/v1", "kind": "Ingress",
            "metadata": {"name": name, "namespace": "bench"},
            "spec": {"rules": [{"host": format!("{name}.example.com"), "http": {"paths": [{
                "path": "/", "pathType": "Prefix",
                "backend": {"service": {"name": name, "port": {"number": 80}}}
            }]}}]}
        }));
    }
    snapshot
}

/// Median, fastest and slowest time per run of `run`, over the batches.
fn time<T>(mut run: impl FnMut() -> T) -> [Duration; 3] {
    let start = Instant::now();
    drop(run());
    let once = start.elapsed().max(Duration::from_micros(1));
    let runs = u32::try_from(BATCH_TARGET.as_nanos() / once.as_nanos())
        .unwrap_or(u32::MAX)
        .max(1);
    let mut batches: Vec<Duration> = (0..BATCHES)
        .map(|_| {
            let start = Instant::now();
            for _ in 0..runs {
                drop(run());
            }
            start.elapsed() / runs
        })
        .collect();
    batches.sort();
    [batches[BATCHES / 2], batches[0], batches[BATCHES - 1]]
}

fn ms(duration: Duration) -> String {
    format!("{:.2}", duration.as_secs_f64() * 1_000.0)
}

fn main() {
    let mut json_dir = None;
    let mut cases = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--json" {
            json_dir = Some(std::path::PathBuf::from(args.next().expect("--json DIR")));
            continue;
        }
        let (distribution, size) = arg.split_once(':').expect("CASE is DISTRIBUTION:N");
        cases.push((
            Distribution::parse(distribution).expect("one-to-one or overlap"),
            size.parse().expect("a number of Deployments"),
        ));
    }
    if cases.is_empty() {
        cases = CASES.to_vec();
    }
    println!(
        "| distribution | deployments | objects | relations | attribution ms (min-max) | project ms (min-max) |"
    );
    println!("|---|---:|---:|---:|---:|---:|");
    for (distribution, deployments) in cases {
        let snapshot = snapshot(distribution, deployments);
        let objects = 6 * deployments + 1;
        let relations = relations(&snapshot).len();
        if let Some(dir) = &json_dir {
            let path = dir.join(format!("{}-{deployments}.json", distribution.name()));
            let text = serde_json::to_vec(&project(&snapshot)).expect("a serialisable project");
            std::fs::write(&path, text).expect("a writable --json directory");
        }
        let [a_median, a_min, a_max] = time(|| attribution(&snapshot));
        let [p_median, p_min, p_max] = time(|| project(&snapshot));
        println!(
            "| {} | {deployments} | {objects} | {relations} | {} ({}-{}) | {} ({}-{}) |",
            distribution.name(),
            ms(a_median),
            ms(a_min),
            ms(a_max),
            ms(p_median),
            ms(p_min),
            ms(p_max),
        );
    }
}
