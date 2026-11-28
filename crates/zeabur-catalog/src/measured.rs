//! What a template's or a component's pods use when idle.
//!
//! The numbers come from Zeabur Catalog's `measurements/templates/<id>.json` and
//! `measurements/components/<id>.json`, which its nightly end-to-end run writes. Only passed runs
//! count. A run lists one pod of each workload; every template runs one pod per workload
//! (`replicas: 1`, `instances: 1`), so the rows are the template's pods.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use zeabur_catalog_format::api::{Environment, MeasuredDistribution, MeasurementFile, RunResult};
use zeabur_catalog_format::common::Profile;
use zeabur_kube::k8s_openapi::jiff::civil::Date;
use zeabur_model::kernel::quantity;

use crate::bundle;

/// What a template's or a component's pods used once idle, on a fresh cluster after its smoke
/// checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IdleUse {
    /// The pods' working set in bytes.
    pub memory_bytes: u64,
    /// The pods' CPU in millicores.
    pub cpu_millicores: u64,
    /// Where it was measured: `<distribution>-<profile>-<architecture>-<Kubernetes version>`.
    pub environment: String,
    /// The day it was measured.
    pub measured_at: Date,
    /// Each workload's pod.
    pub pods: Vec<MeasuredPod>,
}

/// What one pod of a workload used once idle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MeasuredPod {
    /// The workload or database object (`miniflux-db`).
    pub workload: String,
    /// Its working set in bytes.
    pub memory_bytes: u64,
    /// Its CPU in millicores.
    pub cpu_millicores: u64,
}

/// Where `run` was measured: `<distribution>-<profile>-<architecture>-<Kubernetes version>`.
fn environment(run: &Environment) -> String {
    let distribution = match run.distribution {
        MeasuredDistribution::Kind(_) => "kind",
        MeasuredDistribution::Host(d) => d.as_str(),
    };
    let mut name = format!(
        "{distribution}-{}-{}",
        run.profile.as_str(),
        run.platform.architecture
    );
    if let Some(version) = &run.kubernetes_version {
        name.push('-');
        name.push_str(version);
    }
    name
}

/// What the template `id` uses when idle, as the embedded catalogue measured it; `None` when it
/// has no measurement.
#[must_use]
pub fn template_idle(id: &str) -> Option<IdleUse> {
    bundle::TEMPLATE_MEASUREMENTS
        .iter()
        .find(|(name, _)| *name == id)
        .and_then(|(_, text)| idle_from(text))
}

/// What the component `id` uses when idle, as the embedded catalogue measured it; `None` when it
/// has no measurement.
#[must_use]
pub fn component_idle(id: &str) -> Option<IdleUse> {
    bundle::COMPONENT_MEASUREMENTS
        .iter()
        .find(|(name, _)| *name == id)
        .and_then(|(_, text)| idle_from(text))
}

/// The memory a component uses: its measured idle use plus one unit of its heaviest work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComponentMemory {
    /// What its pods used once idle.
    pub idle: IdleUse,
    /// Its work whose `use` has the most memory; absent when the catalogue lists none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work: Option<WorkUse>,
    /// The idle memory plus the work's: what the component uses while it does one unit of that
    /// work, such as one build.
    pub total_bytes: u64,
}

/// What one unit of a component's work uses on top of its idle use, as the catalogue declares it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkUse {
    /// The work's name in the catalogue (`build`).
    pub name: String,
    /// Its memory in bytes.
    pub memory_bytes: u64,
}

/// `idle` plus the work in `resources` whose `use` has the most memory.
#[must_use]
pub fn component_memory(
    idle: IdleUse,
    resources: Option<&zeabur_catalog_format::component::Resources>,
) -> ComponentMemory {
    let work = resources
        .into_iter()
        .flat_map(|r| &r.work)
        .filter_map(|work| {
            let memory = work.uses.as_ref()?.memory.as_deref()?;
            Some(WorkUse {
                name: work.name.clone(),
                memory_bytes: quantity::bytes(memory)?,
            })
        })
        .max_by_key(|work| work.memory_bytes);
    ComponentMemory {
        total_bytes: idle.memory_bytes + work.as_ref().map_or(0, |w| w.memory_bytes),
        idle,
        work,
    }
}

