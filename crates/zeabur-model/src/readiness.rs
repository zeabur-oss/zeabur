//! Whether a workload or a pod has reached the state its spec asks for, as kstatus computes it
//! [kstatus-core].
//!
//! Each function follows the rule of one kind in kstatus's `core.go`, after the generation check
//! of `generic.go` [kstatus-generic]: if `status.observedGeneration` is present and differs from
//! `metadata.generation`, the object is in progress. The deletion check and the `Reconciling`
//! and `Stalled` conditions of `generic.go` are not used: the built-in controllers do not set
//! those conditions. Zeabur's own states (paused, waits for deletion, stuck but still serving)
//! map on top of this result in `zeabur-graph` and `zeabur-api`.
//!
//! # References
//!
//! - [kstatus-core] Kubernetes SIG CLI, `pkg/kstatus/status/core.go`, cli-utils v0.37.2. <https://github.com/kubernetes-sigs/cli-utils/blob/v0.37.2/pkg/kstatus/status/core.go>
//! - [kstatus-generic] Kubernetes SIG CLI, `pkg/kstatus/status/generic.go`, cli-utils v0.37.2. <https://github.com/kubernetes-sigs/cli-utils/blob/v0.37.2/pkg/kstatus/status/generic.go>
//!
//! [kstatus-core]: https://github.com/kubernetes-sigs/cli-utils/blob/v0.37.2/pkg/kstatus/status/core.go
//! [kstatus-generic]: https://github.com/kubernetes-sigs/cli-utils/blob/v0.37.2/pkg/kstatus/status/generic.go

use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
use k8s_openapi::api::batch::v1::Job;
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use k8s_openapi::jiff::{SignedDuration, Timestamp};

/// The kstatus status of an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The object has reached the state its spec asks for.
    Current,
    /// The controller still works towards the spec.
    InProgress,
    /// The object will not reach the spec without a change.
    Failed,
}

/// Why an object has its [`Status`]. The names are the kstatus reasons; the reasons of a
/// `Current` result are Zeabur's names for the kstatus messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// `status.observedGeneration` is not `metadata.generation`.
    LatestGenerationNotObserved,
    /// A DaemonSet has no `metadata.generation`.
    NoGeneration,
    /// A DaemonSet has no `status.observedGeneration`.
    NoObservedGeneration,
    /// Fewer replicas exist than `spec.replicas`.
    LessReplicas,
    /// Fewer replicas are ready than wanted.
    LessReady,
    /// Fewer replicas run the current template than wanted.
    LessUpdated,
    /// Fewer replicas are available than wanted.
    LessAvailable,
    /// Fewer replicas are current than wanted.
    LessCurrent,
    /// More replicas exist than `spec.replicas`.
    ExtraPods,
    /// Fewer replicas at or above the partition run the update revision than wanted.
    PartitionRollout,
    /// `status.currentRevision` is not `status.updateRevision`.
    RevisionMismatch,
    /// The `Progressing` condition does not say `NewReplicaSetAvailable`.
    ReplicaSetNotAvailable,
    /// The `Available` condition is not `True`.
    DeploymentNotAvailable,
    /// A DaemonSet has no `status.desiredNumberScheduled`.
    NoDesiredNumber,
    /// The `Progressing` condition says `ProgressDeadlineExceeded`.
    ProgressDeadlineExceeded,
    /// A pod is not scheduled yet.
    PodNotScheduled,
    /// A pod is unschedulable for longer than [`SCHEDULE_WINDOW`].
    PodUnschedulable,
    /// A pod is `Pending`.
    PodPending,
    /// A pod is `Running` but not ready.
    PodRunningNotReady,
    /// A pod has no phase yet.
    PodNotObserved,
    /// A pod has a phase that kstatus does not know.
    PodPhaseUnknown,
    /// A container of a running pod waits in `CrashLoopBackOff`.
    ContainerCrashLooping,
    /// A Job has no `status.startTime`.
    JobNotStarted,
    /// A Job's `Failed` condition is `True`.
    JobFailed,
    /// Every replica is up to date and ready.
    Available,
    /// A StatefulSet uses `OnDelete`; kstatus does not follow its update.
    OnDelete,
    /// Every replica at or above the partition runs the update revision.
    PartitionRolledOut,
    /// A pod is ready.
    PodReady,
    /// A pod ran to completion, successfully or not.
    PodCompleted,
    /// A Job's `Complete` condition is `True`.
    JobComplete,
    /// A Job has started.
    JobRunning,
}

