//! How far a service's rollout has come: the new release's pods against the old ones, and whether
//! it is done, still going, or stuck and why. Clients poll it, as they poll everything else.
//!
//! # Done
//!
//! As `kubectl rollout status` decides it [kubectl-rollout-status]:
//!
//! - a Deployment is done once its controller has seen the latest spec
//!   (`status.observedGeneration`), every replica is updated, no old replica is left and every
//!   updated one is available;
//! - a StatefulSet once it has seen the spec, `status.updatedReplicas` reaches the replicas the
//!   update covers (those at or above `rollingUpdate.partition`), every replica is ready and
//!   `currentRevision` equals `updateRevision`; with `OnDelete` its pods change only when deleted,
//!   so it is never done by itself;
//! - a DaemonSet once it has seen the spec and every node's pod is updated and available.
//!
//! # Stuck
//!
//! A Deployment whose `Progressing` condition says `ProgressDeadlineExceeded`, or that reports
//! `ReplicaFailure` (a pod the quota refused) [k8s-deployment-failed], is stuck. Any workload is
//! stuck once a new pod is in a state it does not leave alone: a crash loop, out of memory, an
//! image that cannot be pulled, no node with room, a quota, a missing ConfigMap or Secret, a run
//! that failed. The reason is the service's diagnosis of those pods. A failing readiness probe is
//! reported but does not make it stuck: a pod that starts slowly passes in the end.
//!
//! A stuck rollout carries the plan that returns to the release before it.
//!
//! [kubectl-rollout-status]: https://github.com/kubernetes/kubectl/blob/master/pkg/polymorphichelpers/rollout_status.go
//! [k8s-deployment-failed]: https://kubernetes.io/docs/concepts/workloads/controllers/deployment/#failed-deployment

use axum::Json;
use axum::extract::Path;
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::api::core::v1::Pod;
use zeabur_model::{Diagnosis, DiagnosisKind, Message, Origin, WorkloadKind, WorkloadRef, id};
use zeabur_plan::{Change, PlanRequest, Release};

use crate::error::ApiError;
use crate::handlers::Person;
use crate::projects::not_found;
use crate::timeline::history;
use crate::views::UnreadableView;

/// Where a rollout stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum RolloutStatus {
    /// New pods are being started or old ones stopped.
    Progressing,
    /// Every pod runs the current release and is available.
    Done,
    /// It will not finish by itself; see `reason`.
    Stuck,
    /// The Deployment is paused (`spec.paused`).
    Paused,
    /// A StatefulSet or DaemonSet with `OnDelete`: its pods change only when they are deleted.
    WaitsForDeletion,
}

/// Why a rollout is stuck.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum StuckReason {
    /// A new pod's container keeps exiting.
    CrashLoop,
    /// A new pod's container was killed for memory.
    OutOfMemory,
    /// A new pod's image cannot be pulled.
    ImagePull,
    /// No node has room for a new pod, or none matches its placement.
    Unschedulable,
    /// A `ResourceQuota` refuses the new pods.
    Quota,
    /// A ConfigMap or Secret a new pod needs is missing.
    ConfigMissing,
    /// A new pod's run failed.
    RunFailed,
    /// The Deployment's `progressDeadlineSeconds` passed.
    DeadlineExceeded,
    /// The Deployment's controller could not create a pod (`ReplicaFailure`).
    ReplicaFailure,
}

/// How far a service's rollout has come.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RolloutProgress {
    /// `projects/<project>/services/<kind>/<name>/rollout`.
    pub id: String,
    /// The workload.
    pub workload: WorkloadRef,
    /// Where it stands.
    pub status: RolloutStatus,
    /// `rollout.<progressing|done|paused|waitsForDeletion>` or `rollout.stuck.<reason>`.
    pub message: Message,
    /// Why it is stuck.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<StuckReason>,
    /// The release it rolls out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<i64>,
    /// The pods it should have (nodes, for a DaemonSet).
    pub desired: i32,
    /// Pods of the new release.
    pub updated: i32,
    /// Of those, ready.
    pub updated_ready: i32,
    /// Pods of earlier releases still there.
    pub old: i32,
    /// Of those, ready.
    pub old_ready: i32,
    /// The diagnoses of the new pods.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnoses: Vec<Diagnosis>,
    /// The status fields it rests on.
    pub evidence: Vec<Origin>,
    /// The changes that return to the release before, to send to `plan`; while it is not done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback: Option<PlanRequest>,
    /// Reads the person was refused.
    pub unreadable: Vec<UnreadableView>,
}

fn int(object: &Value, pointer: &str) -> Option<i64> {
    object.pointer(pointer).and_then(Value::as_i64)
}

fn count(object: &Value, pointer: &str) -> i32 {
    int(object, pointer)
        .and_then(|n| i32::try_from(n).ok())
        .unwrap_or(0)
}

