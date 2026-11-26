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
//! ([`zeabur_model::id`]), and times are absolute RFC 3339 strings, never relative ("3m ago"),
//! so that a client can compare and sort them.
//!
//! [aip-131]: https://google.aip.dev/131
//! [aip-132]: https://google.aip.dev/132

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use zeabur_core::problem::KubernetesStatus;
use zeabur_kube::Unreadable;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_model::{
    Endpoint, Health, Origin, Project, ProjectDetails, ProjectHealth, Route, Service, WorkloadKind,
};

use crate::projects::namespace::Permission;

/// A kind the person could not read, and kube-apiserver's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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

/// An object of another product that Zeabur read but does not know the shape of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UndecodedView {
    /// The object.
    pub origin: Origin,
    /// Why it does not decode.
    pub error: String,
}

/// Who the cluster says the person is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WhoAmI {
    /// User name.
    pub username: String,
    /// Groups.
    pub groups: Vec<String>,
    /// Signed in with Zeabur Identity, whose roles the interface can name.
    pub member: bool,
    /// In Zeabur Identity's admins group.
    pub admin: bool,
}

/// The cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Cluster {
    /// What to call the cluster, never kube-apiserver's address (see
    /// `crate::settings::cluster_name`).
    pub name: String,
    /// Where `name` comes from.
    pub name_source: crate::settings::cluster_name::NameSource,
    /// kube-apiserver's version (`gitVersion`, such as `v1.36.5+k3s1`).
    pub kubernetes_version: String,
    /// The distribution, when the version names one: `k3s`, `rke2` or `kubesolo`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distribution: Option<String>,
    /// How a new service's public URL is made.
    pub domains: crate::settings::DomainSettings,
    /// Where a service deployed from Git keeps its images.
    pub builds: crate::settings::BuildSettings,
    /// What you may change in the cluster itself.
    pub can: ClusterRights,
}

/// What the person may change in the cluster itself, one cluster-wide `SelfSubjectAccessReview`
/// per action. A binding in one namespace, even of `cluster-admin`, allows none of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterRights {
    /// Change nodes (labels, taints, cordon): `patch` on nodes.
    pub change_nodes: Permission,
    /// Set up Velero backups: `create` on `schedules.velero.io` in every namespace. The backups
    /// page asks again in Velero's own namespace.
    pub change_backups: Permission,
}

impl ClusterRights {
    /// Asks `cluster` for each right.
    pub async fn read(cluster: &dyn zeabur_kube::Cluster) -> Self {
        let review = |verb: &str, group: &str, resource: &str| zeabur_core::problem::Denied {
            verb: verb.to_owned(),
            group: group.to_owned(),
            resource: resource.to_owned(),
            name: None,
            namespace: None,
        };
        let nodes = review("patch", "", "nodes");
        let schedules = review("create", "velero.io", "schedules");
        let (nodes, schedules) = futures_util::join!(
            cluster.review_access(&nodes),
            cluster.review_access(&schedules)
        );
        Self {
            change_nodes: nodes.into(),
            change_backups: schedules.into(),
        }
    }
}

/// Zeabur itself: its version, and what the interface needs to say how to upgrade it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ZeaburVersion {
    /// Zeabur's version, such as `0.1.1`; in local mode, `zeaburctl`'s.
    pub version: String,
    /// Whether the interface compares it with GitHub's releases (`GET /cluster/zeabur/update`).
    pub update_check: bool,
    /// How Zeabur was installed; absent when the person may not read what tells.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_by: Option<InstallMethod>,
}

/// Whether a newer Zeabur than [`ZeaburVersion::version`] is out, from GitHub's releases as the
/// server read them last.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ZeaburUpdate {
    /// Absent when none is out, or while the update check is off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub newer: Option<ZeaburNewer>,
}

