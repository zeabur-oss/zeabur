//! What Zeabur reads from a cluster, as one person: the port and its value types.
//!
//! Everything here is read fresh on every call. Nothing is cached between requests: a cache
//! shared between people could show one person what only another may see, and a cache per person
//! would keep answering after their rights were taken away. kube-apiserver answers each request
//! with the person's current rights instead [k8s-authz].
//!
//! [k8s-authz]: https://kubernetes.io/docs/reference/access-authn-authz/authorization/

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;

use futures_util::Stream;
use futures_util::future::BoxFuture;
use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
use k8s_openapi::api::batch::v1::{CronJob, Job};
use k8s_openapi::api::core::v1::{
    ConfigMap, Namespace, PersistentVolumeClaim, Pod, Secret, Service,
};
use k8s_openapi::api::events::v1::Event;
use k8s_openapi::api::networking::v1::Ingress;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::DynamicObject;
use serde::de::DeserializeOwned;
use zeabur_core::problem::{Denied, KubernetesStatus, Problem};

use crate::objects::{AccessRules, ResourceRef, ServedResource};

/// What a [`Cluster`] call answers: a boxed future, so the port can be a trait object
/// (`dyn Cluster`) and one router serves every implementation [boxfuture].
///
/// [boxfuture]: https://docs.rs/futures-util/latest/futures_util/future/type.BoxFuture.html
pub type Answer<'a, T> = BoxFuture<'a, Result<T, ClusterError>>;

/// Lines of a followed log, as they arrive.
pub type LogLines = Pin<Box<dyn Stream<Item = Result<String, ClusterError>> + Send>>;

/// A command run in a container ([`Cluster::exec`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecRequest {
    /// The container.
    pub container: String,
    /// The command and its arguments, run directly (no shell around it).
    pub command: Vec<String>,
}

/// The size of a terminal, in character cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    /// Columns.
    pub cols: u16,
    /// Rows.
    pub rows: u16,
}

/// How a process run by [`Cluster::exec`] ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitStatus {
    /// It exited with this code (`0` for success).
    Exited(i32),
    /// The command is not in the image, as in a distroless image, which has no shell.
    CommandNotFound,
    /// The container runtime or the connection ended it, for this reason.
    Failed(String),
}

/// Changes the size of an [`ExecSession`]'s terminal.
pub type Resize = Box<dyn FnMut(TerminalSize) + Send>;

/// A process started by [`Cluster::exec`]. Dropping it ends the stream to the container.
pub struct ExecSession {
    /// The process's standard input.
    pub input: Pin<Box<dyn tokio::io::AsyncWrite + Send>>,
    /// What the terminal shows: standard output and standard error, merged by the TTY. It ends
    /// once the process has ended.
    pub output: Pin<Box<dyn tokio::io::AsyncRead + Send>>,
    /// Resizes the terminal.
    pub resize: Resize,
    /// Resolves when the process has ended.
    pub exit: Pin<Box<dyn Future<Output = ExitStatus> + Send>>,
}

impl std::fmt::Debug for ExecSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecSession").finish_non_exhaustive()
    }
}

/// Reads from one cluster as one person. Implemented by [`crate::UserClient`] (kube-apiserver)
/// and `memory::MemoryCluster` (tests, behind the `memory` feature).
///
/// Every method returns an [`Answer`], so callers hold a `dyn Cluster`; the typed reads built on it
/// are in [`ClusterExt`].
pub trait Cluster: Send + Sync {
    /// Who the cluster says this person is.
    fn whoami(&self) -> Answer<'_, Identity>;

    /// The namespaces this person may list.
    fn namespaces(&self) -> Answer<'_, Vec<Namespace>>;

    /// Every object Zeabur projects in `namespace`. Kinds this person may not list are reported
    /// in [`NamespaceObjects::unreadable`]; if none can be listed, the call fails.
    fn namespace_objects<'a>(&'a self, namespace: &'a str) -> Answer<'a, NamespaceObjects>;

    /// The workloads and pods of `namespace` only: what a project's health needs, in six LISTs
    /// instead of twelve. The other kinds are left empty and are not reported as unreadable; if
    /// none of these six can be listed, the call fails.
    fn namespace_workloads<'a>(&'a self, namespace: &'a str) -> Answer<'a, NamespaceObjects>;

