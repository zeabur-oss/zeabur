//! Which pods belong to a workload.
//!
//! StatefulSets, DaemonSets and Jobs own their pods directly (`ownerReferences`); a CronJob owns
//! Jobs, which own pods. A Deployment owns ReplicaSets, which are not part of a snapshot, so its
//! pods are the ones its `spec.selector` matches [k8s-labels], the same rule its ReplicaSets
//! follow.
//!
//! [k8s-labels]: https://kubernetes.io/docs/concepts/overview/working-with-objects/labels/#label-selectors

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{ContainerState, ContainerStateTerminated, ContainerStatus, Pod};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use k8s_openapi::jiff::Timestamp;
use zeabur_adapters::selector;
use zeabur_model::{
    ContainerStatusSummary, ContainerTermination, ContainerWaiting, Health, Origin, PodSummary,
};

use crate::snapshot::{Snapshot, name_of};
use crate::workload::Workload;

/// The pods of `workload`, sorted by name.
#[must_use]
pub fn pods_of(snapshot: &Snapshot, workload: Workload<'_>) -> Vec<PodSummary> {
    let mut pods: Vec<PodSummary> = snapshot
        .pods
        .iter()
        .filter(|pod| belongs(snapshot, workload, pod))
        .map(|pod| summary(&snapshot.namespace, pod, snapshot.now))
        .collect();
    pods.sort_by(|a, b| a.name.cmp(&b.name));
    pods
}

fn owned_by(meta: &ObjectMeta, kind: &str, name: &str) -> bool {
    meta.owner_references
        .iter()
        .flatten()
        .any(|owner| owner.kind == kind && owner.name == name)
}

pub(crate) fn belongs(snapshot: &Snapshot, workload: Workload<'_>, pod: &Pod) -> bool {
    let name = workload.name();
    match workload {
        Workload::StatefulSet(_) => owned_by(&pod.metadata, "StatefulSet", name),
        Workload::DaemonSet(_) => owned_by(&pod.metadata, "DaemonSet", name),
        Workload::Job(_) => owned_by(&pod.metadata, "Job", name),
        Workload::CronJob(_) => snapshot
            .jobs
            .iter()
            .filter(|job| owned_by(&job.metadata, "CronJob", name))
            .any(|job| owned_by(&pod.metadata, "Job", name_of(&job.metadata))),
        Workload::Deployment(deployment) => deployment
            .spec
            .as_ref()
            .is_some_and(|spec| selects(&spec.selector, pod.metadata.labels.as_ref())),
    }
}

/// Whether `selector` matches `labels`. An empty selector matches nothing here: a workload
/// with no selector has no pods of its own (a NetworkPolicy's empty selector, by contrast,
/// selects every pod).
fn selects(selector: &LabelSelector, labels: Option<&BTreeMap<String, String>>) -> bool {
    let empty = BTreeMap::new();
    !selector::is_empty(selector) && selector::matches(selector, labels.unwrap_or(&empty))
}

fn summary(namespace: &str, pod: &Pod, now: Option<Timestamp>) -> PodSummary {
    let name = name_of(&pod.metadata);
    let status = pod.status.as_ref();
    let statuses = status
        .and_then(|s| s.container_statuses.as_deref())
        .unwrap_or_default();
    let mut summary = PodSummary {
        id: zeabur_model::id::pod(namespace, name),
        name: name.to_owned(),
        phase: status.and_then(|s| s.phase.clone()).unwrap_or_default(),
        ready: !statuses.is_empty() && statuses.iter().all(|c| c.ready),
        restarts: statuses.iter().map(|c| c.restart_count).sum(),
        health: Health::Warn,
        containers: pod
            .spec
            .iter()
            .flat_map(|spec| spec.containers.iter().map(|c| c.name.clone()))
            .collect(),
        reason: reason(statuses),
        created: pod
            .metadata
            .creation_timestamp
            .as_ref()
            .map(|time| time.0.to_string()),
        node: pod.spec.as_ref().and_then(|spec| spec.node_name.clone()),
        container_statuses: status
            .and_then(|s| s.init_container_statuses.as_deref())
            .unwrap_or_default()
            .iter()
            .map(|c| container_status(c, true))
            .chain(statuses.iter().map(|c| container_status(c, false)))
            .collect(),
        origin: Origin::object("Pod", name),
    };
    summary.health = crate::health::pod_health(&summary, now);
    summary
}

fn container_status(status: &ContainerStatus, init: bool) -> ContainerStatusSummary {
    let state = status.state.as_ref();
    ContainerStatusSummary {
        name: status.name.clone(),
        init,
        ready: status.ready,
        restarts: status.restart_count,
        waiting: state
            .and_then(|s| s.waiting.as_ref())
            .map(|waiting| ContainerWaiting {
                reason: waiting.reason.clone(),
                message: waiting.message.clone(),
            }),
        terminated: state.and_then(|s| s.terminated.as_ref()).map(termination),
        last_termination: status
            .last_state
            .as_ref()
            .and_then(|s| s.terminated.as_ref())
            .map(termination),
    }
}

fn termination(terminated: &ContainerStateTerminated) -> ContainerTermination {
    ContainerTermination {
        reason: terminated.reason.clone(),
        exit_code: terminated.exit_code,
        started: terminated
            .started_at
            .as_ref()
            .map(|time| time.0.to_string()),
        finished: terminated
            .finished_at
            .as_ref()
            .map(|time| time.0.to_string()),
        message: terminated.message.clone(),
    }
}

/// Why a pod is not running or not ready, the way `kubectl get pods` words its STATUS column:
/// the first container waiting with a reason, else the reason a not-ready container last
/// stopped. `None` for a pod whose containers all run and are ready.
fn reason(statuses: &[ContainerStatus]) -> Option<String> {
    let waiting = statuses.iter().find_map(|status| {
        status
            .state
            .as_ref()
            .and_then(|state| state.waiting.as_ref())
            .and_then(|waiting| waiting.reason.clone())
    });
    waiting.or_else(|| {
        statuses
            .iter()
            .filter(|status| !status.ready)
            .find_map(|status| {
                let terminated = |state: Option<&ContainerState>| {
                    state
                        .and_then(|state| state.terminated.as_ref())
                        .and_then(|terminated| terminated.reason.clone())
                };
                terminated(status.state.as_ref()).or_else(|| terminated(status.last_state.as_ref()))
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selector(value: serde_json::Value) -> LabelSelector {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn an_empty_selector_selects_no_pods() {
        let labels = BTreeMap::from([("app".to_owned(), "web".to_owned())]);
        assert!(!selects(&selector(serde_json::json!({})), Some(&labels)));
        assert!(selects(
            &selector(serde_json::json!({"matchLabels": {"app": "web"}})),
            Some(&labels)
        ));
    }
}
