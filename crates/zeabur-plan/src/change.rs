//! What a person asks to change, and what a plan says about it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeabur_model::{ProjectIcon, WorkloadRef};

use crate::checks::{ProbeRequest, ResourceRef, VolumeRequest};
use crate::rules::{EnvEdit, FieldChange};

/// One change a person asks for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Change {
    /// Set or remove environment variables of one container of a workload.
    #[serde(rename_all = "camelCase")]
    Env {
        /// The workload.
        workload: WorkloadRef,
        /// The container; may be left out when the workload has only one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        container: Option<String>,
        /// The variables to set or remove.
        changes: Vec<EnvEdit>,
    },
    /// Set or remove keys of a ConfigMap or Secret.
    #[serde(rename_all = "camelCase")]
    Data {
        /// `ConfigMap` or `Secret`.
        kind: DataKind,
        /// Its name.
        name: String,
        /// The keys to set or remove.
        changes: Vec<DataEdit>,
    },
    /// Route a new hostname to a workload.
    #[serde(rename_all = "camelCase")]
    AddDomain {
        /// The workload.
        workload: WorkloadRef,
        /// The hostname.
        hostname: String,
    },
    /// Replace a hostname of a route with another.
    #[serde(rename_all = "camelCase")]
    RenameDomain {
        /// The route.
        route: RouteRef,
        /// The hostname now.
        from: String,
        /// The hostname wanted.
        to: String,
    },
    /// Stop routing a hostname.
    #[serde(rename_all = "camelCase")]
    RemoveDomain {
        /// The route.
        route: RouteRef,
        /// The hostname.
        hostname: String,
    },
    /// Replace a workload's pods, as `kubectl rollout restart` does.
    #[serde(rename_all = "camelCase")]
    Restart {
        /// The workload.
        workload: WorkloadRef,
    },
    /// Return a workload's pod template to an earlier release, as `kubectl rollout undo
    /// --to-revision` does. ConfigMap and Secret contents stay as they are.
    #[serde(rename_all = "camelCase")]
    Rollback {
        /// The workload: a Deployment, StatefulSet or DaemonSet.
        workload: WorkloadRef,
        /// The release to return to.
        revision: i64,
    },
    /// Ask for more space on a PersistentVolumeClaim; claims only grow.
    #[serde(rename_all = "camelCase")]
    ExpandVolume {
        /// The claim.
        claim: String,
        /// The new size, a Kubernetes quantity such as `20Gi`.
        size: String,
    },
    /// Create a PersistentVolumeClaim and mount it in a container of a workload.
    #[serde(rename_all = "camelCase")]
    AddVolume {
        /// The workload.
        workload: WorkloadRef,
        /// The container; may be left out when the workload has only one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        container: Option<String>,
        /// The volume.
        volume: VolumeRequest,
    },
    /// Move a mount, or make it read-only or writable.
    #[serde(rename_all = "camelCase")]
    EditMount {
        /// The workload.
        workload: WorkloadRef,
        /// The container; may be left out when the workload has only one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        container: Option<String>,
        /// Where it is mounted now.
        mount_path: String,
        /// Where to mount it instead.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        new_mount_path: Option<String>,
        /// Mount it read-only (or not).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        read_only: Option<bool>,
    },
    /// Stop mounting a volume in a container, and drop it from the pod template once nothing
    /// mounts it. A PersistentVolumeClaim it names stays, with its data.
    #[serde(rename_all = "camelCase")]
    RemoveVolume {
        /// The workload.
        workload: WorkloadRef,
        /// The container; may be left out when the workload has only one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        container: Option<String>,
        /// Where it is mounted.
        mount_path: String,
    },
    /// Set or remove a liveness, readiness or startup probe of a container.
    #[serde(rename_all = "camelCase")]
    Probe {
        /// The workload.
        workload: WorkloadRef,
        /// The container; may be left out when the workload has only one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        container: Option<String>,
        /// Which probe.
        probe: ProbeKind,
        /// The probe; absent to remove it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        set: Option<ProbeRequest>,
    },
    /// Have Stakater Reloader replace a workload's pods when a ConfigMap or Secret it reads
    /// changes, or stop it: the workload's `reloader.stakater.com/auto` annotation (see the
    /// `reload` module).
    #[serde(rename_all = "camelCase")]
    ConfigReload {
        /// The workload: a Deployment, StatefulSet or DaemonSet.
        workload: WorkloadRef,
        /// On or off.
        enabled: bool,
    },
    /// Run another image in one container of a workload.
    #[serde(rename_all = "camelCase")]
    Image {
        /// The workload.
        workload: WorkloadRef,
        /// The container; may be left out when the workload has only one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        container: Option<String>,
        /// The image reference, such as a Zeabur Builder `Build`'s `status.latestImage`.
        image: String,
    },
    /// Build a Zeabur Builder `Build`'s current commit again (see the `builds` module).
    #[serde(rename_all = "camelCase")]
    Rebuild {
        /// The `Build`.
        build: String,
    },
    /// Have Keel move a workload to each new image, or stop it (see the `builds` module).
    #[serde(rename_all = "camelCase")]
    AutoUpdate {
        /// The workload: a Deployment, StatefulSet or DaemonSet.
        workload: WorkloadRef,
        /// On or off.
        enabled: bool,
        /// The `Build` whose new images Keel is told of, when turning it on.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        build: Option<String>,
        /// Keel's native webhook, `…/v1/webhooks/native`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        webhook: Option<String>,
    },
    /// Run this many pods of a Deployment or StatefulSet.
    #[serde(rename_all = "camelCase")]
    Scale {
        /// The workload.
        workload: WorkloadRef,
        /// The replicas.
        replicas: i32,
    },
    /// Set a container's CPU, memory and ephemeral storage requests and limits; those left out
    /// are removed, other resources (such as GPUs) are kept.
    #[serde(rename_all = "camelCase")]
    Resources {
        /// The workload.
        workload: WorkloadRef,
        /// The container; may be left out when the workload has only one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        container: Option<String>,
        /// Requests, by resource (`cpu`, `memory`, `ephemeral-storage`).
        #[serde(default)]
        requests: BTreeMap<String, String>,
        /// Limits, the same way.
        #[serde(default)]
        limits: BTreeMap<String, String>,
    },
    /// Replace one object of the project with an edited copy, written as YAML.
    ///
    /// Fields the server sets (`status`, `resourceVersion`, `managedFields`, …) are kept whatever
    /// the copy says. A ConfigMap's or Secret's edit becomes key changes: a Secret value shown
    /// masked and sent back unchanged is kept.
    #[serde(rename_all = "camelCase")]
    Edit {
        /// The object's resource.
        resource: ResourceRef,
        /// Its name.
        name: String,
        /// The edited object, YAML (JSON is YAML too).
        yaml: String,
    },
    /// Create one object in the project, of any namespaced kind the cluster serves (see the
    /// `create` module). Its namespace is the project's; an object of that name must not exist
    /// yet. Several are created in dependency order, without a transaction.
    #[serde(rename_all = "camelCase")]
    Create {
        /// The object: `apiVersion`, `kind`, `metadata.name`, and its fields. Left out when
        /// `yaml` gives it.
        #[serde(default, skip_serializing_if = "Value::is_null")]
        #[cfg_attr(feature = "openapi", schema(value_type = Option<Object>))]
        object: Value,
        /// The object as one YAML document, as a person wrote it; a problem with it names the
        /// line.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        yaml: Option<String>,
    },
    /// Delete one object of the project, as `kubectl delete` does: what it owns goes after it
    /// (background propagation). Refused for an object Helm, Argo CD or Flux manages, and for one
    /// a controller owns, except a Pod, which its owner replaces (see the `delete` module).
    #[serde(rename_all = "camelCase")]
    Delete {
        /// The object's resource.
        resource: ResourceRef,
        /// Its name.
        name: String,
        /// The `resourceVersion` the object must still have, such as the one an apply returned
        /// for an object it just created; when left out, the version the plan reads.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resource_version: Option<String>,
    },
    /// Make the namespace a project: add the label `zeabur.com/project: "true"` to the
    /// Namespace, and nothing else. Refused for the namespaces Kubernetes keeps for itself
    /// (`kube-` prefix). Planned on its own or with the project's details only.
    Adopt,
    /// Stop showing the namespace as a project: remove that label, and nothing else. Nothing in
    /// the namespace is deleted.
    Unadopt,
    /// Create the project's Namespace, with the project label and, when given, its display
    /// name. Refused when a namespace of that name exists (adopt it instead), and for a name
    /// Kubernetes keeps for itself (`kube-` prefix). Planned on its own.
    #[serde(rename_all = "camelCase")]
    CreateProject {
        /// The name to show instead of the namespace's.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
    },
    /// Set the project's details, the Namespace's `zeabur.com/display-name`,
    /// `…/description` and `…/icon` annotations: each one left out (or blank) is removed.
    #[serde(rename_all = "camelCase")]
    Details {
        /// The name to show instead of the namespace's.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
        /// What the project is.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        /// Its icon.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<ProjectIcon>,
    },
    /// Add a debug container to a pod, for a terminal in a container whose image has no shell:
    /// an ephemeral container with a shell image, sharing the target container's processes
    /// (see the `terminal` module). Kubernetes keeps it until the pod is replaced. When one of
    /// Zeabur's still runs for that container, the plan writes nothing and says so. Planned on
    /// its own.
    #[serde(rename_all = "camelCase")]
    DebugContainer {
        /// The pod.
        pod: String,
        /// The container whose processes it shares; by default the pod's default container.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        container: Option<String>,
    },
    /// Start a pod with `kubectl` and `helm` that act as the person, for a terminal (see the
    /// `terminal` module). When the person's tools pod still runs, the plan writes nothing and
    /// says so. Planned on its own.
    ClusterTools,
}