/// A newer release than the running one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ZeaburNewer {
    /// The newest normal release, or the newest pre-release when the running version is one.
    pub latest: ZeaburRelease,
    /// The newer normal releases that fix vulnerabilities, newest first: the running version has
    /// them.
    pub security: Vec<ZeaburRelease>,
    /// Red while a release fixes vulnerabilities the running version has, else amber.
    pub health: Health,
}

/// A published release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ZeaburRelease {
    /// The tag without its `v`, such as `0.1.2`.
    pub version: String,
    /// Its page on GitHub, with the release notes.
    pub url: String,
    /// GitHub marks it a pre-release.
    pub prerelease: bool,
    /// Its notes have a `### Security` section: it fixes vulnerabilities.
    pub security: bool,
}

/// How Zeabur was installed, which says how to upgrade it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum InstallMethod {
    /// A Helm release.
    Helm,
    /// An Argo CD Application.
    ArgoCd,
    /// A Flux `Kustomization` or `HelmRelease`.
    Flux,
    /// `zeaburctl ui` on a person's computer.
    Zeaburctl,
}

/// The API server's readiness, from the replica that answered (see `crate::cluster::health`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterHealth {
    /// `cluster/health`.
    pub id: String,
    /// Every check passed.
    pub ready: bool,
    /// How many checks ran.
    pub checks: usize,
    /// The checks that failed, in the API server's order.
    pub failing: Vec<FailingCheck>,
    /// The leader leases of kube-scheduler and kube-controller-manager that exist and may be
    /// read, in that order.
    pub leaders: Vec<LeaderLease>,
}

/// A readiness check that failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FailingCheck {
    /// The check's name (`etcd`, `poststarthook/crd-informer-synced`).
    pub name: String,
    /// What the API server gave as the reason; it withholds most (`reason withheld`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// A component's leader lease in `kube-system`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LeaderLease {
    /// The Lease.
    pub origin: Origin,
    /// `kube-scheduler` or `kube-controller-manager`.
    pub component: String,
    /// The leading replica, as it names itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holder: Option<String>,
    /// When the leader last renewed the lease.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renewed_at: Option<Timestamp>,
    /// How long the lease holds without a renewal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<i32>,
    /// Renewed within its duration: a replica is leading.
    pub current: bool,
}

/// A project in the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    /// `projects/<name>`.
    pub id: String,
    /// The namespace.
    pub origin: Origin,
    /// The project's name: its namespace's name.
    pub name: String,
    /// Whether the namespace carries Zeabur's project label. Any namespace can be opened; adopting
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
    /// Its namespace is being deleted (`Terminating`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deleting: bool,
}

/// One service of a project in the list: which it is, how healthy and why, without its
/// containers, variables or configuration (those are in the project document).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectList {
    /// Adopted projects first, then the other namespaces, each sorted by name.
    pub items: Vec<ProjectSummary>,
    /// Whether the person may create a project (`create` on `namespaces`, cluster-wide).
    pub can_create: Permission,
}

/// A project: its services, the resources they use and how they relate, read as the person.
///
/// The one document a client needs for a project: the service, variables and objects views are
/// cut from it rather than read again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDocument {
    /// `projects/<name>`.
    pub id: String,
    /// How many of its services are in each state, and the worst.
    pub health: ProjectHealth,
    /// The projection.
    #[serde(flatten)]
    pub project: Project,
    /// Kinds the person could not list; the projection is missing what they hide.
    pub unreadable: Vec<UnreadableView>,
    /// What the integrations add, by module; a module whose API is not served adds nothing.
    pub extensions: ProjectExtensions,
}

/// What integrations add to a project document, one member per module.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectExtensions {
    /// Who may reach each service, from the project's NetworkPolicy objects. Whether the
    /// cluster's network plugin enforces them is not known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_policy: Option<zeabur_modules::network_policy::Reachability>,
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
    /// When it last happened.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen_at: Option<Timestamp>,
    /// The component that reported it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reporter: Option<String>,
    /// What a `Warning` says went wrong; absent for a `Normal` event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<WarningKind>,
}

