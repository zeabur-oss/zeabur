//! Why a service is unhealthy, in words a client translates.

pub use engawa_core::message::Message;
use serde::{Deserialize, Serialize};

use crate::source::Origin;

/// What kind of failure a diagnosis explains.
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
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
    /// Why: the rule that applies, with this occurrence's numbers where Engawa knows them.
    pub why: Message,
    /// What to do about it, in one sentence.
    pub fix: Message,
    /// The status fields and events it rests on.
    pub evidence: Vec<Origin>,
    /// When it last happened, in RFC 3339, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<String>,
}
