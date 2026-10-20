//! Times the in-memory projection on two fixed, generated distributions.
//!
//! The namespaces come from `tests/generated`: each Deployment has its own ConfigMap, Secret, PVC,
//! Service and Ingress, and imports one ConfigMap they all share, so a case of `n` Deployments
//! holds `6n + 1` objects. In `one-to-one` a Service's selector matches its own Deployment only;
//! in `overlap` every Service's selector matches every Deployment, so the relations grow with the
//! square of the Deployments.
//!
//! The snapshot is built before timing; no API request or JSON parsing is timed. `attribution`
//! times [`zeabur_graph::attribution`], `project` times [`zeabur_graph::project`], which includes
//! the attribution, and `project + JSON` adds serialising it with `serde_json` to a byte counter.
//! Each is run once to warm up, then in 15 batches of enough runs to last about
//! 50 ms; the table gives the median, fastest and slowest batch per run.
//!
//! `JSON MiB` is the length of the project serialised with `serde_json`; the `services`,
//! `endpoints`, `routes` and `resources` MiB are the lengths of those lists alone.
//!
//! ```sh
//! cargo run --release -p zeabur-graph --example projection-bench               # every case
//! cargo run --release -p zeabur-graph --example projection-bench -- overlap:400 # one case
//! cargo run --release -p zeabur-graph --example projection-bench -- --json DIR  # also writes
//!                                                     # each case's project, to compare outputs
//! ```

#![expect(
    clippy::print_stdout,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a development tool that prints a table; fixed generated input and batch count"
)]

use std::time::{Duration, Instant};

use zeabur_graph::{attribution, project, relations};

#[path = "../tests/generated/mod.rs"]
mod generated;

use generated::{Distribution, snapshot};

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

/// Counts the bytes written to it, so a large project is measured without being held in memory.
#[derive(Default)]
struct Counter(usize);

impl std::io::Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The length of `value` serialised as JSON, in MiB.
fn mib(value: &impl serde::Serialize) -> String {
    let mut counter = Counter::default();
    serde_json::to_writer(&mut counter, value).expect("a serialisable project");
    #[expect(
        clippy::cast_precision_loss,
        reason = "a size printed with one decimal place"
    )]
    let size = counter.0 as f64 / 1_048_576.0;
    format!("{size:.1}")
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
        "| distribution | deployments | objects | relations | attribution ms (min-max) | project ms (min-max) | project + JSON ms | JSON MiB | services MiB | endpoints MiB | routes MiB | resources MiB |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
    for (distribution, deployments) in cases {
        let snapshot = snapshot(distribution, deployments).expect("valid objects");
        let objects = 6 * deployments + 1;
        let relations = relations(&snapshot).len();
        let projected = project(&snapshot);
        let sizes = [
            mib(&projected),
            mib(&projected.services),
            mib(&projected.endpoints),
            mib(&projected.routes),
            mib(&projected.resources),
        ];
        if let Some(dir) = &json_dir {
            let path = dir.join(format!("{}-{deployments}.json", distribution.name()));
            let text = serde_json::to_vec(&projected).expect("a serialisable project");
            std::fs::write(&path, text).expect("a writable --json directory");
        }
        drop(projected);
        let [a_median, a_min, a_max] = time(|| attribution(&snapshot));
        let [p_median, p_min, p_max] = time(|| project(&snapshot));
        let [j_median, _, _] = time(|| {
            let mut counter = Counter::default();
            serde_json::to_writer(&mut counter, &project(&snapshot))
                .expect("a serialisable project");
            counter.0
        });
        println!(
            "| {} | {deployments} | {objects} | {relations} | {} ({}-{}) | {} ({}-{}) | {} | {} | {} | {} | {} | {} |",
            distribution.name(),
            ms(a_median),
            ms(a_min),
            ms(a_max),
            ms(p_median),
            ms(p_min),
            ms(p_max),
            ms(j_median),
            sizes[0],
            sizes[1],
            sizes[2],
            sizes[3],
            sizes[4],
        );
    }
}