fn condition<'a>(object: &'a Value, kind: &str) -> Option<&'a Value> {
    object
        .pointer("/status/conditions")
        .and_then(Value::as_array)?
        .iter()
        .find(|c| c.get("type").and_then(Value::as_str) == Some(kind))
}

/// Whether the workload's controller has seen its latest spec.
fn observed(object: &Value) -> bool {
    int(object, "/status/observedGeneration") >= int(object, "/metadata/generation")
}

/// Whether `object` is done, by the rules in the module documentation.
fn done(kind: WorkloadKind, object: &Value) -> bool {
    if !observed(object) {
        return false;
    }
    match kind {
        WorkloadKind::Deployment => {
            let desired = int(object, "/spec/replicas").unwrap_or(1);
            let updated = int(object, "/status/updatedReplicas").unwrap_or(0);
            updated >= desired
                && int(object, "/status/replicas").unwrap_or(0) <= updated
                && int(object, "/status/availableReplicas").unwrap_or(0) >= updated
        }
        WorkloadKind::StatefulSet => {
            let desired = int(object, "/spec/replicas").unwrap_or(1);
            let partition =
                int(object, "/spec/updateStrategy/rollingUpdate/partition").unwrap_or(0);
            let covered = (desired - partition).max(0);
            let revisions_match = object.pointer("/status/currentRevision")
                == object.pointer("/status/updateRevision");
            int(object, "/status/updatedReplicas").unwrap_or(0) >= covered
                && int(object, "/status/readyReplicas").unwrap_or(0) >= desired
                && (partition > 0 || revisions_match)
        }
        WorkloadKind::DaemonSet => {
            let desired = int(object, "/status/desiredNumberScheduled").unwrap_or(0);
            int(object, "/status/updatedNumberScheduled").unwrap_or(0) >= desired
                && int(object, "/status/numberAvailable").unwrap_or(0) >= desired
        }
        WorkloadKind::Job | WorkloadKind::CronJob => true,
    }
}

const fn stuck_by(kind: DiagnosisKind) -> Option<StuckReason> {
    match kind {
        DiagnosisKind::CrashLoop => Some(StuckReason::CrashLoop),
        DiagnosisKind::OutOfMemory => Some(StuckReason::OutOfMemory),
        DiagnosisKind::ImagePull => Some(StuckReason::ImagePull),
        DiagnosisKind::Unschedulable => Some(StuckReason::Unschedulable),
        DiagnosisKind::QuotaBlocked => Some(StuckReason::Quota),
        DiagnosisKind::ConfigMissing => Some(StuckReason::ConfigMissing),
        DiagnosisKind::RunFailed => Some(StuckReason::RunFailed),
        DiagnosisKind::ProbeFailure | DiagnosisKind::Unreachable | DiagnosisKind::Unexplained => {
            None
        }
    }
}

const fn reason_key(reason: StuckReason) -> &'static str {
    match reason {
        StuckReason::CrashLoop => "crashLoop",
        StuckReason::OutOfMemory => "outOfMemory",
        StuckReason::ImagePull => "imagePull",
        StuckReason::Unschedulable => "unschedulable",
        StuckReason::Quota => "quota",
        StuckReason::ConfigMissing => "configMissing",
        StuckReason::RunFailed => "runFailed",
        StuckReason::DeadlineExceeded => "deadlineExceeded",
        StuckReason::ReplicaFailure => "replicaFailure",
    }
}

/// The pods of `release`, the current one, by name.
fn pods_of_release(kind: WorkloadKind, release: &Release, pods: &[Pod]) -> Vec<String> {
    pods.iter()
        .filter(|pod| match kind {
            WorkloadKind::Deployment => pod
                .metadata
                .owner_references
                .iter()
                .flatten()
                .any(|o| o.kind == "ReplicaSet" && o.name == release.name),
            WorkloadKind::StatefulSet
            | WorkloadKind::DaemonSet
            | WorkloadKind::Job
            | WorkloadKind::CronJob => pod
                .metadata
                .labels
                .as_ref()
                .and_then(|l| l.get("controller-revision-hash"))
                .is_some_and(|hash| {
                    *hash == release.name
                        || release
                            .name
                            .strip_suffix(hash.as_str())
                            .is_some_and(|rest| rest.ends_with('-'))
                }),
        })
        .filter_map(|pod| pod.metadata.name.clone())
        .collect()
}