/// The [`Status`] of an object and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Readiness {
    /// The status.
    pub status: Status,
    /// Why.
    pub reason: Reason,
}

impl Readiness {
    const fn current(reason: Reason) -> Self {
        Self {
            status: Status::Current,
            reason,
        }
    }

    const fn in_progress(reason: Reason) -> Self {
        Self {
            status: Status::InProgress,
            reason,
        }
    }

    const fn failed(reason: Reason) -> Self {
        Self {
            status: Status::Failed,
            reason,
        }
    }

    /// Whether the status is [`Status::Current`].
    #[must_use]
    pub fn is_current(self) -> bool {
        self.status == Status::Current
    }
}

/// How long a pod can be unscheduled before kstatus reports it as unschedulable.
pub const SCHEDULE_WINDOW: SignedDuration = SignedDuration::from_secs(15);

/// The generation check of kstatus `checkGeneration`.
fn generation_unobserved(meta: &ObjectMeta, observed: Option<i64>) -> Option<Readiness> {
    let (Some(generation), Some(observed)) = (meta.generation, observed) else {
        return None;
    };
    (generation != observed).then_some(Readiness::in_progress(Reason::LatestGenerationNotObserved))
}

/// A Deployment, by kstatus `deploymentConditions`.
///
/// A `Progressing` condition with reason `ProgressDeadlineExceeded` fails it. It is current once
/// it has exactly `spec.replicas` replicas, all updated, available and ready, its `Available`
/// condition is `True`, and its `Progressing` condition says `NewReplicaSetAvailable`. Without
/// `spec.progressDeadlineSeconds` the controller sets no `Progressing` condition, so that check
/// does not apply.
#[must_use]
pub fn deployment(deployment: &Deployment) -> Readiness {
    let status = deployment.status.as_ref();
    if let Some(unobserved) = generation_unobserved(
        &deployment.metadata,
        status.and_then(|status| status.observed_generation),
    ) {
        return unobserved;
    }
    let spec = deployment.spec.as_ref();
    let mut progressing = spec
        .and_then(|spec| spec.progress_deadline_seconds)
        .is_none_or(|seconds| seconds == i32::MAX);
    let mut available = false;
    let conditions = status.and_then(|status| status.conditions.as_deref());
    for condition in conditions.unwrap_or_default() {
        let reason = condition.reason.as_deref();
        if condition.type_ == "Progressing" {
            if reason == Some("ProgressDeadlineExceeded") {
                return Readiness::failed(Reason::ProgressDeadlineExceeded);
            }
            if condition.status == "True" && reason == Some("NewReplicaSetAvailable") {
                progressing = true;
            }
        } else if condition.type_ == "Available" && condition.status == "True" {
            available = true;
        }
    }
    let wanted = spec.and_then(|spec| spec.replicas).unwrap_or(1);
    let count = |field: Option<i32>| field.unwrap_or_default();
    let replicas = count(status.and_then(|status| status.replicas));
    let updated = count(status.and_then(|status| status.updated_replicas));
    let ready = count(status.and_then(|status| status.ready_replicas));
    let available_replicas = count(status.and_then(|status| status.available_replicas));
    if wanted > replicas {
        Readiness::in_progress(Reason::LessReplicas)
    } else if wanted > updated {
        Readiness::in_progress(Reason::LessUpdated)
    } else if replicas > wanted {
        Readiness::in_progress(Reason::ExtraPods)
    } else if updated > available_replicas {
        Readiness::in_progress(Reason::LessAvailable)
    } else if wanted > ready {
        Readiness::in_progress(Reason::LessReady)
    } else if !progressing {
        Readiness::in_progress(Reason::ReplicaSetNotAvailable)
    } else if !available {
        Readiness::in_progress(Reason::DeploymentNotAvailable)
    } else {
        Readiness::current(Reason::Available)
    }
}

