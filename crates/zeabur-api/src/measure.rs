//! Changes of one Deployment and how their rollout ends, for the experience program
//! (docs/internal/measurement.md). Every read is made with the person's own credential.

use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;
use zeabur_core::crypto;
use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
use zeabur_kube::{Cluster, ClusterError, ClusterExt as _, ResourceRef};
use zeabur_model::WorkloadKind;
use zeabur_model::readiness::{self, Status};
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
/// How long one read, with the credential it needs, may take.
const READ: Duration = Duration::from_secs(10);
/// `spec.progressDeadlineSeconds` when the Deployment does not say.
const PROGRESS_DEADLINE: Duration = Duration::from_secs(600);
/// How long past the progress deadline the controller gets to set the condition.
const GRACE: Duration = Duration::from_secs(60);
/// The longest watch of one change, whatever progress deadline the Deployment sets.
const WINDOW: Duration = Duration::from_mins(30);

/// The Deployment object a write left: a Deployment deleted and made again with the same name
/// has a new `uid`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Version {
    pub(crate) uid: String,
    pub(crate) generation: i64,
}

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
        Change::Create { .. } | Change::CreateImageService(_) => Operation::DeployImage,
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
/// left `version` is then read, as the person, until its rollout ends.
pub(crate) fn submitted(
    state: &AppState,
    person: Person<'_>,
    namespace: &str,
    measured: Measured,
    answer: Result<Option<&Version>, &ApiError>,
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
    let Ok(Some(version)) = answer else {
        return;
    };
    let version = version.clone();
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
        if let Some(result) = follow(&later, &namespace, &measured.name, &version).await {
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

/// Reads the Deployment until the rollout of `version` ends, as `kubectl rollout status`
/// does. `None` when a newer change or a new object replaced it, the person signed out or can no
/// longer read it, or the [`watch`] ended with no result. A read that takes longer than
/// [`READ`] is skipped.
async fn follow(person: &Later, namespace: &str, name: &str, version: &Version) -> Option<Rollout> {
    let started = Instant::now();
    let mut until = watch(PROGRESS_DEADLINE);
    while started.elapsed() < until {
        tokio::time::sleep(POLL).await;
        let Ok(cluster) = tokio::time::timeout(READ, person()).await else {
            continue;
        };
        let cluster = cluster?;
        let deployment =
            match tokio::time::timeout(READ, read(cluster.as_ref(), namespace, name)).await {
                Ok(Ok(Some(deployment))) => deployment,
                Ok(Ok(None)) => return None,
                Ok(Err(error)) if error.is_refused() => return None,
                Ok(Err(_)) | Err(_) => continue,
            };
        match seen(&deployment, version) {
            Seen::Finished(result) => return Some(result),
            Seen::Replaced => return None,
            Seen::Rolling => {}
        }
        until = watch(progress_deadline(&deployment));
    }
    None
}

/// How long a change is watched: the progress deadline and [`GRACE`], at most [`WINDOW`].
fn watch(progress_deadline: Duration) -> Duration {
    progress_deadline.saturating_add(GRACE).min(WINDOW)
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

/// What one read of the Deployment says about the rollout of `version`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Seen {
    /// It ended.
    Finished(Rollout),
    /// A newer change was written, or the object was deleted and made again.
    Replaced,
    /// It goes on.
    Rolling,
}

/// The rollout of `version` by [`readiness::deployment`]: current is available, a passed
/// progress deadline fails it, and anything else rolls on. A newer generation or a new object
/// with the same name replaces it.
pub(crate) fn seen(deployment: &Deployment, version: &Version) -> Seen {
    if deployment.metadata.uid.as_deref() != Some(version.uid.as_str())
        || deployment.metadata.generation.unwrap_or(0) > version.generation
    {
        return Seen::Replaced;
    }
    let state = readiness::deployment(deployment);
    match state.status {
        Status::Current => Seen::Finished(Rollout::Available),
        Status::Failed => Seen::Finished(Rollout::ProgressDeadlineExceeded),
        Status::InProgress => Seen::Rolling,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn deployment(mut value: Value) -> Deployment {
        if value["metadata"]["uid"].is_null() {
            value["metadata"]["uid"] = json!("a");
        }
        serde_json::from_value(value).unwrap()
    }

    fn version(generation: i64) -> Version {
        Version {
            uid: "a".to_owned(),
            generation,
        }
    }

    #[test]
    fn available_means_the_deployment_is_current_by_kstatus() {
        let at = |replicas: i64, status: Value| {
            seen(
                &deployment(json!({"metadata": {"generation": 5},
                    "spec": {"replicas": replicas}, "status": status})),
                &version(5),
            )
        };
        let available = json!([{"type": "Available", "status": "True"}]);
        let all = json!({"observedGeneration": 5, "replicas": 3, "updatedReplicas": 3,
                         "readyReplicas": 3, "availableReplicas": 3, "conditions": available});
        assert_eq!(at(3, all.clone()), Seen::Finished(Rollout::Available));
        let mut old_pod_left = all.clone();
        old_pod_left["replicas"] = json!(4);
        assert_eq!(at(3, old_pod_left), Seen::Rolling);
        let mut not_yet_seen = all.clone();
        not_yet_seen["observedGeneration"] = json!(4);
        assert_eq!(at(3, not_yet_seen), Seen::Rolling);
        let mut no_condition = all;
        no_condition["conditions"] = json!([]);
        assert_eq!(at(3, no_condition), Seen::Rolling);
        let scaled_to_zero = json!({"observedGeneration": 5, "conditions": available});
        assert_eq!(at(0, scaled_to_zero), Seen::Finished(Rollout::Available));
    }

    #[test]
    fn only_a_progress_deadline_ends_it_as_failed() {
        let condition = |status: &str, reason: &str| {
            seen(
                &deployment(json!({"metadata": {"generation": 5},
                    "status": {"observedGeneration": 5, "conditions": [
                        {"type": "Progressing", "status": status, "reason": reason}]}})),
                &version(5),
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
        assert_eq!(seen(&newer, &version(5)), Seen::Replaced);
    }

    #[test]
    fn a_new_object_with_the_same_name_replaces_the_change() {
        let made_again = deployment(json!({"metadata": {"uid": "b", "generation": 5},
            "spec": {"replicas": 1}, "status": {"observedGeneration": 5, "replicas": 1,
            "updatedReplicas": 1, "availableReplicas": 1}}));
        assert_eq!(seen(&made_again, &version(5)), Seen::Replaced);
    }

    #[test]
    fn the_watch_ends_by_the_window() {
        assert_eq!(watch(Duration::from_mins(10)), Duration::from_mins(11));
        assert_eq!(watch(Duration::from_hours(24)), WINDOW);
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