impl Change {
    /// Whether the change is planned for a terminal, alone ([`Change::DebugContainer`],
    /// [`Change::ClusterTools`]).
    #[must_use]
    pub const fn is_terminal_change(&self) -> bool {
        matches!(self, Self::DebugContainer { .. } | Self::ClusterTools)
    }

    /// Whether the change writes the project's Namespace (creating it, adopting it, letting it
    /// go, its details) rather than objects in it.
    #[must_use]
    pub const fn is_namespace_change(&self) -> bool {
        matches!(
            self,
            Self::CreateProject { .. } | Self::Adopt | Self::Unadopt | Self::Details { .. }
        )
    }
}

/// The probes of a container [k8s-probes].
///
/// [k8s-probes]: https://kubernetes.io/docs/concepts/configuration/liveness-readiness-startup-probes/
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum ProbeKind {
    /// Restarts the container when it fails.
    Liveness,
    /// Takes the pod out of its Services while it fails.
    Readiness,
    /// Holds the other two until it succeeds once.
    Startup,
}

impl ProbeKind {
    /// The container field (`livenessProbe`, …).
    #[must_use]
    pub const fn field(self) -> &'static str {
        match self {
            Self::Liveness => "livenessProbe",
            Self::Readiness => "readinessProbe",
            Self::Startup => "startupProbe",
        }
    }
}

