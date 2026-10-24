//! What each namespace's pods reserve, may use at most and use now, summed from one cluster-wide
//! read of pods and one of metrics-server's `PodMetrics`.
//!
//! Requests and limits follow `capacity::pod_amounts`, as the nodes do. A pod without a limit adds
//! nothing to the namespace's limit; `unlimited` counts those pods, so a client can tell a low
//! limit from a missing one. Usage is approximate, "meant only for autoscaling purposes"
//! [metrics-server], and leaves out pods that metrics-server has no sample for yet (see
//! `zeabur_modules::metrics_server`).
//!
//! The person needs `list` on `pods` in all namespaces; a refusal is this endpoint's `403`.
//! `list` on `pods.metrics.k8s.io` adds usage, and `list` on `namespaces` tells projects apart.
//! A refusal of either leaves that part out and names it in `unreadable`.
//!
//! [metrics-server]: https://github.com/kubernetes-sigs/metrics-server

use std::collections::BTreeMap;

use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::names::{LABEL_PROJECT, is_system_namespace};
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::ClusterError;
use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
use zeabur_model::{Origin, ProjectDetails};
use zeabur_modules::Reading;

use super::capacity::{self, Amount};
use super::{WIDE_LIMIT, modules, optional, refused};
use crate::error::{ApiError, list_read};
use crate::handlers::Person;
use crate::views::{MetricsState, PodScan, Resources, UnreadableView};

/// How many pods set no limit, for each resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Unlimited {
    /// Pods without a CPU limit.
    pub cpu: u64,
    /// Pods without a memory limit.
    pub memory: u64,
}

/// One namespace's pods together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceUsage {
    /// Its Namespace.
    pub origin: Origin,
    /// The namespace.
    pub namespace: String,
    /// A project: the Namespace has the label `zeabur.com/project`. Absent when namespaces
    /// cannot be listed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adopted: Option<bool>,
    /// A namespace Kubernetes keeps for itself (`kube-` prefix).
    pub system: bool,
    /// A project's display name, when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Its unfinished pods that were read.
    pub pods: u64,
    /// What those pods reserve.
    pub requests: Resources,
    /// What those pods may use at most; a pod without a limit adds nothing.
    pub limits: Resources,
    /// Pods that set no limit.
    pub unlimited: Unlimited,
    /// What its pods use now, with `pods` the number measured; absent unless `metrics` is
    /// `available`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Resources>,
}

/// Every namespace together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UsageTotals {
    /// Reserved.
    pub requests: Resources,
    /// Limits.
    pub limits: Resources,
    /// In use; absent unless `metrics` is `available`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Resources>,
}

/// What each namespace reserves and uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterUsage {
    /// `cluster/usage`.
    pub id: String,
    /// Each namespace with a pod or a sample, by name.
    pub items: Vec<NamespaceUsage>,
    /// Every namespace together.
    pub totals: UsageTotals,
    /// `complete`, or `partial` when the read stopped at 5,000 pods.
    pub pod_scan: PodScan,
    /// How many unfinished pods were read.
    pub pods_scanned: usize,
    /// Whether usage could be read.
    pub metrics: MetricsState,
    /// When the newest sample was taken (RFC 3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// The window CPU is averaged over, as metrics-server writes it (`15s`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<String>,
    /// What could not be read.
    pub unreadable: Vec<UnreadableView>,
}

#[derive(Default)]
struct Sum {
    pods: u64,
    requests: Amount,
    limits: Amount,
    unlimited: Unlimited,
    usage: Amount,
    sampled: u64,
}

const fn resources(amount: Amount, pods: Option<u64>) -> Resources {
    Resources {
        cpu_millicores: amount.cpu,
        memory_bytes: amount.memory,
        pods,
    }
}

/// Adds each sample to its namespace; returns whether usage was read, and the newest sample's
/// timestamp and window.
fn add_usage(
    metrics: Result<Reading<UnreadableView>, ClusterError>,
    sums: &mut BTreeMap<String, Sum>,
    unreadable: &mut Vec<UnreadableView>,
) -> (MetricsState, Option<String>, Option<String>) {
    let mut timestamp = None;
    let mut window = None;
    let state = match metrics {
        Ok(Reading::Objects(objects)) => {
            let samples = zeabur_modules::metrics_server::pod_usage(&objects);
            if let Some(newest) = samples.iter().max_by(|a, b| a.timestamp.cmp(&b.timestamp)) {
                timestamp.clone_from(&newest.timestamp);
                window.clone_from(&newest.window);
            }
            for sample in samples {
                let sum = sums.entry(sample.namespace).or_default();
                for container in &sample.containers {
                    sum.usage.add(Amount {
                        cpu: capacity::millicores(&container.cpu).unwrap_or(0),
                        memory: capacity::bytes(&container.memory).unwrap_or(0),
                    });
                }
                sum.sampled += 1;
            }
            MetricsState::Available
        }
        Ok(Reading::NotInstalled) => MetricsState::NotInstalled,
        Ok(Reading::Refused(refusal)) => {
            unreadable.push(refusal);
            MetricsState::Unreadable
        }
        Err(_) => MetricsState::Unavailable,
    };
    (state, timestamp, window)
}