/// What a `Warning` event says went wrong, from its reason. An open set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum WarningKind {
    /// A container keeps crashing.
    CrashLoop,
    /// An image cannot be pulled.
    ImagePull,
    /// A pod waits for a node.
    Unschedulable,
    /// Anything else.
    Other,
}

impl WarningKind {
    /// The kind of a warning with `reason` and `note`. The kubelet gives `BackOff` to a pull and
    /// to a restart alike, so its note tells which.
    #[must_use]
    pub fn of(reason: &str, note: &str) -> Self {
        match reason {
            "BackOff" if note.to_ascii_lowercase().contains("image") => Self::ImagePull,
            "BackOff" | "CrashLoopBackOff" => Self::CrashLoop,
            "Failed" | "ErrImagePull" | "ImagePullBackOff" | "InspectFailed" => Self::ImagePull,
            "FailedScheduling" => Self::Unschedulable,
            _ => Self::Other,
        }
    }
}

/// Events of a project, the newest `limit` of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventList {
    /// Newest first (by `lastSeenAt`, events without one last), then by name.
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SecretValue {
    /// The value: text as is, binary data as base64.
    pub value: String,
    /// `utf-8` or `base64`.
    pub encoding: String,
}

/// A Secret's values, shown because the person asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RevealedSecret {
    /// The Secret.
    pub origin: Origin,
    /// Its values by key.
    pub values: BTreeMap<String, SecretValue>,
}

/// Which capabilities the cluster has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityList {
    /// Zeabur Catalog's commit.
    pub catalogue_commit: String,
    /// Each capability, sorted by name: existing (with evidence) or not (with what to install).
    pub items: Vec<zeabur_catalog::Capability>,
    /// How many capabilities exist.
    pub installed: usize,
    /// How many missing capabilities the catalogue recommends.
    pub recommended: usize,
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
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
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
    /// The pods placed here that maintenance would move (all but a DaemonSet's, static pods'
    /// mirrors, finished pods and those in `pinnedPods`); absent when pods cannot be listed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub movable_pods: Option<u64>,
    /// How many of those are being deleted; absent when pods cannot be listed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leaving_pods: Option<u64>,
    /// Pods maintenance leaves here because a volume they use is only on this node, not counted
    /// in `movablePods`; 0 as well when PersistentVolumes cannot be listed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned_pods: Option<u64>,
    /// What it uses now, from metrics-server (approximate, for autoscaling); absent without it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Resources>,
    /// The kubelet's filesystem (`nodefs`), where pods' logs, writable layers and `emptyDir`
    /// volumes live, from the metrics store; absent when the store has no value for this node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk: Option<DiskUsage>,
    /// When it joined the cluster.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<Timestamp>,
    /// What is wrong with it, worst first.
    pub issues: Vec<crate::cluster::node_health::NodeIssue>,
}

/// A filesystem's size and use.
#[expect(
    clippy::struct_field_names,
    reason = "the API names every byte count with its unit"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiskUsage {
    /// Its size.
    pub capacity_bytes: u64,
    /// In use.
    pub used_bytes: u64,
    /// Free to use; can be less than size less use where the filesystem keeps some back.
    pub available_bytes: u64,
}

/// Whether disk use could be read from the metrics store. An open set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DiskState {
    /// The store answered with values.
    Available,
    /// There is no store at the configured Service (`404`).
    NotConfigured,
    /// The person may not reach the store (`get` on its `services/proxy`).
    Unreadable,
    /// The store did not answer, or not as the Prometheus HTTP API.
    Unavailable,
    /// The store answered but holds none of the series asked for: it does not scrape them, or
    /// not yet.
    NotCollected,
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
    /// The nodes' root filesystems together; absent when none could be read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk: Option<DiskUsage>,
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