/// A ConfigMap or a Secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum DataKind {
    /// `v1` `ConfigMap`.
    ConfigMap,
    /// `v1` `Secret`.
    Secret,
}

/// One requested change to a ConfigMap or Secret key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct DataEdit {
    /// `set` or `remove`.
    pub op: String,
    /// The key.
    pub key: String,
    /// The value, for `set`; for a Secret, the plain value (Zeabur encodes it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// A route object: an Ingress or a Gateway API `HTTPRoute`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RouteRef {
    /// `Ingress` or `HTTPRoute`.
    pub kind: RouteKind,
    /// Its name.
    pub name: String,
}

/// The kinds of route object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum RouteKind {
    /// `networking.k8s.io/v1` `Ingress`.
    Ingress,
    /// `gateway.networking.k8s.io/v1` `HTTPRoute`.
    #[serde(rename = "HTTPRoute")]
    HttpRoute,
}

/// What a person asks to change, as one plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct PlanRequest {
    /// The changes, applied in order.
    pub changes: Vec<Change>,
}

/// The object a planned write addresses.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", schema(as = PlanTarget))]
pub struct Target {
    /// `apiVersion`.
    pub api_version: String,
    /// The kind.
    pub kind: String,
    /// The resource (plural).
    pub plural: String,
    /// The name.
    pub name: String,
}

impl Target {
    /// The object `name` of the k8s-openapi type `K`.
    #[must_use]
    pub fn of<K: k8s_openapi::Resource>(name: &str) -> Self {
        Self {
            api_version: K::API_VERSION.to_owned(),
            kind: K::KIND.to_owned(),
            plural: K::URL_PATH_SEGMENT.to_owned(),
            name: name.to_owned(),
        }
    }