/// A StatefulSet, by kstatus `stsConditions`.
///
/// With `OnDelete` it is current at once: its pods change only when someone deletes them. With
/// a partition, it is current once the replicas at or above the partition run the update
/// revision. Otherwise it is current once every replica is current and `status.currentRevision`
/// is `status.updateRevision`. In all cases it first needs exactly `spec.replicas` replicas, all
/// ready.
#[must_use]
pub fn stateful_set(stateful_set: &StatefulSet) -> Readiness {
    let status = stateful_set.status.as_ref();
    if let Some(unobserved) = generation_unobserved(
        &stateful_set.metadata,
        status.and_then(|status| status.observed_generation),
    ) {
        return unobserved;
    }
    let spec = stateful_set.spec.as_ref();
    let strategy = spec.and_then(|spec| spec.update_strategy.as_ref());
    if strategy.and_then(|strategy| strategy.type_.as_deref()) == Some("OnDelete") {
        return Readiness::current(Reason::OnDelete);
    }
    let wanted = spec.and_then(|spec| spec.replicas).unwrap_or(1);
    let count = |field: Option<i32>| field.unwrap_or_default();
    let replicas = status.map_or(0, |status| status.replicas);
    let ready = count(status.and_then(|status| status.ready_replicas));
    let current = count(status.and_then(|status| status.current_replicas));
    let updated = count(status.and_then(|status| status.updated_replicas));
    let partition = strategy
        .and_then(|strategy| strategy.rolling_update.as_ref())
        .and_then(|rolling| rolling.partition);
    if wanted > replicas {
        return Readiness::in_progress(Reason::LessReplicas);
    }
    if wanted > ready {
        return Readiness::in_progress(Reason::LessReady);
    }
    if replicas > wanted {
        return Readiness::in_progress(Reason::ExtraPods);
    }
    if let Some(partition) = partition {
        if updated < wanted - partition {
            return Readiness::in_progress(Reason::PartitionRollout);
        }
        return Readiness::current(Reason::PartitionRolledOut);
    }
    if wanted > current {
        return Readiness::in_progress(Reason::LessCurrent);
    }
    let current_revision = status.and_then(|status| status.current_revision.as_deref());
    let update_revision = status.and_then(|status| status.update_revision.as_deref());
    if current_revision.unwrap_or_default() != update_revision.unwrap_or_default() {
        return Readiness::in_progress(Reason::RevisionMismatch);
    }
    Readiness::current(Reason::Available)
}

/// A DaemonSet, by kstatus `daemonsetConditions`.
///
/// It needs `metadata.generation` and `status.observedGeneration`. It is current once every node
/// that should run its pod has one that is current, updated, available and ready.
#[must_use]
pub fn daemon_set(daemon_set: &DaemonSet) -> Readiness {
    let status = daemon_set.status.as_ref();
    let observed = status.and_then(|status| status.observed_generation);
    if daemon_set.metadata.generation.is_none() {
        return Readiness::in_progress(Reason::NoGeneration);
    }
    if observed.is_none() {
        return Readiness::in_progress(Reason::NoObservedGeneration);
    }
    if let Some(unobserved) = generation_unobserved(&daemon_set.metadata, observed) {
        return unobserved;
    }
    let Some(status) = status else {
        return Readiness::in_progress(Reason::NoDesiredNumber);
    };
    let wanted = status.desired_number_scheduled;
    if wanted > status.current_number_scheduled {
        Readiness::in_progress(Reason::LessCurrent)
    } else if wanted > status.updated_number_scheduled.unwrap_or_default() {
        Readiness::in_progress(Reason::LessUpdated)
    } else if wanted > status.number_available.unwrap_or_default() {
        Readiness::in_progress(Reason::LessAvailable)
    } else if wanted > status.number_ready {
        Readiness::in_progress(Reason::LessReady)
    } else {
        Readiness::current(Reason::Available)
    }
}