/// The newest passed run of a measurement file on a single-node kind cluster on linux/amd64, else
/// its newest passed run.
fn idle_from(text: &str) -> Option<IdleUse> {
    let file: MeasurementFile = serde_json::from_str(text).ok()?;
    let passed = file
        .environments
        .iter()
        .filter(|e| e.result == RunResult::Passed);
    let preferred = |e: &&Environment| {
        matches!(e.distribution, MeasuredDistribution::Kind(_))
            && e.profile == Profile::SingleNode
            && e.platform.os == "linux"
            && e.platform.architecture == "amd64"
    };
    let measured = passed
        .clone()
        .filter(preferred)
        .max_by(|a, b| a.measured_at.cmp(&b.measured_at))
        .or_else(|| passed.max_by(|a, b| a.measured_at.cmp(&b.measured_at)))?;
    let pods = measured
        .pods
        .iter()
        .map(|pod| {
            Some(MeasuredPod {
                workload: pod.workload.clone(),
                memory_bytes: quantity::bytes(&pod.memory)?,
                cpu_millicores: quantity::millicores(&pod.cpu)?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(IdleUse {
        memory_bytes: pods.iter().map(|p| p.memory_bytes).sum(),
        cpu_millicores: pods.iter().map(|p| p.cpu_millicores).sum(),
        environment: environment(measured),
        measured_at: measured.measured_at.parse().ok()?,
        pods,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use zeabur_kube::k8s_openapi::jiff::civil::date;

    use super::*;

    fn run(distribution: &str, arch: &str, memory: &str, result: &str, at: &str) -> Value {
        json!({
            "distribution": distribution,
            "profile": "single-node",
            "platform": {"os": "linux", "architecture": arch},
            "kubernetesVersion": "v1.36.4",
            "pods": [
                {"workload": "app", "kind": "Deployment", "memory": memory, "cpu": "3m"},
                {"workload": "app-db", "kind": "Cluster", "memory": "10Mi", "cpu": "1m"},
            ],
            "result": result,
            "measuredAt": at,
        })
    }

    fn bundle_of(environments: &[Value]) -> String {
        json!({"template": "ghost", "environments": environments}).to_string()
    }

    #[test]
    fn takes_the_newest_passed_single_node_kind_amd64_run() {
        let text = bundle_of(&[
            run("k3s", "amd64", "900Mi", "passed", "2026-10-26"),
            run("kind", "amd64", "300Mi", "passed", "2026-10-11"),
            run("kind", "amd64", "300Mi", "passed", "2026-10-25"),
            run("kind", "amd64", "0", "failed", "2026-10-27"),
            run("kind", "arm64", "280Mi", "passed", "2026-10-28"),
        ]);
        assert_eq!(
            idle_from(&text),
            Some(IdleUse {
                memory_bytes: 310 * 1024 * 1024,
                cpu_millicores: 4,
                environment: "kind-single-node-amd64-v1.36.4".to_owned(),
                measured_at: date(2026, 10, 25),
                pods: vec![
                    MeasuredPod {
                        workload: "app".to_owned(),
                        memory_bytes: 300 * 1024 * 1024,
                        cpu_millicores: 3,
                    },
                    MeasuredPod {
                        workload: "app-db".to_owned(),
                        memory_bytes: 10 * 1024 * 1024,
                        cpu_millicores: 1,
                    },
                ],
            })
        );
    }

    #[test]
    fn the_newest_day_wins_and_reads_back_as_a_date() {
        let text = bundle_of(&[
            run("kind", "amd64", "300Mi", "passed", "2026-11-10"),
            run("kind", "amd64", "900Mi", "passed", "2026-11-09"),
        ]);
        let idle = idle_from(&text).unwrap();
        assert_eq!(idle.measured_at, date(2026, 11, 10));
        assert_eq!(
            serde_json::to_value(idle.measured_at).unwrap(),
            json!("2026-11-10")
        );
    }

    #[test]
    fn falls_back_to_another_passed_run() {
        let text = bundle_of(&[
            run("k3s", "arm64", "12Mi", "passed", "2026-10-11"),
            run("kind", "amd64", "0", "skipped", "2026-10-12"),
        ]);
        let idle = idle_from(&text).unwrap();
        assert_eq!(idle.environment, "k3s-single-node-arm64-v1.36.4");
        assert_eq!(idle.memory_bytes, 22 * 1024 * 1024);
    }

    #[test]
    fn every_embedded_passed_run_reads() {
        for (id, text) in bundle::TEMPLATE_MEASUREMENTS {
            if text.contains(r#""result":"passed""#) || text.contains(r#""result": "passed""#) {
                let idle = idle_from(text);
                assert!(idle.is_some_and(|i| i.memory_bytes > 0), "{id}");
            }
        }
    }

    /// The rows are the template's pods only while each workload runs one pod.
    #[test]
    fn every_embedded_template_runs_one_pod_per_workload() {
        for (id, _, manifests) in bundle::TEMPLATES {
            for line in manifests.lines() {
                let line = line.trim();
                if let Some(count) = line
                    .strip_prefix("replicas:")
                    .or_else(|| line.strip_prefix("instances:"))
                {
                    assert_eq!(count.trim(), "1", "{id}: {line}");
                }
            }
        }
    }

    fn work(name: &str, memory: Option<&str>) -> zeabur_catalog_format::component::Work {
        serde_json::from_value(json!({"name": name, "use": memory.map(|m| json!({"memory": m}))}))
            .unwrap()
    }

    #[test]
    fn a_component_uses_its_idle_memory_plus_its_heaviest_work() {
        let idle = idle_from(&bundle_of(&[run(
            "k3s",
            "amd64",
            "40Mi",
            "passed",
            "2026-11-25",
        )]))
        .unwrap();
        let resources = zeabur_catalog_format::component::Resources {
            work: vec![
                work("load", Some("64Mi")),
                work("build", Some("500Mi")),
                work("pull", None),
            ],
        };
        let memory = component_memory(idle.clone(), Some(&resources));
        assert_eq!(
            memory.work,
            Some(WorkUse {
                name: "build".to_owned(),
                memory_bytes: 500 * 1024 * 1024,
            })
        );
        assert_eq!(memory.total_bytes, 550 * 1024 * 1024);
        let idle_only = component_memory(idle, None);
        assert_eq!(
            (idle_only.work, idle_only.total_bytes),
            (None, 50 * 1024 * 1024)
        );
    }

    /// `docs/deploy/git.mdx` gives the memory Zeabur Builder uses with one build running, rounded
    /// up to 100 MiB, and must give the figure the embedded catalogue works out.
    #[test]
    fn the_git_docs_give_the_builder_figure_of_the_catalogue() {
        let builder = crate::catalogue()
            .unwrap()
            .into_iter()
            .find(|c| c.id == "zeabur-builder")
            .unwrap();
        let memory = builder.memory.unwrap();
        assert_eq!(memory.work.map(|w| w.name).as_deref(), Some("build"));
        let mib = memory.total_bytes.div_ceil(100 * 1024 * 1024) * 100;
        let docs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
        for page in [
            "deploy/git.mdx",
            "zh-TW/deploy/git.mdx",
            "zh-CN/deploy/git.mdx",
        ] {
            let text = std::fs::read_to_string(docs.join(page)).unwrap();
            assert!(
                text.contains(&format!("about {mib} MiB"))
                    || text.contains(&format!("約用 {mib} MiB"))
                    || text.contains(&format!("约用 {mib} MiB")),
                "{page} does not give Zeabur Builder's figure, about {mib} MiB"
            );
        }
    }

    #[test]
    fn nothing_without_a_readable_measurement() {
        assert_eq!(idle_from(&bundle_of(&[])), None);
        assert_eq!(idle_from("{"), None);
        let unreadable = run("kind", "amd64", "lots", "passed", "2026-10-11");
        assert_eq!(idle_from(&bundle_of(&[unreadable])), None);
    }
}
