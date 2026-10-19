//! The JSON the API returns.
//!
//! # Shapes
//!
//! Three rules give every response the same shape [aip-131] [aip-132]:
//!
//! - **A get returns the resource itself.** No `{project: …}` wrapper: the project, service or
//!   resource is the body, and what the person could not read (`unreadable`) is a field of it.
//! - **A list returns `{items}`** with every item, wrapped so that fields can be added next to
//!   them. Event lists return the newest `limit` and count every match in `totalSize` (see the
//!   `query` module).
//! - **Every value names the object it came from** with one type, [`Origin`]: the projection's
//!   values, events, logs, ConfigMaps, Secrets and projects alike. Responses that stand alone carry
//!   a complete reference (`apiVersion`, `kind`, `namespace`, `name`).
//!
//! Every top-level node also has an `id` in the shape of its API path
//! ([`engawa_model::id`]), and times are absolute RFC 3339 strings, never relative ("3m ago"),
//! so that a client can compare and sort them.
//!
//! [aip-131]: https://google.aip.dev/131
//! [aip-132]: https://google.aip.dev/132

use std::collections::BTreeMap;

use engawa_core::problem::KubernetesStatus;
use engawa_kube::Unreadable;
use engawa_model::{
    Endpoint, Health, Origin, Project, ProjectDetails, ProjectHealth, Route, Service, WorkloadKind,
};
use serde::Serialize;
use utoipa::ToSchema;

/// A kind the person could not read, and kube-apiserver's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnreadableView {
    /// The resource (`secrets`, `deployments.apps`, …).
    pub resource: String,
    /// kube-apiserver's answer.
    pub kubernetes: KubernetesStatus,
}

impl From<Unreadable> for UnreadableView {
    fn from(u: Unreadable) -> Self {
        Self {
            resource: u.resource,
            kubernetes: u.status,
        }
    }
}

/// Who the cluster says the person is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WhoAmI {
    /// User name.
    pub username: String,
    /// Groups.
    pub groups: Vec<String>,
}

/// The cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Cluster {
    /// What the person calls the cluster: in local mode the kubeconfig's name for the cluster
    /// of the context in use, otherwise the host of kube-apiserver's address. Kubernetes gives a
    /// cluster no name of its own.
    pub name: String,
    /// kube-apiserver's version (`gitVersion`, such as `v1.36.5+k3s1`).
    pub kubernetes_version: String,
    /// The distribution, when the version names one: `k3s` or `rke2`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distribution: Option<String>,
}

/// A project in the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    /// `projects/<name>`.
    pub id: String,
    /// The namespace.
    pub origin: Origin,
    /// The project's name: its namespace's name.
    pub name: String,
    /// Whether the namespace carries engawa's project label. Any namespace can be opened; adopting
    /// it only adds the label.
    pub adopted: bool,
    /// The namespace is one Kubernetes keeps for itself (`kube-` prefix), which cannot be
    /// adopted.
    pub system: bool,
    /// The name, description and icon given to it (its namespace's annotations).
    #[serde(flatten)]
    pub details: ProjectDetails,
    /// How many of its services are healthy, rolling out, warning or failing. Only for adopted
    /// projects, and absent when their workloads cannot be read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<ProjectHealth>,
    /// Its services with their health, from the same read as `health`: sorted by kind, then
    /// name, as in the project document. Only for adopted projects, and absent when their
    /// workloads cannot be read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub services: Option<Vec<ServiceSummary>>,
}

/// One service of a project in the list: which it is, how healthy and why, without its
/// containers, variables or configuration (those are in the project document).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSummary {
    /// `projects/<project>/services/<kind>/<name>`.
    pub id: String,
    /// The workload's kind.
    pub kind: WorkloadKind,
    /// The workload's name.
    pub name: String,
    /// Its health, by the rule the project counts use.
    pub health: Health,
    /// Ready replicas (succeeded pods for a Job); absent for a CronJob.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ready: Option<i32>,
    /// Desired replicas (completions for a Job); absent for a CronJob.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desired: Option<i32>,
    /// Restarts, summed over its pods.
    pub restarts: i32,
    /// Why its first failing pod fails, as `kubectl get pods` shows it (`CrashLoopBackOff`);
    /// absent when no pod fails or the kubelet gives no reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The projects (namespaces) the person may list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectList {
    /// Adopted projects first, then the other namespaces, each sorted by name.
    pub items: Vec<ProjectSummary>,
}

