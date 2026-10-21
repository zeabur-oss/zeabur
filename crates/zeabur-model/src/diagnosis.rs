//! Why a service is unhealthy, in words a client translates.

use serde::{Deserialize, Serialize};
pub use zeabur_kernel::message::Message;

use crate::source::Origin;

/// What kind of failure a diagnosis explains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum DiagnosisKind {
    /// A long-running container keeps exiting and being restarted.
    CrashLoop,
    /// A Job's or CronJob's run ended with an error.
    RunFailed,
    /// The kernel killed a container for using more memory than allowed.
    OutOfMemory,
    /// The image cannot be pulled.
    ImagePull,
    /// A liveness, readiness or startup probe keeps failing.
    ProbeFailure,
    /// Traffic cannot reach it: something sends to it by name but no Service selects its pods,
    /// an `HTTPRoute` to it is not accepted, its Service has no ready pod, or a Service's
    /// `targetPort` names no port of its containers.
    Unreachable,
    /// A `ResourceQuota` keeps its pods from being created.
    QuotaBlocked,
    /// The scheduler finds no node for its pods.
    Unschedulable,
    /// A container cannot start because a Secret or ConfigMap it reads, or a key in one, is
    /// missing (`CreateContainerConfigError`), or a volume from one cannot be mounted.
    ConfigMissing,
    /// It is failing and no other diagnosis explains why: the fallback, so the raw reason is
    /// never the only thing said.
    Unexplained,
}

/// One explained failure of a service's container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Diagnosis {
    /// What kind of failure.
    pub kind: DiagnosisKind,
    /// The container it is about; absent for what is about the service as a whole (traffic,
    /// quota, scheduling, the fallback).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub container: String,
    /// The pods it was seen in, sorted.
    pub pods: Vec<String>,
    /// The cause, in one sentence.
    pub cause: Message,
    /// Why: the rule that applies, with this occurrence's numbers where Zeabur knows them.
    pub why: Message,
    /// What to do about it, in one sentence.
    pub fix: Message,
    /// The status fields and events it rests on.
    pub evidence: Vec<Origin>,
    /// When it last happened, in RFC 3339, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<String>,
    /// Where the cause lies and who can remove it.
    #[serde(default)]
    pub fault: Fault,
    /// The cluster incident this is a symptom of ([`Incident::id`]); set only in answers that
    /// read the incidents too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incident: Option<String>,
}

/// Where the cause of a failure lies, so a cluster's fault never reads as the reader's.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum FaultOrigin {
    /// The program in the image: it exits, crashes or does not answer.
    Code,
    /// How the service is set up: its command, ports, probes, variables, image name or limits.
    Configuration,
    /// The cluster: its nodes, capacity or quotas.
    Cluster,
    /// A component installed in the cluster, named in [`Fault::component`].
    Component,
    /// A service outside the cluster, named in [`Fault::component`]: an image registry.
    Upstream,
    /// Zeabur itself.
    Zeabur,
    /// Nothing Zeabur reads tells where it lies.
    #[default]
    Unknown,
}

/// Who can remove the cause.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum Fixer {
    /// Whoever may change the service.
    #[default]
    You,
    /// A cluster administrator.
    ClusterAdmin,
    /// Whoever runs the component or upstream service named in [`Fault::component`].
    ComponentOwner,
}

/// Where a failure's cause lies, and who can fix it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Fault {
    /// Where the cause lies.
    pub origin: FaultOrigin,
    /// The component or upstream service, for [`FaultOrigin::Component`] and
    /// [`FaultOrigin::Upstream`]: `ingress-controller`, `cert-manager`, `cluster-dns`,
    /// `metrics-server`, `node/<name>`, or a registry's host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    /// Who can fix it.
    pub fixer: Fixer,
}

impl Fault {
    /// A fault of `origin` that `fixer` removes.
    #[must_use]
    pub const fn new(origin: FaultOrigin, fixer: Fixer) -> Self {
        Self {
            origin,
            component: None,
            fixer,
        }
    }

    /// This fault, naming `component`.
    #[must_use]
    pub fn of(self, component: impl Into<String>) -> Self {
        Self {
            component: Some(component.into()),
            ..self
        }
    }
}

/// What kind of cluster-side cause an incident is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum IncidentKind {
    /// A component with no ready replica: an ingress controller, cert-manager, cluster DNS or
    /// metrics-server.
    ComponentDown,
    /// A node whose `Ready` condition is not `True`.
    NodeNotReady,
}

/// A service an incident reaches.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct AffectedService {
    /// Its id, `projects/<project>/services/<kind>/<name>`.
    pub id: String,
    /// The project.
    pub project: String,
    /// The workload's kind.
    pub kind: crate::WorkloadKind,
    /// The workload's name.
    pub name: String,
    /// The kinds of its diagnoses this incident explains, sorted; empty when it shows none yet.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symptoms: Vec<DiagnosisKind>,
}

/// One cause on the cluster's side with everything it reaches, told once instead of as a
/// separate problem of every service it breaks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Incident {
    /// `incidents/<kind>/<name>`, stable while the cause lasts, so diagnoses link to it.
    pub id: String,
    /// What kind of cause.
    pub kind: IncidentKind,
    /// The cause, in one sentence.
    pub cause: Message,
    /// What stops working because of it.
    pub why: Message,
    /// What to do, and who does it.
    pub fix: Message,
    /// Where it lies and who can fix it.
    pub fault: Fault,
    /// The services it reaches, sorted by id, as far as the reader could see them.
    pub services: Vec<AffectedService>,
    /// The public domains it reaches, sorted.
    pub domains: Vec<String>,
    /// The projects it reaches, sorted.
    pub projects: Vec<String>,
    /// The objects and fields it rests on.
    pub evidence: Vec<Origin>,
    /// Since when, in RFC 3339, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
}