/// Each project's name, with its display name.
fn projects_of(list: Vec<Namespace>) -> BTreeMap<String, Option<String>> {
    list.into_iter()
        .filter(|n| {
            n.metadata
                .labels
                .as_ref()
                .is_some_and(|l| l.contains_key(LABEL_PROJECT))
        })
        .filter_map(|n| {
            let details = ProjectDetails::from_annotations(n.metadata.annotations.as_ref());
            Some((n.metadata.name?, details.display_name))
        })
        .collect()
}

/// What each namespace's pods reserve, may use at most and use now.
///
/// Requests and limits are summed from every unfinished pod, as for the nodes; a pod without a
/// limit adds nothing and is counted in `unlimited`. Usage comes from metrics-server and is
/// approximate. `403` when pods cannot be listed in all namespaces. Without metrics-server, or
/// without the right to read it, `metrics` says so and usage is absent. Without `list` on
/// namespaces, `adopted` is absent.
#[utoipa::path(get, path = "/api/v1/cluster/usage", tag = "cluster", operation_id = "getClusterUsage",
    responses((status = 200, body = ClusterUsage), (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(Person(cluster): Person) -> Result<Json<ClusterUsage>, ApiError> {
    let (pods, metrics, namespaces) = futures_util::join!(
        cluster.cluster_pods(WIDE_LIMIT),
        modules::read(
            cluster.as_ref(),
            &zeabur_modules::metrics_server::POD_METRICS
        ),
        cluster.namespaces()
    );
    let pods = pods.map_err(|e| {
        refused(
            e,
            "the cluster's pods",
            "`list` on `pods` (all namespaces)",
            list_read("", "pods", None),
        )
    })?;
    let mut unreadable = Vec::new();
    // A project's name, and its display name.
    let projects: Option<BTreeMap<String, Option<String>>> =
        optional(namespaces, "namespaces", &mut unreadable)
            .map_err(|e| ApiError::from(e.into_problem("the cluster's namespaces")))?
            .map(projects_of);

    let mut sums: BTreeMap<String, Sum> = BTreeMap::new();
    for pod in &pods.pods {
        let (requests, limits) = capacity::pod_amounts(pod);
        let sum = sums.entry(pod.namespace.clone()).or_default();
        sum.pods += 1;
        sum.requests.add(requests);
        sum.limits.add(limits);
        sum.unlimited.cpu += u64::from(limits.cpu == 0);
        sum.unlimited.memory += u64::from(limits.memory == 0);
    }

    let (state, timestamp, window) = add_usage(metrics, &mut sums, &mut unreadable);
    let measured = state == MetricsState::Available;

    let mut totals = Sum::default();
    let items: Vec<NamespaceUsage> = sums
        .into_iter()
        .map(|(namespace, sum)| {
            totals.requests.add(sum.requests);
            totals.limits.add(sum.limits);
            totals.usage.add(sum.usage);
            totals.sampled += sum.sampled;
            NamespaceUsage {
                origin: Origin::complete("v1", "Namespace", None, &namespace),
                adopted: projects.as_ref().map(|p| p.contains_key(&namespace)),
                system: is_system_namespace(&namespace),
                display_name: projects
                    .as_ref()
                    .and_then(|p| p.get(&namespace).cloned().flatten()),
                pods: sum.pods,
                requests: resources(sum.requests, None),
                limits: resources(sum.limits, None),
                unlimited: sum.unlimited,
                usage: measured.then(|| resources(sum.usage, Some(sum.sampled))),
                namespace,
            }
        })
        .collect();
    Ok(Json(ClusterUsage {
        id: "cluster/usage".to_owned(),
        items,
        totals: UsageTotals {
            requests: resources(totals.requests, None),
            limits: resources(totals.limits, None),
            usage: measured.then(|| resources(totals.usage, Some(totals.sampled))),
        },
        pod_scan: if pods.truncated {
            PodScan::Partial
        } else {
            PodScan::Complete
        },
        pods_scanned: pods.pods.len(),
        metrics: state,
        timestamp,
        window,
        unreadable,
    }))
}
