//! How healthy a service is, as one word, and the counts for a project.

use serde::{Deserialize, Serialize};

/// A service's health, from worst to best: what a status dot shows.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum Health {
    /// A pod keeps failing, the workload failed, or no replica is ready.
    Fail,
    /// Some replica serves, but fewer than wanted with no rollout under way, or an update is stuck.
    Warn,
    /// A rollout is under way.
    Progress,
    /// Every wanted replica is ready.
    #[default]
    Ok,
    /// Nothing is meant to run now: paused, suspended, completed, scaled to zero, a CronJob
    /// between runs, or a status too thin to judge.
    Idle,
}

/// What a service is doing, in one word for a person. An open set: clients must show an unknown
/// value by its [`Health`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum StatusWord {
    /// Every wanted replica is ready.
    Ready,
    /// A rollout is under way.
    RollingOut,
    /// A pod keeps failing.
    Failing,
    /// Fewer replicas are ready than wanted, or the workload says it is degraded.
    Degraded,
    /// A CronJob between runs.
    Scheduled,
    /// Paused or suspended.
    Paused,
    /// The workload failed.
    Failed,
    /// An update does not finish, while replicas of the release before it still serve.
    RolloutStuck,
    /// Scaled to zero replicas.
    ScaledToZero,
    /// A Job that finished.
    Completed,
    /// The workload's status is too thin to judge.
    Unknown,
    /// Its image is still being built: Zeabur Builder has not pushed the first one yet.
    Building,
    /// Its image comes from an uploaded folder or archive that nobody has uploaded yet.
    WaitingUpload,
}

/// A service's health, the word for it, and why when a pod keeps failing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatus {
    /// What its status dot shows.
    pub health: Health,
    /// The word for it.
    pub word: StatusWord,
    /// The first failing pod's reason, as `kubectl get pods` shows it (`CrashLoopBackOff`), or,
    /// for a pod that runs again after a recent crash, how that crash ended (`Error`); absent
    /// when no pod fails or the kubelet gives no reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// What `reason` means, in one word a client translates; absent for a reason with no word.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<StatusCause>,
}

/// What a failing pod's reason means for a person. An open set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum StatusCause {
    /// A long-running container keeps exiting (`CrashLoopBackOff`, or `Error`).
    Crashing,
    /// A run of a Job or CronJob ended with an error (`Error`).
    RunFailed,
    /// The kernel stopped a container for its memory (`OOMKilled`).
    OutOfMemory,
    /// The image cannot be pulled.
    ImagePull,
    /// The image name is not valid.
    ImageName,
    /// A Secret or ConfigMap that it reads is missing.
    ConfigMissing,
    /// The container cannot start.
    CantStart,
    /// No node has room for it.
    WaitingForRoom,
    /// Its containers are being made.
    Starting,
    /// The node stopped it.
    Evicted,
    /// The release made no progress before its deadline.
    ReleaseStuck,
    /// The Job stopped after its retries.
    GaveUp,
    /// The build of its image failed.
    BuildFailed,
    /// Pod Security keeps it from running. Only a diagnosis gives this word, never a reason.
    Security,
}

impl StatusCause {
    /// The word for `reason`, as `kubectl get pods` shows it. `Error` is how a run ended: the
    /// run of a task (`task`) failed, and a service that keeps restarting crashed.
    #[must_use]
    pub fn of(reason: Option<&str>, task: bool) -> Option<Self> {
        Some(match reason? {
            "Error" if task => Self::RunFailed,
            "Error" | "CrashLoopBackOff" => Self::Crashing,
            "OOMKilled" => Self::OutOfMemory,
            "ErrImagePull" | "ImagePullBackOff" => Self::ImagePull,
            "InvalidImageName" => Self::ImageName,
            "CreateContainerConfigError" => Self::ConfigMissing,
            "ContainerCannotRun" | "RunContainerError" | "StartError" => Self::CantStart,
            "Unschedulable" => Self::WaitingForRoom,
            "ContainerCreating" | "PodInitializing" => Self::Starting,
            "Evicted" => Self::Evicted,
            "ProgressDeadlineExceeded" => Self::ReleaseStuck,
            "BackoffLimitExceeded" => Self::GaveUp,
            "BuildFailed" => Self::BuildFailed,
            _ => return None,
        })
    }
}

/// How many of a project's services are in each state, for lists that show a project's health
/// without its whole document.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ProjectHealth {
    /// Services (workloads, without the Jobs a CronJob runs).
    pub services: usize,
    /// Services that are fine: [`Health::Ok`] or [`Health::Idle`].
    pub healthy: usize,
    /// Services rolling out ([`Health::Progress`]).
    pub progressing: usize,
    /// Services with too few ready replicas ([`Health::Warn`]).
    pub warning: usize,
    /// Services failing ([`Health::Fail`]).
    pub failing: usize,
    /// The worst health of any service; [`Health::Ok`] when every one is fine or there are none.
    pub worst: Health,
}

impl ProjectHealth {
    /// Counts `healths`, one per service.
    #[must_use]
    pub fn count(healths: impl IntoIterator<Item = Health>) -> Self {
        let mut out = Self::default();
        for health in healths {
            out.services += 1;
            match health {
                Health::Fail => out.failing += 1,
                Health::Warn => out.warning += 1,
                Health::Progress => out.progressing += 1,
                Health::Ok | Health::Idle => out.healthy += 1,
            }
        }
        out.worst = if out.failing > 0 {
            Health::Fail
        } else if out.warning > 0 {
            Health::Warn
        } else if out.progressing > 0 {
            Health::Progress
        } else {
            Health::Ok
        };
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_worst_health_ranks_fail_warn_progress_then_ok() {
        let worst = |healths: &[Health]| ProjectHealth::count(healths.iter().copied()).worst;
        assert_eq!(worst(&[]), Health::Ok);
        assert_eq!(worst(&[Health::Idle, Health::Ok]), Health::Ok);
        assert_eq!(worst(&[Health::Ok, Health::Progress]), Health::Progress);
        assert_eq!(worst(&[Health::Progress, Health::Warn]), Health::Warn);
        assert_eq!(worst(&[Health::Warn, Health::Fail]), Health::Fail);
    }

    #[test]
    fn a_reason_names_its_cause() {
        assert_eq!(
            StatusCause::of(Some("Error"), false),
            Some(StatusCause::Crashing)
        );
        assert_eq!(
            StatusCause::of(Some("Error"), true),
            Some(StatusCause::RunFailed)
        );
        assert_eq!(
            StatusCause::of(Some("ImagePullBackOff"), false),
            Some(StatusCause::ImagePull)
        );
        assert_eq!(
            StatusCause::of(Some("Unschedulable"), false),
            Some(StatusCause::WaitingForRoom)
        );
        assert_eq!(StatusCause::of(Some("Whatever"), false), None);
        assert_eq!(StatusCause::of(None, false), None);
    }
}