    /// kube-apiserver's `gitVersion` (`/version`, readable by every authenticated person through
    /// the default `system:public-info-viewer` role).
    fn version(&self) -> Answer<'_, String>;

    /// What the person calls this cluster: in local mode the kubeconfig's name for the cluster
    /// the context uses, otherwise (and when the kubeconfig cannot be read for it) the host of
    /// kube-apiserver's address. Kubernetes itself gives a cluster no name.
    fn name(&self) -> String;

    /// A snapshot of a container's log.
    fn pod_logs<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        options: &'a LogOptions,
    ) -> Answer<'a, String>;

    /// A container's log as it is written, until the stream is dropped.
    fn follow_pod_logs<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        options: &'a LogOptions,
    ) -> Answer<'a, LogLines>;

    /// Runs a command in a container with a terminal (`pods/exec` over a WebSocket), as `kubectl
    /// exec -it` does: standard input, a TTY (which merges standard error into the output) and
    /// its size. The process's stream ends when it exits or the returned [`ExecSession`] is
    /// dropped.
    fn exec<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        request: &'a ExecRequest,
    ) -> Answer<'a, ExecSession>;

    /// A Secret's values. Only on an explicit request from the person; never cached or logged.
    fn reveal_secret<'a>(
        &'a self,
        namespace: &'a str,
        name: &'a str,
    ) -> Answer<'a, BTreeMap<String, Vec<u8>>>;

    /// Cluster-wide facts used to detect installed capabilities. Probes this person may not run
    /// are left empty and listed in [`ClusterFacts::unreadable`].
    fn cluster_facts(&self) -> Answer<'_, ClusterFacts>;

    /// The pods of every namespace that are not finished (`status.phase` neither `Succeeded` nor
    /// `Failed`, the selection `kubectl describe node` makes), in pages of 500, stopping after
    /// `limit` pods. Decoded into [`ClusterPod`], only what resource sums need, so a large
    /// cluster costs little memory.
    fn cluster_pods(&self, limit: usize) -> Answer<'_, ClusterPods>;

    /// Events of every namespace, in pages of 500, stopping after `limit`.
    fn cluster_events(&self, limit: usize) -> Answer<'_, ClusterEvents>;

    /// Whether kube-apiserver serves `group_version` (`apps/v1`, `v1`): `Some(false)` when it does
    /// not (not installed), `None` when discovery itself is refused. See [`crate::objects`].
    fn serves<'a>(&'a self, group_version: &'a str) -> Answer<'a, Option<bool>>;

    /// Every resource the cluster serves, in its preferred version, from discovery.
    fn served_resources(&self) -> Answer<'_, Vec<ServedResource>>;

    /// The objects of `resource` in `namespace` (every namespace when `None`); `None` when its
    /// API is not installed. A Secret's values are hidden ([`crate::objects`]).
    fn list_objects<'a>(
        &'a self,
        resource: &'a ResourceRef,
        namespace: Option<&'a str>,
    ) -> Answer<'a, Option<Vec<DynamicObject>>>;

    /// Only the metadata of the objects of `resource` in `namespace` (every namespace when
    /// `None`), as `kubectl get` reads it for a table; `None` when its API is not installed.
    fn list_metadata<'a>(
        &'a self,
        resource: &'a ResourceRef,
        namespace: Option<&'a str>,
    ) -> Answer<'a, Option<Vec<ObjectMeta>>>;

    /// [`Cluster::list_metadata`] for a resource discovery has just named: no discovery check,
    /// and answered from kube-apiserver's watch cache when it can be (possibly slightly behind).
    /// See [`crate::objects`].
    fn list_served_metadata<'a>(
        &'a self,
        resource: &'a ResourceRef,
        namespace: &'a str,
    ) -> Answer<'a, Option<Vec<ObjectMeta>>>;

    /// What the person may do in `namespace`, from one `SelfSubjectRulesReview`: a hint for
    /// which kinds a listing would be refused. See [`crate::objects`].
    fn access_rules<'a>(&'a self, namespace: &'a str) -> Answer<'a, AccessRules>;

    /// Whether kube-apiserver's authorizers allow the person `request`, from one
    /// `SelfSubjectAccessReview` [k8s-authz]. Admission is not asked: a request this allows can
    /// still be refused by a quota or a policy, which is how a `403` from admission is told from
    /// one for missing rights.
    ///
    /// [k8s-authz]: https://kubernetes.io/docs/reference/access-authn-authz/authorization/#checking-api-access
    fn review_access<'a>(&'a self, request: &'a Denied) -> Answer<'a, bool>;

    /// The objects of a built-in kind, which kube-apiserver always serves, in `namespace` (every
    /// namespace, or a cluster-scoped kind, when `None`): no discovery check first, and a refusal
    /// is an error. A Secret's values are hidden ([`crate::objects`]). Read it typed with
    /// [`ClusterExt::list`].
    fn list_kind<'a>(
        &'a self,
        resource: &'a ResourceRef,
        namespace: Option<&'a str>,
    ) -> Answer<'a, Vec<DynamicObject>>;

    /// One object of `resource`; `None` when its API is not installed, a `404` error when the
    /// object does not exist ([`ClusterExt::find_object`] makes that `None` too). A Secret's
    /// values are hidden ([`crate::objects`]). `resource.kind` may be empty.
    fn get_object<'a>(
        &'a self,
        resource: &'a ResourceRef,
        namespace: Option<&'a str>,
        name: &'a str,
    ) -> Answer<'a, Option<DynamicObject>>;

    // ── Writes ───────────────────────────────────────────────────────────────────────────────
    // Each write is one request as the person; kube-apiserver authorises it with their rights.

    /// Sends one change to one object (see [`WriteRequest`]) and returns its new
    /// `resourceVersion`.
    fn write<'a>(&'a self, request: &'a WriteRequest) -> Answer<'a, Written>;

    // ── Reads through the service proxy ──────────────────────────────────────────────────────

    /// `GET path_and_query` of `service` through kube-apiserver's service proxy
    /// (`/api/v1/namespaces/{namespace}/services/{name}:{port}/proxy/…`) [k8s-proxy], as the
    /// person: the right it needs is `get` on `services/proxy` for that Service. The answer must
    /// be JSON.
    ///
    /// kube-apiserver does not pass the person's identity on: it removes `Authorization` and the
    /// impersonation headers before proxying [k8s-authn-filter], so the Service sees a request
    /// without a user and cannot tell people apart. `path_and_query` is Zeabur's own (it starts
    /// with `/` and is already encoded), never a person's.
    ///
    /// [k8s-proxy]: https://kubernetes.io/docs/tasks/access-application-cluster/access-cluster-services/#manually-constructing-apiserver-proxy-urls
    /// [k8s-authn-filter]: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/apiserver/pkg/endpoints/filters/authentication.go
    fn proxy_get<'a>(
        &'a self,
        service: &'a ServicePort,
        path_and_query: &'a str,
    ) -> Answer<'a, serde_json::Value>;
}