    /// The project's Namespace.
    #[must_use]
    pub fn namespace(name: &str) -> Self {
        Self::of::<k8s_openapi::api::core::v1::Namespace>(name)
    }

    /// Whether the object lives in the project's namespace: every object a plan writes does,
    /// except the Namespace itself.
    #[must_use]
    pub fn namespaced(&self) -> bool {
        !(self.api_version == "v1" && self.plural == "namespaces")
    }
}

/// How a planned write changes its object: a new object is created whole, an existing one is
/// patched or deleted. Creations and patches go as the field manager `zeabur`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "type", rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", schema(as = PlannedWrite))]
pub enum Write {
    /// A new object, created with `POST`; it fails if one of that name exists by then.
    #[serde(rename_all = "camelCase")]
    Create {
        /// The object, `apiVersion`, `kind` and `metadata` included.
        #[cfg_attr(feature = "openapi", schema(value_type = Object))]
        body: Value,
    },
    /// An RFC 6902 JSON Patch whose first operation tests `metadata.generation` (or, for an
    /// object without one, `metadata.resourceVersion`), and whose
    /// `test` operations check every value it replaces or removes.
    #[serde(rename_all = "camelCase")]
    Patch {
        /// The operations.
        #[cfg_attr(feature = "openapi", schema(value_type = Vec<Object>))]
        operations: Vec<Value>,
    },
    /// A `DELETE` with background propagation, refused (`409`) unless the object still has this
    /// `uid` and `resourceVersion`.
    #[serde(rename_all = "camelCase")]
    Delete {
        /// The object's `metadata.uid`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        uid: Option<String>,
        /// The object's `metadata.resourceVersion`.
        resource_version: String,
    },
    /// An RFC 6902 JSON Patch of a Pod's `ephemeralcontainers` subresource that adds one
    /// container. Its first operation tests `metadata.uid`, so a pod replaced since the plan
    /// fails it.
    #[serde(rename_all = "camelCase")]
    EphemeralContainers {
        /// The operations.
        #[cfg_attr(feature = "openapi", schema(value_type = Vec<Object>))]
        operations: Vec<Value>,
    },
}

/// The managers that own a field a write touches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = PlanFieldOwners))]
pub struct FieldOwners {
    /// The field.
    pub path: String,
    /// Its managers, sorted.
    pub managers: Vec<String>,
}

/// One object a plan writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct PlannedObject {
    /// The object.
    pub target: Target,
    /// The object as it is, without `managedFields` and with Secret values redacted; absent
    /// when the write creates it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<Object>))]
    pub before: Option<Value>,
    /// The object as it will be, the same way; absent when the write deletes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<Object>))]
    pub after: Option<Value>,
    /// `before` as YAML, written as the object's YAML view writes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_yaml: Option<String>,
    /// `after` as YAML, the same way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_yaml: Option<String>,
    /// The fields that change.
    pub changes: Vec<FieldChange>,
    /// Who owns the fields the write touches now.
    pub owners: Vec<FieldOwners>,
    /// The `resourceVersion` the plan was computed against; absent for a creation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_version: Option<String>,
    /// The write.
    pub write: Write,
    /// For an object a `create` makes, the index of that change in the request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<usize>,
    /// The object as it will be, unredacted: for re-projection only, never sent; `Null` when the
    /// write deletes it.
    #[serde(skip)]
    #[cfg_attr(feature = "openapi", schema(ignore))]
    pub raw_after: Value,
}