/// A pod at `now`, by kstatus `podConditions`.
///
/// A pod that ran to completion is current, whether it succeeded or failed. A running pod is
/// current once ready, and failed while a container waits in `CrashLoopBackOff`. A pending pod
/// that is unschedulable fails once it is older than [`SCHEDULE_WINDOW`].
#[must_use]
pub fn pod(pod: &Pod, now: Timestamp) -> Readiness {
    let status = pod.status.as_ref();
    let conditions = status
        .and_then(|status| status.conditions.as_deref())
        .unwrap_or_default();
    match status.and_then(|status| status.phase.as_deref()) {
        Some("Succeeded" | "Failed") => Readiness::current(Reason::PodCompleted),
        Some("Running") => {
            for condition in conditions {
                if condition.type_ == "Ready" && condition.status == "True" {
                    return Readiness::current(Reason::PodReady);
                }
            }
            let containers = status
                .and_then(|status| status.container_statuses.as_deref())
                .unwrap_or_default();
            for container in containers {
                let waiting = container.state.as_ref().and_then(|s| s.waiting.as_ref());
                if waiting.and_then(|w| w.reason.as_deref()) == Some("CrashLoopBackOff") {
                    return Readiness::failed(Reason::ContainerCrashLooping);
                }
            }
            Readiness::in_progress(Reason::PodRunningNotReady)
        }
        Some("Pending") => {
            for condition in conditions {
                if condition.type_ == "PodScheduled"
                    && condition.status == "False"
                    && condition.reason.as_deref() == Some("Unschedulable")
                {
                    let created = pod.metadata.creation_timestamp.as_ref();
                    let recent = created
                        .is_some_and(|created| now.duration_since(created.0) < SCHEDULE_WINDOW);
                    if recent {
                        return Readiness::in_progress(Reason::PodNotScheduled);
                    }
                    return Readiness::failed(Reason::PodUnschedulable);
                }
            }
            Readiness::in_progress(Reason::PodPending)
        }
        None | Some("") => Readiness::in_progress(Reason::PodNotObserved),
        Some(_) => Readiness::in_progress(Reason::PodPhaseUnknown),
    }
}