/// The typed reads built on [`Cluster`], for every implementation and for `dyn Cluster`.
pub trait ClusterExt: Cluster {
    /// Every object of the built-in kind `K` in `namespace` (every namespace, or a
    /// cluster-scoped kind, when `None`), as [`Cluster::list_kind`] reads them.
    fn list<K>(
        &self,
        namespace: Option<&str>,
    ) -> impl Future<Output = Result<Vec<K>, ClusterError>> + Send
    where
        K: k8s_openapi::Resource + DeserializeOwned,
    {
        let resource = ResourceRef::of::<K>();
        async move {
            self.list_kind(&resource, namespace)
                .await?
                .into_iter()
                .map(|object| {
                    serde_json::to_value(object)
                        .and_then(serde_json::from_value)
                        .map_err(|e| ClusterError::Decode(e.to_string()))
                })
                .collect()
        }
    }

    /// [`Cluster::get_object`], with an object that does not exist as `None`.
    fn find_object(
        &self,
        resource: &ResourceRef,
        namespace: Option<&str>,
        name: &str,
    ) -> impl Future<Output = Result<Option<DynamicObject>, ClusterError>> + Send {
        async move {
            match self.get_object(resource, namespace, name).await {
                Err(error) if error.is_not_found() => Ok(None),
                other => other,
            }
        }
    }

