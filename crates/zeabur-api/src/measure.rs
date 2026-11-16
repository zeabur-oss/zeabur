//! Changes of one Deployment and how their rollout ends, for the experience program
//! (docs/internal/measurement.md). Every read is made with the person's own credential.

use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;
use zeabur_core::crypto;
use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
use zeabur_kube::{Cluster, ClusterError, ClusterExt as _, ResourceRef};
use zeabur_model::WorkloadKind;
use zeabur_plan::{Change, Plan};
use zeabur_telemetry::{ChangeId, Operation, Rollout, Submitted};

use crate::access::Later;
use crate::error::ApiError;
use crate::handlers::AppState;

/// What happened to one change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fact {
    /// Zeabur sent it to the cluster and answered the person.
    Submitted(Submitted),
    /// Its rollout ended.
    Finished(Rollout),
}

/// One fact about one change, with the user name of the person who made it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The person's user name, which the receiver hashes and never sends.
    pub user: String,
    /// When the change was submitted, in Unix seconds. Both facts of a change are hashed with
    /// this day, so one change never links two days of the same person.
    pub submitted_at: i64,
    /// The change.
    pub id: ChangeId,
    /// What the change does.
    pub operation: Operation,
    /// What happened.
    pub fact: Fact,
}

/// What receives each [`Report`]; it decides whether to send.
pub type Changes = Arc<dyn Fn(Report) + Send + Sync>;

const POLL: Duration = Duration::from_secs(5);
/// `spec.progressDeadlineSeconds` when the Deployment does not say.
const PROGRESS_DEADLINE: Duration = Duration::from_secs(600);
/// How long past the progress deadline the controller gets to set the condition.
const GRACE: Duration = Duration::from_secs(60);

/// A plan that is one change of one Deployment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Measured {
    operation: Operation,
    name: String,
}

/// The operation that `changes` and the `plan` made from them are, if they are exactly one change
/// of one Deployment.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "only these changes are measured"
)]
pub(crate) fn measured(changes: &[Change], plan: &Plan) -> Option<Measured> {
    let ([change], [object]) = (changes, plan.objects.as_slice()) else {
        return None;
    };
    if object.target.api_version != "apps/v1" || object.target.kind != "Deployment" {
        return None;
    }
    let operation = match change {
        Change::Image { workload, .. } if workload.kind == WorkloadKind::Deployment => {
            Operation::DeployImage
        }
        Change::Create { .. } => Operation::DeployImage,
        Change::Scale { workload, .. } if workload.kind == WorkloadKind::Deployment => {
            Operation::ScaleReplicas
        }
        Change::Env { workload, .. } if workload.kind == WorkloadKind::Deployment => {
            Operation::ChangeEnv
        }
        _ => return None,
    };
    Some(Measured {
        operation,
        name: object.target.name.clone(),
    })
}

/// Who made a change, and how to read the cluster as them.
pub(crate) struct Person<'a> {
    /// Their user name.
    pub(crate) user: &'a str,
    /// The connection of their request.
    pub(crate) now: Arc<dyn Cluster>,
    /// Their connection after the request, where the request's one expires.
    pub(crate) later: Option<Later>,
}

/// Reports how the cluster answered the change `measured` of `person`. An accepted write that
/// left `generation` is then read, as the person, until its rollout ends.
pub(crate) fn submitted(
    state: &AppState,
    person: Person<'_>,
    namespace: &str,
    measured: Measured,
    answer: Result<Option<i64>, &ApiError>,
) {
    let (Some(changes), Ok(bytes)) = (&state.changes, crypto::random()) else {
        return;
    };
    let submitted = Report {
        user: person.user.to_owned(),
        submitted_at: (state.clock)().as_second(),
        id: ChangeId::from_random(bytes),
        operation: measured.operation,
        fact: Fact::Submitted(answer.map_or_else(|e| refusal(e.0.status), |_| Submitted::Accepted)),
    };
    let finished = submitted.clone();
    changes(submitted);
    let Ok(Some(generation)) = answer else {
        return;
    };
    let changes = Arc::clone(changes);
    let later = person.later.unwrap_or_else(|| {
        let now = person.now;
        Arc::new(move || {
            let now = Arc::clone(&now);
            Box::pin(async move { Some(now) })
        })
    });
    let namespace = namespace.to_owned();
    tokio::spawn(async move {
        if let Some(result) = follow(&later, &namespace, &measured.name, generation).await {
            changes(Report {
                fact: Fact::Finished(result),
                ..finished
            });
        }
    });
}

/// The class of the HTTP status the person got.
const fn refusal(status: u16) -> Submitted {
    match status {
        403 => Submitted::Forbidden,
        409 => Submitted::Conflict,
        422 => Submitted::Invalid,
        502..=504 => Submitted::Unreachable,
        _ => Submitted::Other,
    }
}

/// Reads the Deployment until the rollout of `generation` ends, as `kubectl rollout status`
/// does. `None` when a newer change replaced it, the person signed out or can no longer read
/// it, or the progress deadline passed by [`GRACE`] with no result.
async fn follow(person: &Later, namespace: &str, name: &str, generation: i64) -> Option<Rollout> {
    let started = Instant::now();
    let mut deadline = PROGRESS_DEADLINE;
    while started.elapsed() < deadline + GRACE {
        tokio::time::sleep(POLL).await;
        let cluster = person().await?;
        let deployment = match read(cluster.as_ref(), namespace, name).await {
            Ok(Some(deployment)) => deployment,
            Ok(None) => return None,
            Err(error) if error.is_refused() => return None,
            Err(_) => continue,
        };
        match seen(&deployment, generation) {
            Seen::Finished(result) => return Some(result),
            Seen::Replaced => return None,
            Seen::Rolling => {}
        }
        deadline = progress_deadline(&deployment);
    }
    None
}