/// A project: its services, the resources they use and how they relate, read as the person.
///
/// The one document a client needs for a project: the service, variables and objects views are
/// cut from it rather than read again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDocument {
    /// `projects/<name>`.
    pub id: String,
    /// The projection.
    #[serde(flatten)]
    pub project: Project,
    /// Kinds the person could not list; the projection is missing what they hide.
    pub unreadable: Vec<UnreadableView>,
    /// What the integrations add, by module; a module whose API is not served adds nothing.
    pub extensions: ProjectExtensions,
}

/// What integrations add to a project document, one member per module.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectExtensions {
    /// Who may reach each service, from the project's NetworkPolicy objects. Whether the
    /// cluster's network plugin enforces them is not known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_policy: Option<engawa_modules::network_policy::Reachability>,
}

/// One service of a project, as the project document holds it, with the details of its own
/// endpoints and routes.
///
/// In both documents `endpoints` and `routes` hold the details and a service's
/// `endpointNames` and `routeNames` name its own; here they are only the service's, so the
/// document stays the size of one workload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDocument {
    /// The service.
    #[serde(flatten)]
    pub service: Service,
    /// The Kubernetes Services selecting its pods, in the order of `endpointNames`.
    pub endpoints: Vec<Endpoint>,
    /// The Ingresses routing to them, in the order of `routeNames`.
    pub routes: Vec<Route>,
    /// Kinds the person could not list.
    pub unreadable: Vec<UnreadableView>,
}

/// One event about an object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventView {
    /// `projects/<project>/events/<name>`.
    pub id: String,
    /// The Event object itself.
    pub origin: Origin,
    /// The object the event is about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regarding: Option<Origin>,
    /// `Normal` or `Warning`.
    pub r#type: String,
    /// Machine-readable reason (`BackOff`, `Pulled`, …).
    pub reason: String,
    /// What happened, as the component reported it.
    pub note: String,
    /// How many times it happened.
    pub count: i32,
    /// When it last happened (RFC 3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<String>,
    /// The component that reported it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reporter: Option<String>,
}

/// Events of a project, the newest `limit` of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventList {
    /// Newest first (by `lastSeen`, events without one last), then by name.
    pub items: Vec<EventView>,
    /// How many events matched, also those past `limit`.
    pub total_size: usize,
}

/// A snapshot of a container's log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogSnapshot {
    /// The pod (and container, as `fieldPath`).
    pub origin: Origin,
    /// The lines, oldest first.
    pub lines: Vec<String>,
}

/// A ConfigMap, in full: ConfigMaps hold configuration, not secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigMapView {
    /// `projects/<project>/resources/configmap/<name>`.
    pub id: String,
    /// The ConfigMap.
    pub origin: Origin,
    /// Its text values.
    pub data: BTreeMap<String, String>,
    /// Keys holding binary data (not shown).
    pub binary_keys: Vec<String>,
}

/// The ConfigMaps of a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigMapList {
    /// Sorted by name.
    pub items: Vec<ConfigMapView>,
}

/// One value of a revealed Secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SecretValue {
    /// The value: text as is, binary data as base64.
    pub value: String,
    /// `utf-8` or `base64`.
    pub encoding: String,
}

/// A Secret's values, shown because the person asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RevealedSecret {
    /// The Secret.
    pub origin: Origin,
    /// Its values by key.
    pub values: BTreeMap<String, SecretValue>,
}

/// Which capabilities the cluster has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityList {
    /// The recommendations catalogue's commit.
    pub catalogue_commit: String,
    /// Each capability, sorted by name: existing (with evidence) or not (with what to install).
    pub items: Vec<engawa_recommend::Capability>,
}

/// CPU, memory and pods in plain units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Resources {
    /// CPU in millicores (1000 is one core).
    pub cpu_millicores: u64,
    /// Memory in bytes.
    pub memory_bytes: u64,
    /// Pods; only where a count of pods applies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pods: Option<u64>,
}

/// A node condition that is not as it should be: `Ready` not `True`, or a pressure that is
/// `True`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeCondition {
    /// `Ready`, `MemoryPressure`, `DiskPressure`, `PIDPressure` or `NetworkUnavailable`.
    pub r#type: String,
    /// `True`, `False` or `Unknown`.
    pub status: String,
    /// The kubelet's reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The kubelet's message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// A node taint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Taint {
    /// Its key.
    pub key: String,
    /// Its value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// `NoSchedule`, `PreferNoSchedule` or `NoExecute`.
    pub effect: String,
}

