//! Workloads as clients list them.

use serde::{Deserialize, Serialize};

use crate::provenance::Provenance;
use crate::source::Origin;

/// One pod of a workload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PodSummary {
    /// Its resource name, `projects/<project>/pods/<name>`; see [`crate::id`].
    pub id: String,
    /// The pod's name.
    pub name: String,
    /// `Pending`, `Running`, `Succeeded`, `Failed` or `Unknown`, as the kubelet reports it.
    pub phase: String,
    /// Whether every container is ready.
    pub ready: bool,
    /// Restarts, summed over its containers.
    pub restarts: i32,
    /// `fail` when it keeps failing, `ok` when ready, `idle` when it succeeded, `warn` otherwise.
    pub health: crate::Health,
    /// Its containers' names, for picking whose log to read.
    pub containers: Vec<String>,
    /// Why it is not running or not ready, as `kubectl get pods` shows it: a container's waiting
    /// reason (`CrashLoopBackOff`, `ImagePullBackOff`, `CreateContainerConfigError`), or, for a
    /// container that is not ready, why it last stopped (`OOMKilled`, `Error`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// When it was created, in RFC 3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// The node it runs on, once scheduled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// Each container's state as the kubelet reports it, init containers first; empty before
    /// the kubelet reports any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub container_statuses: Vec<ContainerStatusSummary>,
    /// The Pod object.
    pub origin: Origin,
}

/// One container's state in a pod [k8s-container-states].
///
/// [k8s-container-states]: https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/#container-states
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContainerStatusSummary {
    /// The container's name.
    pub name: String,
    /// An init container (including a restartable "sidecar").
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub init: bool,
    /// Whether it passes its readiness probe.
    pub ready: bool,
    /// How many times the kubelet restarted it.
    pub restarts: i32,
    /// Why it is waiting, when it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting: Option<ContainerWaiting>,
    /// How it ended, when it is terminated now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminated: Option<ContainerTermination>,
    /// How its previous run ended, after a restart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_termination: Option<ContainerTermination>,
}

/// Why a container is waiting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContainerWaiting {
    /// `CrashLoopBackOff`, `ImagePullBackOff`, `ErrImagePull`, `CreateContainerConfigError`, …
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The kubelet's message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// How a container run ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContainerTermination {
    /// `Error`, `Completed`, `OOMKilled`, `ContainerCannotRun`, …
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The process's exit code.
    pub exit_code: i32,
    /// When it started, in RFC 3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started: Option<String>,
    /// When it ended, in RFC 3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished: Option<String>,
    /// The container's termination message, or the runtime's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// The workload kinds Engawa projects.
///
/// Declared in alphabetical order, so the derived [`Ord`] sorts as the kind names do.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    utoipa::ToSchema,
)]
pub enum WorkloadKind {
    /// `batch/v1` `CronJob`.
    CronJob,
    /// `apps/v1` `DaemonSet`.
    DaemonSet,
    /// `apps/v1` `Deployment`.
    Deployment,
    /// `batch/v1` `Job`.
    Job,
    /// `apps/v1` `StatefulSet`.
    StatefulSet,
}

impl WorkloadKind {
    /// Every kind, in [`Ord`] order.
    pub const ALL: [Self; 5] = [
        Self::CronJob,
        Self::DaemonSet,
        Self::Deployment,
        Self::Job,
        Self::StatefulSet,
    ];

    /// The fields from a workload of this kind down to its pod template: a CronJob's is inside
    /// its Job template [k8s-cronjob].
    ///
    /// [k8s-cronjob]: https://kubernetes.io/docs/reference/kubernetes-api/workload-resources/cron-job-v1/
    #[must_use]
    pub const fn pod_template_path(self) -> &'static [&'static str] {
        match self {
            Self::CronJob => &["spec", "jobTemplate", "spec", "template"],
            Self::DaemonSet | Self::Deployment | Self::Job | Self::StatefulSet => {
                &["spec", "template"]
            }
        }
    }

    /// The Kubernetes kind name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CronJob => "CronJob",
            Self::DaemonSet => "DaemonSet",
            Self::Deployment => "Deployment",
            Self::Job => "Job",
            Self::StatefulSet => "StatefulSet",
        }
    }

    /// The kind as a path segment: lower case (`deployment`); see [`crate::id`].
    #[must_use]
    pub const fn segment(self) -> &'static str {
        match self {
            Self::CronJob => "cronjob",
            Self::DaemonSet => "daemonset",
            Self::Deployment => "deployment",
            Self::Job => "job",
            Self::StatefulSet => "statefulset",
        }
    }

    /// The kind named `text` in any case (`deployment`, `Deployment`, `DEPLOYMENT`).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str().eq_ignore_ascii_case(text))
    }
}

/// A one-word aggregate of a workload's status for lists. The Kubernetes conditions stay the
/// authority; this is only the badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
pub enum Phase {
    /// Every desired replica is up to date and ready.
    Running,
    /// A rollout is under way.
    Progressing,
    /// Nothing is ready although no rollout is pending.
    Degraded,
    /// Paused, or scaled to zero.
    Paused,
    /// A suspended Job or CronJob.
    Suspended,
    /// A Job that finished successfully.
    Completed,
    /// A rollout past its progress deadline, or a failed Job.
    Failed,
    /// The status says too little to tell.
    Unknown,
}

/// The custom resource whose operator controls a workload. Direct edits to the workload are
/// reverted by that operator; the resource is what to change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManagedBy {
    /// The resource's kind as people create it, such as `PerconaPGCluster`.
    pub kind: String,
    /// The resource's name.
    pub name: String,
    /// The database engine, such as `PostgreSQL`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
}

/// Where a workload is listed and under which name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Classification {
    /// The group the workload is listed in.
    pub group: String,
    /// The name to show instead of the workload's, when it stands for something a person named,
    /// such as the database cluster whose data it holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// The operator-managed resource the workload belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_by: Option<ManagedBy>,
}

/// Ready and desired replicas (per node for a DaemonSet, completions for a Job).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Replicas {
    /// Ready (or, for a Job, succeeded) pods.
    pub ready: i32,
    /// Desired pods.
    pub desired: i32,
}

/// A workload as lists show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkloadSummary {
    /// The workload's kind.
    pub kind: WorkloadKind,
    /// The workload's name.
    pub name: String,
    /// The workload's namespace.
    pub namespace: String,
    /// Group, display name and managing resource.
    #[serde(flatten)]
    pub classification: Classification,
    /// The status badge.
    pub phase: Phase,
    /// Ready and desired replicas; absent for a CronJob, which has no meaningful ratio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicas: Option<Replicas>,
    /// The first container's image, as a quick identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// The tool that manages the workload, if any.
    pub provenance: Provenance,
    /// The workload object.
    pub origin: Origin,
}