async fn read(
    cluster: &dyn Cluster,
    namespace: &str,
    name: &str,
) -> Result<Option<Deployment>, ClusterError> {
    let deployments = ResourceRef::new("apps", "v1", "deployments", "Deployment");
    let found = match cluster
        .find_object(&deployments, Some(namespace), name)
        .await
    {
        Ok(found) => found,
        Err(ClusterError::InvalidName(_)) => None,
        Err(error) => return Err(error),
    };
    found
        .map(|object| {
            serde_json::to_value(object)
                .and_then(serde_json::from_value)
                .map_err(|e| ClusterError::Decode(e.to_string()))
        })
        .transpose()
}

fn progress_deadline(deployment: &Deployment) -> Duration {
    deployment
        .spec
        .as_ref()
        .and_then(|spec| spec.progress_deadline_seconds)
        .and_then(|seconds| u64::try_from(seconds).ok())
        .map_or(PROGRESS_DEADLINE, Duration::from_secs)
}

/// What one read of the Deployment says about the rollout of `generation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Seen {
    /// It ended.
    Finished(Rollout),
    /// A newer change was written.
    Replaced,
    /// It goes on.
    Rolling,
}

/// The checks of `kubectl rollout status` for a Deployment
/// ([deployment status](https://kubernetes.io/docs/concepts/workloads/controllers/deployment/#deployment-status)):
/// once the controller has seen `generation`, a `Progressing` condition that is `False` with
/// `ProgressDeadlineExceeded` ends it as failed, and it is available when every replica is
/// updated and available. An absent `spec.replicas` is 1, the default kube-apiserver sets.
pub(crate) fn seen(deployment: &Deployment, generation: i64) -> Seen {
    if deployment.metadata.generation.unwrap_or(0) > generation {
        return Seen::Replaced;
    }
    let status = deployment.status.clone().unwrap_or_default();
    if status.observed_generation.unwrap_or(0) < generation {
        return Seen::Rolling;
    }
    let exceeded = status.conditions.unwrap_or_default().iter().any(|c| {
        c.type_ == "Progressing"
            && c.status == "False"
            && c.reason.as_deref() == Some("ProgressDeadlineExceeded")
    });
    if exceeded {
        return Seen::Finished(Rollout::ProgressDeadlineExceeded);
    }
    let wanted = deployment
        .spec
        .as_ref()
        .and_then(|spec| spec.replicas)
        .unwrap_or(1);
    let updated = status.updated_replicas.unwrap_or(0);
    let available = updated >= wanted
        && status.replicas.unwrap_or(0) <= updated
        && status.available_replicas.unwrap_or(0) >= updated;
    if available {
        Seen::Finished(Rollout::Available)
    } else {
        Seen::Rolling
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn deployment(value: Value) -> Deployment {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn available_means_every_replica_is_updated_and_available() {
        let at = |status: Value| {
            seen(
                &deployment(json!({"metadata": {"generation": 5},
                    "spec": {"replicas": 3}, "status": status})),
                5,
            )
        };
        let all = json!({"observedGeneration": 5, "replicas": 3, "updatedReplicas": 3,
                         "availableReplicas": 3});
        assert_eq!(at(all), Seen::Finished(Rollout::Available));
        let old_pod_left = json!({"observedGeneration": 5, "replicas": 4, "updatedReplicas": 3,
                                  "availableReplicas": 3});
        assert_eq!(at(old_pod_left), Seen::Rolling);
        let not_yet_seen = json!({"observedGeneration": 4, "replicas": 3, "updatedReplicas": 3,
                                  "availableReplicas": 3});
        assert_eq!(at(not_yet_seen), Seen::Rolling);
        let scaled_to_zero = deployment(json!({"metadata": {"generation": 2},
            "spec": {"replicas": 0}, "status": {"observedGeneration": 2}}));
        assert_eq!(seen(&scaled_to_zero, 2), Seen::Finished(Rollout::Available));
    }

    #[test]
    fn only_a_progress_deadline_ends_it_as_failed() {
        let condition = |status: &str, reason: &str| {
            seen(
                &deployment(json!({"metadata": {"generation": 5},
                    "status": {"observedGeneration": 5, "conditions": [
                        {"type": "Progressing", "status": status, "reason": reason}]}})),
                5,
            )
        };
        assert_eq!(
            condition("False", "ProgressDeadlineExceeded"),
            Seen::Finished(Rollout::ProgressDeadlineExceeded)
        );
        assert_eq!(condition("True", "ReplicaSetUpdated"), Seen::Rolling);
        assert_eq!(condition("False", "SomethingElse"), Seen::Rolling);
    }

    #[test]
    fn a_newer_generation_replaces_the_change() {
        let newer = deployment(json!({"metadata": {"generation": 6},
            "status": {"observedGeneration": 6}}));
        assert_eq!(seen(&newer, 5), Seen::Replaced);
    }

    #[test]
    fn a_refusal_is_named_by_its_status() {
        assert_eq!(refusal(403), Submitted::Forbidden);
        assert_eq!(refusal(409), Submitted::Conflict);
        assert_eq!(refusal(422), Submitted::Invalid);
        assert_eq!(refusal(503), Submitted::Unreachable);
        assert_eq!(refusal(400), Submitted::Other);
    }
}
