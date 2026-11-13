//! How far a service's rollout has come: the new release's pods against the old ones, and whether
//! it is done, still going, or stuck and why. Clients poll it, as they poll everything else.
//!
//! # Done
//!
//! A rollout is done once `zeabur_graph::rolled_out` says so: kstatus for a Deployment,
//! StatefulSet (rolling or partitioned) or DaemonSet, and every replica updated and ready for an
//! `OnDelete` StatefulSet. A Deployment is done only when no old replica is left.
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
//! [k8s-deployment-failed]: https://kubernetes.io/docs/concepts/workloads/controllers/deployment/#failed-deployment

use axum::extract::State;
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_graph::Workload;
use zeabur_kube::k8s_openapi::api::apps::v1::DeploymentCondition;
use zeabur_kube::k8s_openapi::api::core::v1::Pod;
use zeabur_model::{Diagnosis, DiagnosisKind, Message, Origin, WorkloadKind, WorkloadRef, id};
use zeabur_plan::{Change, PlanRequest, Release};

use crate::error::ApiError;
use crate::extract::{Json, Path};
use crate::handlers::{AppState, Person};
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
    /// Pod Security keeps a new pod from running.
    PodSecurity,
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

/// The condition of `type_` of a Deployment.
fn deployment_condition<'a>(
    workload: Workload<'a>,
    type_: &str,
) -> Option<&'a DeploymentCondition> {
    let Workload::Deployment(deployment) = workload else {
        return None;
    };
    let conditions = deployment.status.as_ref()?.conditions.as_deref()?;
    conditions.iter().find(|condition| condition.type_ == type_)
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
        DiagnosisKind::PodSecurity => Some(StuckReason::PodSecurity),
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
        StuckReason::PodSecurity => "podSecurity",
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

