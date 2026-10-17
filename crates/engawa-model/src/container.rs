//! A container's environment and storage.

use engawa_core::quantity;
use serde::{Deserialize, Serialize};

use crate::attribution::Attribution;
use crate::is_false;
use crate::provenance::Tool;
use crate::source::{ObjectRef, Origin, Source};

/// One container of a workload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Container {
    /// The container's name.
    pub name: String,
    /// The image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// An init container (including a restartable "sidecar").
    #[serde(default, skip_serializing_if = "is_false")]
    pub init: bool,
    /// The `env` entries, in order.
    pub env: Vec<EnvVar>,
    /// The `envFrom` imports, in order.
    pub imported_env: Vec<ImportedEnv>,
    /// The volume mounts, in order.
    pub mounts: Vec<Mount>,
    /// The CPU and memory it requests and is limited to; absent when it sets neither.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<ContainerResources>,
    /// Its liveness, readiness and startup probes; absent when it sets none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probes: Option<Probes>,
    /// The container in its workload.
    pub origin: Origin,
}

/// A container's probes [k8s-probes].
///
/// Liveness restarts the container when it fails, readiness takes the pod out of its Services'
/// endpoints, and startup holds the other two back until the application has started.
///
/// [k8s-probes]: https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/#container-probes
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Probes {
    /// The liveness probe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liveness: Option<ProbeSummary>,
    /// The readiness probe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readiness: Option<ProbeSummary>,
    /// The startup probe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startup: Option<ProbeSummary>,
}

impl Probes {
    /// No probe is set.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.liveness.is_none() && self.readiness.is_none() && self.startup.is_none()
    }
}

/// What one probe checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProbeSummary {
    /// How it checks.
    pub handler: ProbeHandler,
    /// The port, a number or a container port name (HTTP, TCP and gRPC).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    /// The path (HTTP).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// A probe's handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[schema(as = ContainerProbeHandler)]
pub enum ProbeHandler {
    /// `httpGet`.
    #[serde(rename = "HTTP")]
    Http,
    /// `tcpSocket`.
    #[serde(rename = "TCP")]
    Tcp,
    /// `grpc`.
    #[serde(rename = "gRPC")]
    Grpc,
    /// `exec`.
    #[serde(rename = "exec")]
    Exec,
}

/// A container's `resources`, as written: requests are what the scheduler reserves, limits what
/// the kubelet enforces [k8s-resources].
///
/// [k8s-resources]: https://kubernetes.io/docs/concepts/configuration/manage-resources-containers/
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContainerResources {
    /// What the scheduler reserves.
    #[serde(default, skip_serializing_if = "Quantities::is_empty")]
    pub requests: Quantities,
    /// What the kubelet enforces.
    #[serde(default, skip_serializing_if = "Quantities::is_empty")]
    pub limits: Quantities,
}

/// CPU and memory as Kubernetes quantity strings, such as `250m` and `512Mi`, and as numbers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Quantities {
    /// CPU.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu: Option<String>,
    /// Memory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    /// CPU in millicores, rounded up; absent when `cpu` is absent or not a quantity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_millicores: Option<u64>,
    /// Memory in bytes, rounded up; absent when `memory` is absent or not a quantity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<u64>,
}

impl Quantities {
    /// The quantities as written, with their numbers.
    #[must_use]
    pub fn new(cpu: Option<String>, memory: Option<String>) -> Self {
        Self {
            cpu_millicores: cpu.as_deref().and_then(quantity::millicores),
            memory_bytes: memory.as_deref().and_then(quantity::bytes),
            cpu,
            memory,
        }
    }

    /// Neither CPU nor memory is set.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.cpu.is_none() && self.memory.is_none()
    }
}

/// One `env` entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EnvVar {
    /// The variable's name.
    pub name: String,
    /// The value, for a literal only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Where the value is read from.
    pub source: Source,
    /// A Secret-backed value: it is never part of the projection. Revealing it is a separate
    /// read made with the person's own credential.
    #[serde(default, skip_serializing_if = "is_false")]
    pub masked: bool,
    /// The entry in its workload.
    pub origin: Origin,
}