/// A node: its roles, health, versions and capacity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeView {
    /// `cluster/nodes/<name>`.
    pub id: String,
    /// The Node.
    pub origin: Origin,
    /// Its name.
    pub name: String,
    /// Its roles (`control-plane`, `etcd`, …), from `node-role.kubernetes.io/<role>` labels;
    /// empty for a plain worker.
    pub roles: Vec<String>,
    /// The `Ready` condition: `True`, `False` or `Unknown`.
    pub ready: String,
    /// Conditions that are not as they should be.
    pub problems: Vec<NodeCondition>,
    /// Cordoned: new pods are not scheduled here (`spec.unschedulable`).
    pub cordoned: bool,
    /// Its taints.
    pub taints: Vec<Taint>,
    /// The kubelet's version.
    pub kubelet_version: String,
    /// The operating system image.
    pub os_image: String,
    /// The kernel version.
    pub kernel_version: String,
    /// The CPU architecture (`arm64`, `amd64`).
    pub architecture: String,
    /// The container runtime (`containerd://2.1.4-k3s1`).
    pub container_runtime: String,
    /// What pods may use: capacity less what is reserved for the system.
    pub allocatable: Resources,
    /// The machine's capacity.
    pub capacity: Resources,
    /// What the pods placed here request, with their count; absent when pods cannot be listed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requests: Option<Resources>,
    /// What the pods placed here may use at most (containers without a limit add nothing);
    /// absent when pods cannot be listed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits: Option<Resources>,
    /// What it uses now, from metrics-server (approximate, for autoscaling); absent without it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Resources>,
    /// When it joined the cluster (RFC 3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
}

/// The cluster's totals over every node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeTotals {
    /// Allocatable.
    pub allocatable: Resources,
    /// Requested; absent when pods cannot be listed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requests: Option<Resources>,
    /// Limits; absent when pods cannot be listed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits: Option<Resources>,
    /// In use; absent without metrics-server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Resources>,
}

/// How much of the pods were read for the requests. An open set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PodScan {
    /// Every pod.
    Complete,
    /// The read stopped at its limit: the sums are lower than the truth.
    Partial,
    /// Pods could not be listed cluster-wide.
    Unreadable,
}

/// Whether usage could be read. An open set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum MetricsState {
    /// metrics-server answered.
    Available,
    /// No metrics API is installed.
    NotInstalled,
    /// The person may not read node metrics.
    Unreadable,
    /// The metrics API is installed but did not answer.
    Unavailable,
}

/// The nodes, with the cluster's totals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeList {
    /// Sorted by name.
    pub items: Vec<NodeView>,
    /// Totals over every node.
    pub totals: NodeTotals,
    /// How much of the pods the requests come from.
    pub pod_scan: PodScan,
    /// How many unfinished pods were read.
    pub pods_scanned: usize,
    /// Pods not yet placed on a node.
    pub unscheduled_pods: usize,
    /// Whether usage could be read.
    pub metrics: MetricsState,
    /// What could not be read.
    pub unreadable: Vec<UnreadableView>,
}

/// The cluster's own version and what it calls for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Upgrades {
    /// `cluster/upgrades`.
    pub id: String,
    /// kube-apiserver's version.
    pub kubernetes_version: String,
    /// `k3s` or `rke2`, when the version names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distribution: Option<String>,
    /// `1.34`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minor: Option<String>,
    /// That minor's support window, from engawa's bundled table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<engawa_recommend::versions::Release>,
    /// When the bundled table was copied from the Kubernetes schedule (`YYYY-MM-DD`).
    pub data_as_of: String,
    /// Every state that applies, most severe first. An open set.
    pub states: Vec<engawa_recommend::versions::UpgradeState>,
    /// The most severe state's severity.
    pub severity: engawa_recommend::versions::Severity,
    /// A newer build of the same minor, from system-upgrade-controller's Plans; absent when no
    /// Plan resolved a newer version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
    /// The next minor's build a Plan resolved (upgrade one minor at a time); absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_minor: Option<String>,
    /// Nodes whose kubelet minor differs from kube-apiserver's.
    pub skew: Vec<engawa_recommend::versions::NodeSkew>,
    /// Nodes system-upgrade-controller is upgrading now.
    pub upgrading: Vec<String>,
    /// What could not be read (nodes, Plans); the rest is still right.
    pub unreadable: Vec<UnreadableView>,
    /// What the integrations add, by module; a module whose component is not installed adds
    /// nothing.
    pub extensions: UpgradeExtensions,
}

/// What integrations add to the cluster's upgrades, one member per module.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeExtensions {
    /// system-upgrade-controller's Plans.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_upgrade_controller: Option<engawa_modules::system_upgrade_controller::Upgrades>,
}

/// Events of the whole cluster, the newest `limit` of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterEventList {
    /// Newest first (by `lastSeen`, events without one last), then by namespace and name.
    pub items: Vec<EventView>,
    /// How many events matched, also those past `limit`.
    pub total_size: usize,
    /// `true` when the cluster holds more events than engawa read (5,000): older ones may be
    /// missing.
    pub partial: bool,
}