/// The progress of the rollout of `workload` (the object as JSON), with its `releases` (newest
/// first), the names of the current release's pods and the service's diagnoses.
pub(crate) fn progress(
    workload: &WorkloadRef,
    object: &Value,
    releases: &[Release],
    new_pods: &[String],
    diagnoses: &[Diagnosis],
) -> (
    RolloutStatus,
    Option<StuckReason>,
    Vec<Diagnosis>,
    Vec<Origin>,
) {
    let kind = workload.kind.as_str();
    let on = |path: &str| Origin::field(kind, &workload.name, path.to_owned());
    let mine: Vec<Diagnosis> = diagnoses
        .iter()
        .filter(|d| d.pods.is_empty() || d.pods.iter().any(|p| new_pods.contains(p)))
        .cloned()
        .collect();
    let mut evidence = vec![on("status")];
    if workload.kind == WorkloadKind::Deployment
        && object.pointer("/spec/paused").and_then(Value::as_bool) == Some(true)
    {
        return (RolloutStatus::Paused, None, mine, vec![on("spec.paused")]);
    }
    let on_delete = object
        .pointer("/spec/updateStrategy/type")
        .and_then(Value::as_str)
        == Some("OnDelete");
    if done(workload.kind, object) {
        return (RolloutStatus::Done, None, Vec::new(), evidence);
    }
    let deadline = condition(object, "Progressing")
        .filter(|c| c.get("reason").and_then(Value::as_str) == Some("ProgressDeadlineExceeded"));
    let failure = condition(object, "ReplicaFailure")
        .filter(|c| c.get("status").and_then(Value::as_str) == Some("True"));
    let from_pods = mine.iter().find_map(|d| stuck_by(d.kind));
    let reason = from_pods
        .or_else(|| failure.map(|_| StuckReason::ReplicaFailure))
        .or_else(|| deadline.map(|_| StuckReason::DeadlineExceeded));
    if failure.is_some() {
        evidence.push(on("status.conditions[type=ReplicaFailure]"));
    }
    if deadline.is_some() {
        evidence.push(on("status.conditions[type=Progressing]"));
    }
    let status = match reason {
        Some(_) => RolloutStatus::Stuck,
        None if on_delete && releases.first().is_some_and(|r| r.pods.total == 0) => {
            RolloutStatus::WaitsForDeletion
        }
        None => RolloutStatus::Progressing,
    };
    (status, reason, mine, evidence)
}