impl PodScan {
    /// How much of the pods `pods` holds; `None` when they could not be listed.
    pub(crate) const fn of(pods: Option<&zeabur_kube::ClusterPods>) -> Self {
        match pods {
            None => Self::Unreadable,
            Some(p) if p.truncated => Self::Partial,
            Some(_) => Self::Complete,
        }
    }
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
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
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
    /// Whether the nodes' disks could be read.
    pub disk: DiskState,
    /// What could not be read.
    pub unreadable: Vec<UnreadableView>,
    /// The thresholds the nodes are judged by.
    pub thresholds: crate::cluster::node_health::NodeThresholds,
    /// Each kind of issue over every node that has it, worst first.
    pub attention: Vec<crate::cluster::node_health::NodeIssueGroup>,
    /// The nodes' names, worst first.
    pub worst_first: Vec<String>,
    /// The node the totals would hide: the worst one, when there is more than one node and its
    /// worst issue takes services down.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worst: Option<String>,
}

/// The cluster's own version and what it calls for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Upgrades {
    /// `cluster/upgrades`.
    pub id: String,
    /// kube-apiserver's version.
    pub kubernetes_version: String,
    /// `k3s`, `rke2` or `kubesolo`, when the version names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distribution: Option<String>,
    /// `1.34`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minor: Option<String>,
    /// That minor's support window, from Zeabur's bundled table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<zeabur_catalog::versions::Release>,
    /// When the bundled table was copied from the Kubernetes schedule.
    pub data_copied_at: zeabur_kube::k8s_openapi::jiff::civil::Date,
    /// Every state that applies, most severe first. An open set.
    pub states: Vec<zeabur_catalog::versions::UpgradeState>,
    /// The one state the cluster's header and Home show: the most severe that shows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<zeabur_catalog::versions::UpgradeFinding>,
    /// The states someone has to act on, red and amber, most severe first.
    pub attention: Vec<zeabur_catalog::versions::UpgradeFinding>,
    /// A newer build of the same minor, from system-upgrade-controller's Plans; absent when no
    /// Plan resolved a newer version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
    /// The next minor's build a Plan resolved (upgrade one minor at a time); absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_minor: Option<String>,
    /// Nodes whose kubelet minor differs from kube-apiserver's.
    pub skew: Vec<zeabur_catalog::versions::NodeSkew>,
    /// Nodes system-upgrade-controller is upgrading now.
    pub upgrading: Vec<String>,
    /// What could not be read (nodes, Plans); the rest is still right.
    pub unreadable: Vec<UnreadableView>,
    /// What the integrations add, by module; a module whose component is not installed adds
    /// nothing.
    pub extensions: UpgradeExtensions,
}

/// What integrations add to the cluster's upgrades, one member per module.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeExtensions {
    /// system-upgrade-controller's Plans.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_upgrade_controller: Option<zeabur_modules::system_upgrade_controller::Upgrades>,
}

/// Events of the whole cluster, the newest `limit` of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterEventList {
    /// Newest first (by `lastSeenAt`, events without one last), then by namespace and name.
    pub items: Vec<EventView>,
    /// How many events matched, also those past `limit`.
    pub total_size: usize,
    /// `true` when the cluster holds more events than Zeabur read (5,000): older ones may be
    /// missing.
    pub partial: bool,
}

#[cfg(test)]
mod tests {
    use super::WarningKind;

    #[test]
    fn a_warning_folds_by_its_reason_and_a_back_off_by_its_note() {
        let pull = "Back-off pulling image \"x\"";
        assert_eq!(WarningKind::of("BackOff", pull), WarningKind::ImagePull);
        let restart = "Back-off restarting failed container";
        assert_eq!(WarningKind::of("BackOff", restart), WarningKind::CrashLoop);
        assert_eq!(WarningKind::of("ErrImagePull", ""), WarningKind::ImagePull);
        assert_eq!(
            WarningKind::of("FailedScheduling", ""),
            WarningKind::Unschedulable
        );
        assert_eq!(WarningKind::of("FailedMount", ""), WarningKind::Other);
    }
}