/// Something a person should know before applying.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "code", rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", schema(as = PlanWarning))]
pub enum Warning {
    /// These workloads read a changed ConfigMap or Secret into environment variables, which a
    /// running container reads only when it starts: restart them for the change to take effect.
    #[serde(rename_all = "camelCase")]
    RestartNeeded {
        /// The workloads.
        workloads: Vec<WorkloadRef>,
    },
    /// These workloads read a changed ConfigMap or Secret and ask Stakater Reloader to replace
    /// their pods when it changes, so Reloader restarts them after the apply.
    #[serde(rename_all = "camelCase")]
    ReloaderRestarts {
        /// The workloads.
        workloads: Vec<WorkloadRef>,
        /// Whether Reloader was seen running in the cluster; `false` when that could not be
        /// read with the person's rights.
        confirmed: bool,
    },
    /// A changed hostname was also listed for HTTPS; its certificate must be issued again for
    /// the new name.
    #[serde(rename_all = "camelCase")]
    TlsHostChanged {
        /// The route.
        route: String,
        /// The new hostname.
        hostname: String,
    },
    /// The new hostname is listed for HTTPS on an Ingress cert-manager watches: once applied,
    /// cert-manager asks this issuer for its certificate.
    #[serde(rename_all = "camelCase")]
    HttpsCertificate {
        /// The hostname.
        hostname: String,
        /// The `ClusterIssuer` or `Issuer` the Ingress names.
        issuer: String,
    },
    /// The new hostname is served over HTTP only.
    #[serde(rename_all = "camelCase")]
    HttpOnly {
        /// The hostname.
        hostname: String,
        /// Why.
        reason: HttpOnlyReason,
        /// The Ingress whose certificates come from elsewhere, for `certificatesElsewhere`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        route: Option<String>,
    },
    /// The new `HTTPRoute` attaches to this Gateway, the first of several.
    #[serde(rename_all = "camelCase")]
    GatewayChosen {
        /// Its namespace.
        namespace: String,
        /// Its name.
        name: String,
        /// How many Gateways there are.
        of: usize,
    },
    /// Gateways could not be listed, so the new route is an Ingress.
    GatewaysUnreadable,
    /// The Secret's values could not be read, so its diff shows only which keys change.
    #[serde(rename_all = "camelCase")]
    SecretValuesUnknown {
        /// The Secret.
        name: String,
    },
    /// A rollback returns the pod template only; the ConfigMaps and Secrets the workload reads
    /// keep their current contents.
    #[serde(rename_all = "camelCase")]
    ConfigNotRolledBack {
        /// The workload.
        workload: WorkloadRef,
    },
    /// The claim's `StorageClass` could not be read, so whether it can grow is known only when
    /// kube-apiserver answers.
    #[serde(rename_all = "camelCase")]
    StorageClassUnknown {
        /// The claim.
        claim: String,
    },
    /// A new `ReadWriteOnce` claim is mounted by a workload with several pods, which can share
    /// it only while they run on one node.
    #[serde(rename_all = "camelCase")]
    ReadWriteOnceShared {
        /// The workload.
        workload: WorkloadRef,
    },
    /// A new `ReadWriteOncePod` claim is mounted by a workload with several pods, of which only
    /// one at a time can use it.
    #[serde(rename_all = "camelCase")]
    ReadWriteOncePodShared {
        /// The workload.
        workload: WorkloadRef,
    },
    /// The deleted object owns these objects, as far as the person could read them; the garbage
    /// collector deletes them after it [k8s-gc].
    ///
    /// [k8s-gc]: https://kubernetes.io/docs/concepts/architecture/garbage-collection/
    #[serde(rename_all = "camelCase")]
    DeletesDependents {
        /// The deleted object.
        target: ObjectName,
        /// What goes with it, owners before what they own.
        dependents: Vec<ObjectName>,
    },
    /// The deleted Pod belongs to a controller, which starts a new one in its place.
    #[serde(rename_all = "camelCase")]
    PodReplaced {
        /// The Pod.
        pod: String,
        /// Its controller.
        owner: ObjectName,
    },
    /// Deleting the claim may lose its data: what happens to its PersistentVolume is the volume's
    /// reclaim policy, `Delete` (the volume and its data go) or `Retain` (they stay until an
    /// administrator removes them) [k8s-reclaim].
    ///
    /// [k8s-reclaim]: https://kubernetes.io/docs/concepts/storage/persistent-volumes/#reclaiming
    #[serde(rename_all = "camelCase")]
    ClaimDataMayBeLost {
        /// The claim.
        claim: String,
        /// Its PersistentVolume; absent while the claim is not bound.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        volume: Option<String>,
        /// The volume's `persistentVolumeReclaimPolicy`; absent when it could not be read.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reclaim_policy: Option<String>,
    },
    /// These workloads still use the deleted ConfigMap, Secret or claim: their pods that start
    /// after it is gone fail to start, and a claim in use stays until no pod uses it
    /// [k8s-in-use].
    ///
    /// [k8s-in-use]: https://kubernetes.io/docs/concepts/storage/persistent-volumes/#storage-object-in-use-protection
    #[serde(rename_all = "camelCase")]
    StillUsed {
        /// The deleted object.
        target: ObjectName,
        /// The workloads.
        workloads: Vec<WorkloadRef>,
    },
    /// Deleting the NetworkPolicy opens traffic it held back: these services are no longer
    /// isolated, and these connections the project shows become allowed.
    #[serde(rename_all = "camelCase")]
    NetworkPolicyRemoved {
        /// The policy.
        policy: String,
        /// Services nothing limits reaching once it is gone.
        ingress_opened: Vec<WorkloadRef>,
        /// Services nothing limits sending once it is gone.
        egress_opened: Vec<WorkloadRef>,
        /// Connections it blocked or limited that become allowed.
        connections_opened: Vec<ConnectionRef>,
    },
    /// The plan adds this debug container to the pod. Kubernetes cannot remove an ephemeral
    /// container: it stays in the pod's spec, stopped once its shell exits, until the pod is
    /// replaced.
    #[serde(rename_all = "camelCase")]
    DebugContainerStays {
        /// The pod.
        pod: String,
        /// The debug container.
        container: String,
        /// The container whose processes it shares.
        target: String,
        /// Its image.
        image: String,
    },
    /// A debug container of Zeabur's still runs (or is starting) for this container; the
    /// terminal uses it, and nothing is written.
    #[serde(rename_all = "camelCase")]
    DebugContainerReused {
        /// The pod.
        pod: String,
        /// The debug container.
        container: String,
        /// The container whose processes it shares.
        target: String,
    },
    /// The person's credential lacks rights the plan needs: kube-apiserver will refuse it unless
    /// an administrator grants them (or the person acts as one).
    #[serde(rename_all = "camelCase")]
    RightsMissing {
        /// Each right, as kube-apiserver would name it in a refusal.
        rights: Vec<zeabur_kernel::denied::Denied>,
    },
    /// The plan creates a tools pod: what runs where, as whom, until when.
    #[serde(rename_all = "camelCase")]
    ToolsPod {
        /// Its namespace.
        namespace: String,
        /// Its name.
        pod: String,
        /// Its image.
        image: String,
        /// Who `kubectl` and `helm` act as: the person's user name.
        person: String,
        /// Whether they act with the administrator's credential, which the person holds now.
        admin: bool,
        /// When the credential written into the pod expires, RFC 3339; `kubectl` is refused
        /// from then until a terminal opens again with a fresh one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        credential_expires_at: Option<String>,
        /// The pod stops this long after it starts, whatever happens (`activeDeadlineSeconds`).
        deadline_seconds: i64,
        /// It is deleted this long after its last terminal closes.
        grace_seconds: i64,
    },
    /// While the credential is valid, anyone who can run commands in the tools pod, or read its
    /// memory, can use it as the person.
    #[serde(rename_all = "camelCase")]
    ToolsCredentialExposed {
        /// The pod's namespace, whose `pods/exec` grants decide who that is.
        namespace: String,
    },
    /// The person's tools pod still runs; the terminal uses it, and nothing is written.
    #[serde(rename_all = "camelCase")]
    ToolsPodReused {
        /// Its namespace.
        namespace: String,
        /// Its name.
        pod: String,
    },
}

/// An object of the project, by kind and name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ObjectName {
    /// The kind.
    pub kind: String,
    /// The name.
    pub name: String,
}

/// A connection between two services of the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ConnectionRef {
    /// The caller.
    pub from: WorkloadRef,
    /// The callee.
    pub to: WorkloadRef,
}

/// What a plan will do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// The objects it writes, in the order it writes them.
    pub objects: Vec<PlannedObject>,
    /// The workloads it affects, sorted.
    pub impact: Vec<WorkloadRef>,
    /// What to know before applying.
    pub warnings: Vec<Warning>,
}

/// Why a new hostname gets no HTTPS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum HttpOnlyReason {
    /// The chart names no `ClusterIssuer` (`domains.clusterIssuer`) and the Ingress no issuer.
    NoIssuer,
    /// The Ingress lists certificates cert-manager does not issue; the new hostname has to be
    /// added where they are made.
    CertificatesElsewhere,
}