/// One `envFrom` entry. Imports are not flattened into variables: a ConfigMap's twelve keys
/// shown as twelve entries would misrepresent what the workload says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportedEnv {
    /// The imported ConfigMap or Secret.
    pub source: Source,
    /// The prefix added to every imported name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    /// How many keys the imported object holds now; absent when that is unknown (the object
    /// is missing, or its key count was not read).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_count: Option<usize>,
    /// The import must not be edited in place; see [`ReadOnlyReason`].
    pub read_only: bool,
    /// Why it is read-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only_reason: Option<ReadOnlyReason>,
    /// The entry in its workload.
    pub origin: Origin,
}

/// Why an `envFrom` entry is read-only. Either way the way out is the same: override one
/// variable with a same-named `env` entry, which takes precedence and leaves the import alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "reason", rename_all = "camelCase")]
pub enum ReadOnlyReason {
    /// Another field manager owns the container's `envFrom`. The list is atomic, so changing one
    /// entry takes the whole list from that manager and its next apply conflicts.
    #[serde(rename_all = "camelCase")]
    ListManagedBy {
        /// The field manager.
        manager: String,
    },
    /// The imported object is managed by a tool that restores its contents on the next sync.
    #[serde(rename_all = "camelCase")]
    SourceManagedBy {
        /// The tool.
        tool: Tool,
    },
}

/// What kind of storage a mount is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum MountCategory {
    /// Durable storage, paid for, that outlives the pod.
    Persistent,
    /// Configuration: a ConfigMap, Secret, projected or downward API volume.
    Config,
    /// Scratch space that goes with the pod, or a volume whose kind is unknown.
    Ephemeral,
}

/// One volume mount of a container.
#[expect(
    clippy::struct_field_names,
    reason = "the field names are the Kubernetes ones"
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Mount {
    /// The volume's name in the pod spec.
    pub name: String,
    /// Where it is mounted.
    pub mount_path: String,
    /// The sub-path mounted, if not the whole volume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_path: Option<String>,
    /// Mounted read-only.
    #[serde(default, skip_serializing_if = "is_false")]
    pub read_only: bool,
    /// The volume source kind (`PersistentVolumeClaim`, `ConfigMap`, `Secret`, `Projected`,
    /// `DownwardAPI`, `EmptyDir`, `Ephemeral`, `Other`), or `Unknown` when no volume has the
    /// mount's name.
    pub kind: String,
    /// Persistent, config or ephemeral.
    pub category: MountCategory,
    /// The claim: the PVC's name, or for a StatefulSet's claim template the template's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_name: Option<String>,
    /// The ConfigMap, Secret or PVC the volume reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<ObjectRef>,
    /// The requested size, or an `emptyDir` size limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// `size` in bytes, rounded up; absent when `size` is absent or not a quantity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// The claim's access modes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub access_modes: Vec<String>,
    /// The claim's storage class.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_class: Option<String>,
    /// The claim's volume mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_mode: Option<String>,
    /// The claim's phase, such as `Bound`, once a PVC exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Which workloads use the ConfigMap, Secret or PVC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<Attribution>,
    /// The mount in its workload.
    pub origin: Origin,
}

impl Mount {
    /// Sets the size as written, and its number.
    pub fn set_size(&mut self, size: Option<String>) {
        self.size_bytes = size.as_deref().and_then(quantity::bytes);
        self.size = size;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantities_carry_their_numbers_and_none_for_what_is_not_one() {
        let q = Quantities::new(Some("250m".to_owned()), Some("1.5Gi".to_owned()));
        assert_eq!(q.cpu_millicores, Some(250));
        assert_eq!(q.memory_bytes, Some(3 << 29));
        let odd = Quantities::new(Some("a lot".to_owned()), None);
        assert_eq!(
            (odd.cpu.as_deref(), odd.cpu_millicores),
            (Some("a lot"), None)
        );
        assert_eq!(odd.memory_bytes, None);
    }
}