/// How far a service's rollout has come: pods of the new release and of earlier ones, ready or
/// not, and whether it is done, progressing, or stuck (with the reason and the plan that rolls
/// back). Poll it while `status` is `progressing`.
#[utoipa::path(get, path = "/api/v1/projects/{project}/services/{kind}/{name}/rollout", tag = "services",
    operation_id = "serviceRollout",
    params(("project" = String, Path),
           ("kind" = String, Path, description = "deployment, statefulset or daemonset; any case"),
           ("name" = String, Path)),
    responses((status = 200, body = RolloutProgress), (status = 400, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn rollout(
    Person(cluster): Person,
    Path((project, kind, name)): Path<(String, String, String)>,
) -> Result<Json<RolloutProgress>, ApiError> {
    let Some(workload_kind) = WorkloadKind::parse(&kind) else {
        return Err(not_found(&project, &kind, &name));
    };
    if matches!(workload_kind, WorkloadKind::Job | WorkloadKind::CronJob) {
        return Err(ApiError::from(Problem::new(
            Kind::BadRequest,
            "A Job or CronJob has no rollout: each run starts new pods.",
        )));
    }
    let cluster = cluster.as_ref();
    let read = crate::projects::read(cluster, &project).await?;
    let workload = zeabur_graph::workloads(&read.snapshot)
        .into_iter()
        .find(|w| w.kind() == workload_kind && w.name() == name)
        .ok_or_else(|| not_found(&project, workload_kind.as_str(), &name))?;
    let service = read
        .document
        .project
        .services
        .iter()
        .find(|s| s.workload.kind == workload_kind && s.workload.name == name)
        .ok_or_else(|| not_found(&project, workload_kind.as_str(), &name))?;
    let mut unreadable = read.document.unreadable.clone();
    let releases = history(cluster, &project, &mut unreadable)
        .await?
        .of(workload, &read.snapshot.pods);
    let object = match workload {
        zeabur_graph::Workload::Deployment(o) => serde_json::to_value(o),
        zeabur_graph::Workload::StatefulSet(o) => serde_json::to_value(o),
        zeabur_graph::Workload::DaemonSet(o) => serde_json::to_value(o),
        zeabur_graph::Workload::Job(o) => serde_json::to_value(o),
        zeabur_graph::Workload::CronJob(o) => serde_json::to_value(o),
    }
    .unwrap_or_default();
    let me = workload.reference();
    let current = releases.iter().find(|r| r.current);
    let new_pods = current.map_or_else(Vec::new, |r| {
        pods_of_release(workload_kind, r, &read.snapshot.pods)
    });
    let (status, reason, diagnoses, evidence) =
        progress(&me, &object, &releases, &new_pods, &service.diagnoses);
    let older = releases.iter().filter(|r| !r.current);
    let desired = if workload_kind == WorkloadKind::DaemonSet {
        count(&object, "/status/desiredNumberScheduled")
    } else {
        count(&object, "/spec/replicas")
    };
    let rollback = releases
        .iter()
        .find(|r| !r.current && current.is_some_and(|c| r.revision < c.revision))
        .filter(|_| matches!(status, RolloutStatus::Stuck | RolloutStatus::Progressing))
        .map(|before| PlanRequest {
            changes: vec![Change::Rollback {
                workload: me.clone(),
                revision: before.revision,
            }],
        });
    let message = match (status, reason) {
        (RolloutStatus::Stuck, Some(reason)) => {
            Message::new(&format!("rollout.stuck.{}", reason_key(reason)))
        }
        (RolloutStatus::Progressing | RolloutStatus::Stuck, _) => {
            Message::new("rollout.progressing")
        }
        (RolloutStatus::Done, _) => Message::new("rollout.done"),
        (RolloutStatus::Paused, _) => Message::new("rollout.paused"),
        (RolloutStatus::WaitsForDeletion, _) => Message::new("rollout.waitsForDeletion"),
    }
    .with("name", name.clone())
    .with("desired", desired.to_string())
    .with("updated", current.map_or(0, |r| r.pods.total).to_string());
    Ok(Json(RolloutProgress {
        id: format!("{}/rollout", id::service(&project, workload_kind, &name)),
        workload: me,
        status,
        message,
        reason,
        revision: current.map(|r| r.revision),
        desired,
        updated: current.map_or(0, |r| r.pods.total),
        updated_ready: current.map_or(0, |r| r.pods.ready),
        old: older.clone().map(|r| r.pods.total).sum(),
        old_ready: older.map(|r| r.pods.ready).sum(),
        diagnoses,
        evidence,
        rollback,
        unreadable,
    }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn web() -> WorkloadRef {
        WorkloadRef {
            kind: WorkloadKind::Deployment,
            name: "web".to_owned(),
        }
    }

    #[test]
    fn a_deployment_is_done_as_kubectl_says() {
        let object = |updated: i64, replicas: i64, available: i64| {
            json!({"metadata": {"generation": 3}, "spec": {"replicas": 2},
                "status": {"observedGeneration": 3, "updatedReplicas": updated,
                    "replicas": replicas, "availableReplicas": available}})
        };
        assert!(done(WorkloadKind::Deployment, &object(2, 2, 2)));
        assert!(!done(WorkloadKind::Deployment, &object(2, 3, 2)));
        assert!(!done(WorkloadKind::Deployment, &object(1, 2, 2)));
        let unseen = json!({"metadata": {"generation": 4}, "spec": {"replicas": 1},
            "status": {"observedGeneration": 3, "updatedReplicas": 1, "replicas": 1,
                "availableReplicas": 1}});
        assert!(!done(WorkloadKind::Deployment, &unseen));
    }

    #[test]
    fn a_crash_loop_of_a_new_pod_makes_it_stuck_and_a_deadline_says_so_without_one() {
        let object = json!({"metadata": {"generation": 2}, "spec": {"replicas": 1},
            "status": {"observedGeneration": 2, "updatedReplicas": 1, "replicas": 2,
                "availableReplicas": 1,
                "conditions": [{"type": "Progressing", "status": "False",
                    "reason": "ProgressDeadlineExceeded"}]}});
        let diagnosis: Diagnosis = serde_json::from_value(json!({
            "kind": "crashLoop", "container": "app", "pods": ["web-2-abcde"],
            "cause": {"key": "diagnosis.crashLoop.cause.exitCode", "params": {}},
            "why": {"key": "w", "params": {}}, "fix": {"key": "f", "params": {}},
            "evidence": []}))
        .unwrap();
        let (status, reason, mine, _) = progress(
            &web(),
            &object,
            &[],
            &["web-2-abcde".to_owned()],
            std::slice::from_ref(&diagnosis),
        );
        assert_eq!(status, RolloutStatus::Stuck);
        assert_eq!(reason, Some(StuckReason::CrashLoop));
        assert_eq!(mine.len(), 1);
        let (_, reason, mine, _) = progress(&web(), &object, &[], &[], &[]);
        assert_eq!(reason, Some(StuckReason::DeadlineExceeded));
        assert_eq!(mine, Vec::<Diagnosis>::new());
        let (_, reason, mine, _) = progress(
            &web(),
            &object,
            &[],
            &["web-3-zzzzz".to_owned()],
            &[diagnosis],
        );
        assert_eq!(reason, Some(StuckReason::DeadlineExceeded));
        assert_eq!(mine, Vec::<Diagnosis>::new());
    }
}