    /// The Gateway API `Gateway` objects of every namespace, which a new `HTTPRoute` can attach
    /// to, sorted; empty when the API is not installed, `None` when the person may not list
    /// them.
    fn gateways(
        &self,
    ) -> impl Future<Output = Result<Option<Vec<GatewayRef>>, ClusterError>> + Send {
        async move {
            let resource =
                ResourceRef::new("gateway.networking.k8s.io", "v1", "gateways", "Gateway");
            match self.list_metadata(&resource, None).await {
                Ok(listed) => {
                    let mut gateways: Vec<GatewayRef> = listed
                        .unwrap_or_default()
                        .into_iter()
                        .filter_map(|metadata| {
                            Some(GatewayRef {
                                namespace: metadata.namespace?,
                                name: metadata.name?,
                            })
                        })
                        .collect();
                    gateways.sort();
                    Ok(Some(gateways))
                }
                Err(error) if error.is_refused() => Ok(None),
                Err(error) => Err(error),
            }
        }
    }
}

impl<C: Cluster + ?Sized> ClusterExt for C {}

/// A port of a Service, as the service proxy names it: `namespace/name:port`, the port by number
/// or by name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ServicePort {
    /// The Service's namespace.
    pub namespace: String,
    /// The Service's name.
    pub name: String,
    /// The port's number or name.
    pub port: String,
}

impl std::fmt::Display for ServicePort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}:{}", self.namespace, self.name, self.port)
    }
}

/// The pods of the whole cluster, as far as they were read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClusterPods {
    /// The pods read.
    pub pods: Vec<ClusterPod>,
    /// `true` when the limit stopped the read before the last page.
    pub truncated: bool,
}

/// A pod, reduced to what node capacity needs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClusterPod {
    /// Its namespace.
    pub namespace: String,
    /// Its name.
    pub name: String,
    /// The node it is bound to (`spec.nodeName`); `None` while it waits to be scheduled.
    pub node_name: Option<String>,
    /// `status.phase`.
    pub phase: Option<String>,
    /// Its regular containers.
    pub containers: Vec<ContainerResources>,
    /// Its init containers, in order; sidecars are marked.
    pub init_containers: Vec<ContainerResources>,
    /// `spec.overhead` (the `RuntimeClass`'s fixed cost per pod), as quantities.
    pub overhead: BTreeMap<String, String>,
    /// Pod-level `spec.resources`, which replace the containers' sum for the resources they name.
    pub pod_resources: Option<ContainerResources>,
}

/// The requests and limits of one container, as Kubernetes quantities (`250m`, `512Mi`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerResources {
    /// `resources.requests`.
    pub requests: BTreeMap<String, String>,
    /// `resources.limits`.
    pub limits: BTreeMap<String, String>,
    /// An init container with `restartPolicy: Always`: a sidecar, which keeps running beside the
    /// regular containers.
    pub sidecar: bool,
}

/// Events of the whole cluster, as far as they were read.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ClusterEvents {
    /// The events read, in no particular order.
    pub events: Vec<Event>,
    /// `true` when the limit stopped the read before the last page.
    pub truncated: bool,
}

// ── Writes ───────────────────────────────────────────────────────────────────────────────────

/// The field manager every Zeabur write names (`fieldManager`), so `managedFields` tells its
/// edits apart [k8s-field-manager].
///
/// [k8s-field-manager]: https://kubernetes.io/docs/reference/using-api/server-side-apply/#field-management
pub use zeabur_core::names::FIELD_MANAGER;

/// The object a write addresses.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectTarget {
    /// `apiVersion`, such as `apps/v1` or `v1`.
    pub api_version: String,
    /// The kind, such as `Deployment`.
    pub kind: String,
    /// The resource (plural), such as `deployments`.
    pub plural: String,
    /// The namespace; empty for a cluster-scoped object (a Namespace).
    pub namespace: String,
    /// The name.
    pub name: String,
}

impl ObjectTarget {
    /// The resource of the object, to read it with [`Cluster::get_object`].
    #[must_use]
    pub fn resource(&self) -> ResourceRef {
        let (group, version) = self
            .api_version
            .split_once('/')
            .unwrap_or(("", self.api_version.as_str()));
        ResourceRef::new(group, version, &self.plural, &self.kind)
    }

    /// The namespace to read the object in; `None` for a cluster-scoped object.
    #[must_use]
    pub fn namespace(&self) -> Option<&str> {
        (!self.namespace.is_empty()).then_some(self.namespace.as_str())
    }
}