/// The progress of the rollout of `workload`, with its `releases` (newest first), the names of
/// the current release's pods and the service's diagnoses.
pub(crate) fn progress(
    workload: Workload<'_>,
    releases: &[Release],
    new_pods: &[String],
    diagnoses: &[Diagnosis],
) -> (
    RolloutStatus,
    Option<StuckReason>,
    Vec<Diagnosis>,
    Vec<Origin>,
) {
    let kind = workload.kind().as_str();
    let on = |path: &str| Origin::field(kind, workload.name(), path.to_owned());
    let mine: Vec<Diagnosis> = diagnoses
        .iter()
        .filter(|d| d.pods.is_empty() || d.pods.iter().any(|p| new_pods.contains(p)))
        .cloned()
        .collect();
    let mut evidence = vec![on("status")];
    if let Workload::Deployment(deployment) = workload
        && deployment.spec.as_ref().and_then(|spec| spec.paused) == Some(true)
    {
        return (RolloutStatus::Paused, None, mine, vec![on("spec.paused")]);
    }
    if zeabur_graph::rolled_out(workload) {
        return (RolloutStatus::Done, None, Vec::new(), evidence);
    }
    let deadline = deployment_condition(workload, "Progressing")
        .filter(|c| c.reason.as_deref() == Some("ProgressDeadlineExceeded"));
    let failure = deployment_condition(workload, "ReplicaFailure").filter(|c| c.status == "True");
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
        None if zeabur_graph::on_delete(workload)
            && releases.first().is_some_and(|r| r.pods.total == 0) =>
        {
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
    operation_id = "getServiceRollout",
    params(("project" = String, Path),
           ("kind" = String, Path, description = "deployment, statefulset or daemonset; any case"),
           ("name" = String, Path)),
    responses((status = 200, body = RolloutProgress), (status = 400, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn rollout(
    State(state): State<AppState>,
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
    let read = crate::projects::read(cluster, &project, (state.clock)()).await?;
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
    let me = workload.reference();
    let current = releases.iter().find(|r| r.current);
    let new_pods = current.map_or_else(Vec::new, |r| {
        pods_of_release(workload_kind, r, &read.snapshot.pods)
    });
    let (status, reason, diagnoses, evidence) =
        progress(workload, &releases, &new_pods, &service.diagnoses);
    let older = releases.iter().filter(|r| !r.current);
    let desired = zeabur_graph::replicas(workload).map_or(0, |replicas| replicas.desired);
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
    use serde_json::{Value, json};
    use zeabur_graph::Snapshot;
    use zeabur_kube::k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
    use zeabur_model::StatusWord;
    use zeabur_plan::ReleasePods;

    use super::*;

    fn typed<T: serde::de::DeserializeOwned>(object: Value) -> T {
        serde_json::from_value(object).unwrap()
    }

    /// The status light's word and the rollout panel's status, with no pod and no release.
    fn both(workload: Workload<'_>) -> (StatusWord, RolloutStatus) {
        let summary = zeabur_graph::summarise(&Snapshot::default(), workload);
        let light = zeabur_graph::status(&summary, &[], None).word;
        (light, progress(workload, &[], &[], &[]).0)
    }

    const UPDATING: (StatusWord, RolloutStatus) =
        (StatusWord::RollingOut, RolloutStatus::Progressing);
    const DONE: (StatusWord, RolloutStatus) = (StatusWord::Ready, RolloutStatus::Done);

    /// A StatefulSet of three replicas at observed generation 2, updated with `strategy`, and
    /// its `status`.
    fn stateful_set(strategy: &Value, status: &Value) -> StatefulSet {
        let mut status = status.clone();
        status["replicas"] = json!(3);
        status["observedGeneration"] = json!(2);
        typed(json!({"metadata": {"name": "db", "generation": 2},
            "spec": {"replicas": 3, "serviceName": "db", "selector": {}, "template": {},
                "updateStrategy": strategy},
            "status": status}))
    }

    #[test]
    fn a_stateful_set_image_change_shows_updating_until_the_update_revision_rolled_out() {
        let rolling = json!({"type": "RollingUpdate", "rollingUpdate": {"partition": 0}});
        let stage = |updated: i32, current: i32, ready: i32, current_revision: &str| {
            stateful_set(
                &rolling,
                &json!({"updatedReplicas": updated, "currentReplicas": current,
                    "readyReplicas": ready, "currentRevision": current_revision,
                    "updateRevision": "db-new"}),
            )
        };
        let cases = [
            ("old pods all ready", stage(0, 3, 3, "db-old"), UPDATING),
            ("one pod replaced", stage(1, 2, 3, "db-old"), UPDATING),
            ("a pod restarting", stage(2, 1, 2, "db-old"), UPDATING),
            ("rolled out", stage(3, 3, 3, "db-new"), DONE),
        ];
        for (name, object, expected) in &cases {
            assert_eq!(both(Workload::StatefulSet(object)), *expected, "{name}");
        }
        let mut unobserved = stage(3, 3, 3, "db-new");
        unobserved.status.as_mut().unwrap().observed_generation = Some(1);
        assert_eq!(both(Workload::StatefulSet(&unobserved)), UPDATING);
        let mut without_partition = stage(0, 3, 3, "db-old");
        without_partition.spec.as_mut().unwrap().update_strategy =
            typed(json!({"type": "RollingUpdate"}));
        assert_eq!(both(Workload::StatefulSet(&without_partition)), UPDATING);
    }

    #[test]
    fn an_on_delete_stateful_set_waits_for_deletion_and_is_done_once_every_pod_is_new() {
        let on_delete = json!({"type": "OnDelete"});
        // The controller keeps `currentRevision` for `OnDelete`, also after every pod is new.
        let object = |updated: i32, ready: i32| {
            stateful_set(
                &on_delete,
                &json!({"updatedReplicas": updated, "currentReplicas": 3 - updated,
                    "readyReplicas": ready, "currentRevision": "db-old",
                    "updateRevision": "db-new"}),
            )
        };
        let release = |pods: i32| Release {
            revision: 2,
            name: "db-new".to_owned(),
            images: Vec::new(),
            created: None,
            change_cause: None,
            current: true,
            pods: ReleasePods {
                ready: pods,
                total: pods,
            },
            template: Value::Null,
            template_yaml: String::new(),
        };
        let panel = |object: &StatefulSet, pods: i32| {
            progress(Workload::StatefulSet(object), &[release(pods)], &[], &[]).0
        };
        let waiting = object(0, 3);
        assert_eq!(panel(&waiting, 0), RolloutStatus::WaitsForDeletion);
        assert_eq!(both(Workload::StatefulSet(&waiting)).0, StatusWord::Ready);
        let recreating = object(1, 2);
        assert_eq!(panel(&recreating, 1), RolloutStatus::Progressing);
        assert_eq!(
            both(Workload::StatefulSet(&recreating)).0,
            StatusWord::RollingOut
        );
        assert_eq!(both(Workload::StatefulSet(&object(3, 3))), DONE);
    }

    #[test]
    fn a_partitioned_stateful_set_is_done_once_the_pods_above_the_partition_are_new() {
        let partition = json!({"type": "RollingUpdate", "rollingUpdate": {"partition": 2}});
        let object = |updated: i32| {
            stateful_set(
                &partition,
                &json!({"updatedReplicas": updated, "currentReplicas": 3 - updated,
                    "readyReplicas": 3, "currentRevision": "db-old",
                    "updateRevision": "db-new"}),
            )
        };
        assert_eq!(both(Workload::StatefulSet(&object(0))), UPDATING);
        assert_eq!(both(Workload::StatefulSet(&object(1))), DONE);
    }

    #[test]
    fn a_daemon_set_with_a_new_template_shows_updating_until_every_node_runs_it() {
        let object = |updated: i32| -> DaemonSet {
            typed(json!({"metadata": {"name": "agent", "generation": 2},
                "spec": {"selector": {}, "template": {}},
                "status": {"observedGeneration": 2, "desiredNumberScheduled": 3,
                    "currentNumberScheduled": 3, "updatedNumberScheduled": updated,
                    "numberAvailable": 3, "numberReady": 3, "numberMisscheduled": 0}}))
        };
        assert_eq!(both(Workload::DaemonSet(&object(0))), UPDATING);
        assert_eq!(both(Workload::DaemonSet(&object(2))), UPDATING);
        assert_eq!(both(Workload::DaemonSet(&object(3))), DONE);
    }

    fn deployment(status: &Value) -> Deployment {
        typed(json!({"metadata": {"name": "web", "generation": 3},
            "spec": {"replicas": 2, "progressDeadlineSeconds": 600, "selector": {},
                "template": {}},
            "status": status}))
    }

    #[test]
    fn a_deployment_whose_old_replica_set_is_still_available_is_updating() {
        let surging = deployment(&json!({"observedGeneration": 3, "replicas": 3,
            "updatedReplicas": 2, "readyReplicas": 3, "availableReplicas": 3,
            "conditions": [{"type": "Available", "status": "True"},
                {"type": "Progressing", "status": "True", "reason": "ReplicaSetUpdated"}]}));
        assert_eq!(both(Workload::Deployment(&surging)), UPDATING);
        let finished = deployment(&json!({"observedGeneration": 3, "replicas": 2,
            "updatedReplicas": 2, "readyReplicas": 2, "availableReplicas": 2,
            "conditions": [{"type": "Available", "status": "True"},
                {"type": "Progressing", "status": "True", "reason": "NewReplicaSetAvailable"}]}));
        assert_eq!(both(Workload::Deployment(&finished)), DONE);
        let mut unobserved = finished;
        unobserved.metadata.generation = Some(4);
        assert_eq!(both(Workload::Deployment(&unobserved)), UPDATING);
    }

    #[test]
    fn a_crash_loop_of_a_new_pod_makes_it_stuck_and_a_deadline_says_so_without_one() {
        let object = deployment(&json!({"observedGeneration": 3, "replicas": 3,
            "updatedReplicas": 2, "readyReplicas": 2, "availableReplicas": 2,
            "conditions": [{"type": "Progressing", "status": "False",
                "reason": "ProgressDeadlineExceeded"}]}));
        let web = Workload::Deployment(&object);
        let diagnosis: Diagnosis = serde_json::from_value(json!({
            "kind": "crashLoop", "container": "app", "pods": ["web-2-abcde"],
            "cause": {"key": "diagnosis.crashLoop.cause.exitCode", "params": {}},
            "why": {"key": "w", "params": {}}, "fix": {"key": "f", "params": {}},
            "evidence": []}))
        .unwrap();
        let (status, reason, mine, _) = progress(
            web,
            &[],
            &["web-2-abcde".to_owned()],
            std::slice::from_ref(&diagnosis),
        );
        assert_eq!(status, RolloutStatus::Stuck);
        assert_eq!(reason, Some(StuckReason::CrashLoop));
        assert_eq!(mine, vec![diagnosis.clone()]);
        let (_, reason, mine, _) = progress(web, &[], &[], &[]);
        assert_eq!(reason, Some(StuckReason::DeadlineExceeded));
        assert_eq!(mine, Vec::<Diagnosis>::new());
        let (_, reason, mine, _) = progress(web, &[], &["web-3-zzzzz".to_owned()], &[diagnosis]);
        assert_eq!(reason, Some(StuckReason::DeadlineExceeded));
        assert_eq!(mine, Vec::<Diagnosis>::new());
    }
}