/// A Job, by kstatus `jobConditions`.
///
/// A `Complete` condition makes it current and a `Failed` condition fails it. Otherwise it is in
/// progress until it starts, and current while it runs.
#[must_use]
pub fn job(job: &Job) -> Readiness {
    let status = job.status.as_ref();
    let conditions = status
        .and_then(|status| status.conditions.as_deref())
        .unwrap_or_default();
    for condition in conditions {
        if condition.status != "True" {
            continue;
        }
        if condition.type_ == "Complete" {
            return Readiness::current(Reason::JobComplete);
        }
        if condition.type_ == "Failed" {
            return Readiness::failed(Reason::JobFailed);
        }
    }
    if status
        .and_then(|status| status.start_time.as_ref())
        .is_none()
    {
        return Readiness::in_progress(Reason::JobNotStarted);
    }
    Readiness::current(Reason::JobRunning)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn typed<T: serde::de::DeserializeOwned>(object: Value) -> T {
        serde_json::from_value(object).unwrap()
    }

    fn of(status: Status, reason: Reason) -> Readiness {
        Readiness { status, reason }
    }

    #[test]
    fn a_pod_is_current_once_ready_or_finished_and_fails_in_a_crash_loop_or_unschedulable() {
        let now: Timestamp = "2026-11-13T12:00:00Z".parse().unwrap();
        let pod = |created: &str, status: Value| -> Pod {
            typed(
                json!({"metadata": {"name": "web-1", "creationTimestamp": created},
                "status": status}),
            )
        };
        let unschedulable = json!({"phase": "Pending", "conditions": [
            {"type": "PodScheduled", "status": "False", "reason": "Unschedulable"}]});
        let cases = vec![
            (
                "no phase",
                pod("2026-11-13T11:00:00Z", json!({})),
                of(Status::InProgress, Reason::PodNotObserved),
            ),
            (
                "pending",
                pod("2026-11-13T11:00:00Z", json!({"phase": "Pending"})),
                of(Status::InProgress, Reason::PodPending),
            ),
            (
                "unschedulable for 10 s",
                pod("2026-11-13T11:59:50Z", unschedulable.clone()),
                of(Status::InProgress, Reason::PodNotScheduled),
            ),
            (
                "unschedulable for 20 s",
                pod("2026-11-13T11:59:40Z", unschedulable),
                of(Status::Failed, Reason::PodUnschedulable),
            ),
            (
                "running and ready",
                pod(
                    "2026-11-13T11:00:00Z",
                    json!({"phase": "Running",
                "conditions": [{"type": "Ready", "status": "True"}]}),
                ),
                of(Status::Current, Reason::PodReady),
            ),
            (
                "running, not ready",
                pod(
                    "2026-11-13T11:00:00Z",
                    json!({"phase": "Running",
                "conditions": [{"type": "Ready", "status": "False"}]}),
                ),
                of(Status::InProgress, Reason::PodRunningNotReady),
            ),
            (
                "crash loop",
                pod(
                    "2026-11-13T11:00:00Z",
                    json!({"phase": "Running",
                "containerStatuses": [{"name": "app", "image": "app", "imageID": "",
                    "ready": false, "restartCount": 4,
                    "state": {"waiting": {"reason": "CrashLoopBackOff"}}}]}),
                ),
                of(Status::Failed, Reason::ContainerCrashLooping),
            ),
            (
                "failed run",
                pod("2026-11-13T11:00:00Z", json!({"phase": "Failed"})),
                of(Status::Current, Reason::PodCompleted),
            ),
            (
                "unknown phase",
                pod("2026-11-13T11:00:00Z", json!({"phase": "Unknown"})),
                of(Status::InProgress, Reason::PodPhaseUnknown),
            ),
        ];
        for (name, object, expected) in &cases {
            assert_eq!(super::pod(object, now), *expected, "{name}");
        }
    }

    #[test]
    fn a_job_is_current_once_started_and_fails_by_its_condition() {
        let job =
            |status: Value| -> Job { typed(json!({"metadata": {"name": "t"}, "status": status})) };
        let cases = [
            (
                "not started",
                job(json!({})),
                of(Status::InProgress, Reason::JobNotStarted),
            ),
            (
                "running",
                job(json!({"startTime": "2026-11-13T11:00:00Z", "active": 1})),
                of(Status::Current, Reason::JobRunning),
            ),
            (
                "complete",
                job(json!({"startTime": "2026-11-13T11:00:00Z",
                "conditions": [{"type": "Complete", "status": "True"}]})),
                of(Status::Current, Reason::JobComplete),
            ),
            (
                "failed",
                job(json!({"startTime": "2026-11-13T11:00:00Z",
                "conditions": [{"type": "Failed", "status": "True"}]})),
                of(Status::Failed, Reason::JobFailed),
            ),
        ];
        for (name, object, expected) in &cases {
            assert_eq!(super::job(object), *expected, "{name}");
        }
    }

    #[test]
    fn a_daemon_set_the_controller_has_not_observed_is_in_progress() {
        let object = |metadata: Value, status: Value| -> DaemonSet {
            typed(
                json!({"metadata": metadata, "spec": {"selector": {}, "template": {}},
                "status": status}),
            )
        };
        let ready = json!({"desiredNumberScheduled": 1, "currentNumberScheduled": 1,
            "updatedNumberScheduled": 1, "numberAvailable": 1, "numberReady": 1,
            "numberMisscheduled": 0});
        let mut observed = ready.clone();
        observed["observedGeneration"] = json!(1);
        let cases = [
            (
                "no generation",
                object(json!({"name": "a"}), observed.clone()),
                Reason::NoGeneration,
            ),
            (
                "no observed generation",
                object(json!({"name": "a", "generation": 1}), ready),
                Reason::NoObservedGeneration,
            ),
            (
                "older generation",
                object(json!({"name": "a", "generation": 2}), observed),
                Reason::LatestGenerationNotObserved,
            ),
        ];
        for (name, object, reason) in &cases {
            assert_eq!(
                daemon_set(object),
                of(Status::InProgress, *reason),
                "{name}"
            );
        }
    }

    #[test]
    fn a_deployment_past_its_deadline_fails_and_one_without_a_deadline_needs_no_progressing() {
        let deployment = |spec: Value, conditions: Value| -> Deployment {
            typed(json!({"metadata": {"name": "web", "generation": 1},
                "spec": spec,
                "status": {"observedGeneration": 1, "replicas": 1, "updatedReplicas": 1,
                    "readyReplicas": 1, "availableReplicas": 1, "conditions": conditions}}))
        };
        let base = json!({"replicas": 1, "selector": {}, "template": {}});
        let mut with_deadline = base.clone();
        with_deadline["progressDeadlineSeconds"] = json!(600);
        let available = json!([{"type": "Available", "status": "True"}]);
        let deadline = json!([{"type": "Progressing", "status": "False",
            "reason": "ProgressDeadlineExceeded"}]);
        let cases = [
            (
                "deadline passed",
                deployment(with_deadline.clone(), deadline),
                of(Status::Failed, Reason::ProgressDeadlineExceeded),
            ),
            (
                "deadline, no Progressing",
                deployment(with_deadline, available.clone()),
                of(Status::InProgress, Reason::ReplicaSetNotAvailable),
            ),
            (
                "no deadline",
                deployment(base.clone(), available),
                of(Status::Current, Reason::Available),
            ),
            (
                "not available",
                deployment(base, json!([])),
                of(Status::InProgress, Reason::DeploymentNotAvailable),
            ),
        ];
        for (name, object, expected) in &cases {
            assert_eq!(super::deployment(object), *expected, "{name}");
        }
    }
}