/// How a write changes its object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteBody {
    /// A new object, `apiVersion`, `kind` and `metadata` included, created with `POST` to its
    /// collection; refused (`409 AlreadyExists`) when one of that name exists.
    Create(serde_json::Value),
    /// An RFC 6902 JSON Patch, an array of operations [rfc6902]; its `test` operations make it
    /// all-or-nothing against what was read.
    ///
    /// [rfc6902]: https://www.rfc-editor.org/rfc/rfc6902
    JsonPatch(serde_json::Value),
    /// A deletion, `DELETE` of the object with background propagation and these
    /// [`Preconditions`]; refused (`409 Conflict`) when the object no longer meets them.
    Delete(Preconditions),
}

/// What a deletion requires of the object, as `DeleteOptions.preconditions` names it
/// [k8s-delete-options]: kube-apiserver refuses the deletion with `409 Conflict` when one does
/// not hold.
///
/// [k8s-delete-options]: https://kubernetes.io/docs/reference/kubernetes-api/common-definitions/delete-options/
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Preconditions {
    /// The object's `metadata.uid`: the object read, not another of the same name since.
    pub uid: Option<String>,
    /// The object's `metadata.resourceVersion`: unchanged since it was read.
    pub resource_version: Option<String>,
}

/// One write to one object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteRequest {
    /// The object.
    pub target: ObjectTarget,
    /// The change.
    pub body: WriteBody,
    /// Validate and run admission without persisting (`dryRun=All`) [k8s-dry-run].
    ///
    /// [k8s-dry-run]: https://kubernetes.io/docs/reference/using-api/api-concepts/#dry-run
    pub dry_run: bool,
}

/// What a write left behind.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Written {
    /// The object's `resourceVersion` after the write.
    pub resource_version: Option<String>,
}

pub use zeabur_core::names::GatewayRef;

/// The person, as the cluster sees them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Identity {
    /// User name.
    pub username: String,
    /// Groups.
    pub groups: Vec<String>,
}

/// Which part of a container's log to read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LogOptions {
    /// The container; required when the pod has several.
    pub container: Option<String>,
    /// Only the last lines.
    pub tail_lines: Option<i64>,
    /// Only lines newer than this many seconds.
    pub since_seconds: Option<i64>,
    /// The previous instance of the container (after a restart).
    pub previous: bool,
    /// Prefix each line with its RFC 3339 timestamp.
    pub timestamps: bool,
}

/// A Secret without its values: what the projection may know about it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SecretSummary {
    /// The Secret, with `data` and `string_data` always `None`.
    pub secret: Secret,
    /// How many keys it holds.
    pub key_count: usize,
}

/// The objects of one namespace that the projection reads.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NamespaceObjects {
    /// The namespace.
    pub namespace: String,
    /// Deployments.
    pub deployments: Vec<Deployment>,
    /// StatefulSets.
    pub stateful_sets: Vec<StatefulSet>,
    /// DaemonSets.
    pub daemon_sets: Vec<DaemonSet>,
    /// Jobs.
    pub jobs: Vec<Job>,
    /// CronJobs.
    pub cron_jobs: Vec<CronJob>,
    /// Pods.
    pub pods: Vec<Pod>,
    /// Services.
    pub services: Vec<Service>,
    /// ConfigMaps, in full.
    pub config_maps: Vec<ConfigMap>,
    /// Secrets, without values.
    pub secrets: Vec<SecretSummary>,
    /// PersistentVolumeClaims.
    pub persistent_volume_claims: Vec<PersistentVolumeClaim>,
    /// Ingresses.
    pub ingresses: Vec<Ingress>,
    /// Gateway API HTTPRoutes, as JSON (no typed bindings ship with k8s-openapi); empty when
    /// the CRD is not installed.
    pub http_routes: Vec<serde_json::Value>,
    /// Kinds that could not be listed, with kube-apiserver's answer.
    pub unreadable: Vec<Unreadable>,
}

/// A kind the person could not list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable {
    /// The resource, as `group/resource` (`deployments.apps`, `secrets`).
    pub resource: String,
    /// kube-apiserver's answer.
    pub status: KubernetesStatus,
}

/// Facts about the whole cluster, for capability detection.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClusterFacts {
    /// kube-apiserver's `gitVersion`.
    pub version: String,
    /// How many nodes there are (`None` when nodes cannot be listed).
    pub nodes: Option<usize>,
    /// Names of installed CustomResourceDefinitions.
    pub crds: BTreeSet<String>,
    /// Names of APIServices (`v1beta1.metrics.k8s.io`, …).
    pub api_services: BTreeSet<String>,
    /// IngressClass controller names.
    pub ingress_controllers: BTreeSet<String>,
    /// GatewayClass controller names.
    pub gateway_controllers: BTreeSet<String>,
    /// Helm releases as `namespace/release`, from the `name` label of release Secrets
    /// (`owner=helm`). That label is the *release* name; the chart's name is only inside the
    /// encoded release record, which Zeabur never decodes, so it is not known here [helm-secrets].
    ///
    /// [helm-secrets]: https://github.com/helm/helm/blob/main/pkg/storage/driver/secrets.go
    pub helm_releases: BTreeSet<String>,
    /// k3s `HelmChart` objects (`kube-system/traefik`, …), as `namespace/name` → chart.
    pub k3s_helm_charts: BTreeMap<String, String>,
    /// The `spec.version` of those `HelmChart` objects that name one, as `namespace/name` →
    /// version.
    pub k3s_helm_chart_versions: BTreeMap<String, String>,
    /// Labels of every Deployment, StatefulSet and DaemonSet the person can list, for detection
    /// by workload labels.
    pub workload_labels: Vec<(String, BTreeMap<String, String>)>,
    /// Probes that could not run.
    pub unreadable: Vec<Unreadable>,
}

/// Why a read failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClusterError {
    /// kube-apiserver answered with an error status.
    #[error("kube-apiserver: {0:?}")]
    Status(KubernetesStatus),
    /// kube-apiserver could not be reached, or the connection failed.
    #[error("cannot reach kube-apiserver: {0}")]
    Transport(String),
    /// The answer could not be understood.
    #[error("unexpected answer from kube-apiserver: {0}")]
    Decode(String),
    /// A namespace or object name no object can have, refused before it reached
    /// kube-apiserver (a namespace that is not a DNS-1123 label, or a name a URL path cannot
    /// carry as it is); it reads as not found.
    #[error("no such object: {0}")]
    InvalidName(String),
}

impl ClusterError {
    /// The problem to show the person, about `what`.
    #[must_use]
    pub fn into_problem(self, what: &str) -> Problem {
        match self {
            Self::Status(status) => Problem::from_kubernetes(status, what),
            Self::Transport(message) => Problem::new(
                zeabur_core::problem::Kind::Unavailable,
                format!("Zeabur cannot reach the cluster's API server ({message})."),
            ),
            Self::Decode(message) => Problem::new(
                zeabur_core::problem::Kind::Internal,
                format!("The cluster's answer about {what} could not be read ({message})."),
            ),
            Self::InvalidName(message) => Problem::new(
                zeabur_core::problem::Kind::NotFound,
                format!("There is no {what}: {message}."),
            ),
        }
    }

    /// Whether kube-apiserver refused the person (see [`KubernetesStatus::is_refusal`]).
    #[must_use]
    pub const fn is_refused(&self) -> bool {
        matches!(self, Self::Status(status) if status.is_refusal())
    }

    /// Whether kube-apiserver answered `404` (see [`KubernetesStatus::is_not_found`]); a name
    /// refused before it was sent is not this.
    #[must_use]
    pub const fn is_not_found(&self) -> bool {
        matches!(self, Self::Status(status) if status.is_not_found())
    }

    /// A refusal of `resource` (`group/resource`, as [`Unreadable::resource`] names it) as what
    /// could not be read; any other error as it is.
    ///
    /// # Errors
    ///
    /// `self`, when it is not a refusal.
    pub fn into_unreadable(self, resource: &str) -> Result<Unreadable, Self> {
        match self {
            Self::Status(status) if status.is_refusal() => Ok(Unreadable {
                resource: resource.to_owned(),
                status,
            }),
            other @ (Self::Status(_)
            | Self::Transport(_)
            | Self::Decode(_)
            | Self::InvalidName(_)) => Err(other),
        }
    }

    /// The HTTP status code, when kube-apiserver answered; `404` for a name no object can
    /// have, as kube-apiserver answers a read of a name that does not exist.
    #[must_use]
    pub const fn code(&self) -> Option<u16> {
        match self {
            Self::Status(status) => Some(status.code),
            Self::InvalidName(_) => Some(404),
            Self::Transport(_) | Self::Decode(_) => None,
        }
    }
}
