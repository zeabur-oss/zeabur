//! An in-memory [`Cluster`] for tests: fixed objects, and refusals that can be switched on at
//! any time, as when a cluster administrator removes a person's rights.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard};

use k8s_openapi::Metadata;
use k8s_openapi::api::apps::v1::{
    ControllerRevision, DaemonSet, Deployment, ReplicaSet, StatefulSet,
};
use k8s_openapi::api::batch::v1::{CronJob, Job};
use k8s_openapi::api::core::v1::{
    ConfigMap, Namespace, Node, PersistentVolumeClaim, ResourceQuota, Secret, Service,
};
use k8s_openapi::api::events::v1::Event;
use k8s_openapi::api::networking::v1::Ingress;
use k8s_openapi::api::storage::v1::StorageClass;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::DynamicObject;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use zeabur_core::json_patch::apply_patch;
use zeabur_core::problem::{Denied, KubernetesStatus};

mod files;

pub use files::{MemoryFiles, MemoryNode};

use crate::objects::{
    AccessRule, AccessRules, ResourceRef, ServedResource, hide_metadata, hide_secret_values,
};
use crate::port::{
    Answer, Cluster, ClusterError, ClusterEvents, ClusterFacts, ClusterPods, ExecRequest,
    ExecSession, ExecStream, ExitStatus, Identity, LogLines, LogOptions, NamespaceObjects,
    ObjectTarget, Preconditions, SecretSummary, ServicePort, TerminalSize, Unreadable, WriteBody,
    WriteRequest, Written,
};

/// What the in-memory cluster holds.
#[derive(Debug, Default)]
pub struct State {
    /// The kubeconfig's name for the cluster ([`Cluster::kubeconfig_name`]); empty: none.
    pub name: String,
    /// The person's identity.
    pub identity: Identity,
    /// Namespaces.
    pub namespaces: Vec<Namespace>,
    /// Objects per namespace.
    pub objects: BTreeMap<String, NamespaceObjects>,
    /// Events per namespace.
    pub events: BTreeMap<String, Vec<Event>>,
    /// Logs per `namespace/pod`; a pod among `objects` without one has written nothing yet.
    pub logs: BTreeMap<String, String>,
    /// Logs of the run before the last restart, per `namespace/pod`; a pod with a log but none
    /// here has not restarted, and asking for its previous run is refused as the kubelet does.
    pub previous_logs: BTreeMap<String, String>,
    /// Secret values per `namespace/name`.
    pub secret_values: BTreeMap<String, BTreeMap<String, Vec<u8>>>,
    /// Cluster-wide facts.
    pub facts: ClusterFacts,
    /// The body of `/readyz?verbose`; empty answers three passing checks. Refused with `403`
    /// when `forbidden_resources` holds `/readyz`.
    pub readyz: String,
    /// When set, every call is refused with `403 Forbidden`.
    pub forbid_everything: bool,
    /// When set, every call is refused with `429 Too Many Requests` and a `Retry-After` of this
    /// many seconds, as API Priority and Fairness answers under load [k8s-apf].
    ///
    /// [k8s-apf]: https://kubernetes.io/docs/concepts/cluster-administration/flow-control/
    pub busy: Option<u32>,
    /// Namespaces whose objects are refused with `403 Forbidden`.
    pub forbidden_namespaces: BTreeSet<String>,
    /// How many times a Secret's values were read.
    pub secret_reads: usize,
    /// Nodes.
    pub nodes: Vec<Node>,
    /// Resources served besides the built-in kinds ([`builtin_resources`]), such as custom
    /// resources or `metrics.k8s.io`.
    pub served: Vec<ServedResource>,
    /// Objects of the resources in `served`, as JSON with `apiVersion` and `kind`.
    pub dynamic: Vec<DynamicObject>,
    /// Cluster-wide reads refused with `403 Forbidden`, by resource as [`ResourceRef::key`]
    /// names it: `nodes`, `pods`, `nodes.metrics.k8s.io`, `events.events.k8s.io`,
    /// `plans.upgrade.cattle.io`.
    pub forbidden_resources: BTreeSet<String>,
    /// Reads refused with `403 Forbidden` in every namespace, by resource as
    /// [`ResourceRef::key`] names it (`secrets`, `httproutes.gateway.networking.k8s.io`): a
    /// namespace's objects leave them out and name them in `unreadable`.
    pub forbidden_in_namespaces: BTreeSet<String>,
    /// Every write that was persisted (not dry runs), in order.
    pub writes: Vec<WriteRequest>,
    /// How many dry-run writes were made.
    pub dry_runs: usize,
    /// Objects whose writes are refused with `403 Forbidden`, as `kind/name`.
    pub forbidden_writes: BTreeSet<String>,
    /// Objects whose writes, dry runs included, admission refuses after the person's rights
    /// allowed them, as `kind/name`, with kube-apiserver's answer: a quota's `403` or a
    /// webhook's `400` or `403`.
    pub admission_refusals: BTreeMap<String, KubernetesStatus>,
    /// Objects whose dry run passes but whose write is refused, as `kind/name`, with
    /// kube-apiserver's answer: what changed in the cluster between the two.
    pub write_refusals: BTreeMap<String, KubernetesStatus>,
    /// `ResourceQuotas` per namespace.
    pub resource_quotas: BTreeMap<String, Vec<ResourceQuota>>,
    /// ReplicaSets per namespace.
    pub replica_sets: BTreeMap<String, Vec<ReplicaSet>>,
    /// `ControllerRevisions` per namespace.
    pub controller_revisions: BTreeMap<String, Vec<ControllerRevision>>,
    /// `StorageClasses`; `None` when listing them is refused.
    pub storage_classes: Option<Vec<StorageClass>>,
    /// How many `metadata.uid` values creations were given.
    pub uids: u64,
    /// Pods, as `namespace/pod`, in which running a command is refused with `403 Forbidden`.
    pub exec_refused: BTreeSet<String>,
    /// Pods, as `namespace/pod`, whose image has no shell, as a distroless image; their ephemeral
    /// containers have one.
    pub without_shell: BTreeSet<String>,
    /// The files of each pod, by `namespace/pod`, which [`Cluster::stream`] reads and writes. A
    /// pod of `without_shell` has no `tar` either, but in its ephemeral containers.
    pub files: BTreeMap<String, MemoryFiles>,
    /// Every command [`Cluster::stream`] ran, as `namespace/pod/container` and its arguments.
    pub streams: Vec<(String, Vec<String>)>,
    /// Every terminal size a [`Cluster::exec`] session was given, in order.
    pub terminal_sizes: Vec<TerminalSize>,
    /// Every command [`Cluster::run`] ran, in order, with what it was given on standard input.
    pub runs: Vec<Run>,
    /// The `ephemeralcontainers` subresource is not served (`404`), as on a cluster that has
    /// ephemeral containers turned off.
    pub without_ephemeral_containers: bool,
    /// What each Service answers through the service proxy, by `namespace/name:port`; a Service
    /// not here does not exist (`404`).
    pub proxied: BTreeMap<String, ProxyAnswer>,
    /// Every path and query asked of a Service through the proxy, as
    /// `namespace/name:port/path?query`, in order.
    pub proxy_requests: Vec<String>,
    /// What a `SelfSubjectRulesReview` answers in a namespace, instead of rules that follow the
    /// refusals above; `None`: the review fails with `500`.
    pub access_rules: BTreeMap<String, Option<AccessRules>>,
    /// Every cluster-wide `SelfSubjectAccessReview` (one without a namespace) answers no, and
    /// creating a Namespace is refused.
    pub cluster_wide_denied: bool,
    /// Rights every `SelfSubjectAccessReview` answers no to, as `verb resource` (`create
    /// pods/exec`); the calls themselves are not refused, as on a cluster that checks fewer
    /// verbs.
    pub refused_rights: BTreeSet<String>,
}

/// A command [`Cluster::run`] ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// `namespace/pod`.
    pub pod: String,
    /// The container.
    pub container: String,
    /// The command.
    pub command: Vec<String>,
    /// What it read on standard input.
    pub input: Vec<u8>,
}

/// How a Service answers a `GET` through the service proxy: from the path and query, its JSON,
/// or the status kube-apiserver passes on (`403` for a refused proxy, `503` when the Service has
/// no ready endpoint).
pub type ProxyAnswer = fn(&str) -> Result<Value, KubernetesStatus>;

/// A [`Cluster`] whose answers come from a [`State`] the test controls.
#[derive(Debug, Clone, Default)]
pub struct MemoryCluster {
    state: Arc<Mutex<State>>,
}

fn forbidden(what: &str) -> ClusterError {
    ClusterError::Status(KubernetesStatus::new(
        403,
        "Forbidden",
        format!("{what} is forbidden"),
    ))
}

fn not_found(what: &str) -> ClusterError {
    ClusterError::Status(KubernetesStatus::new(
        404,
        "NotFound",
        format!("{what} not found"),
    ))
}

/// The built-in kinds the in-memory cluster serves from [`State`]'s typed fields.
#[must_use]
pub fn builtin_resources() -> Vec<ServedResource> {
    let r = |group: &str, resource: &str, kind: &str, namespaced: bool| ServedResource {
        verbs: [
            "create", "delete", "get", "list", "patch", "update", "watch",
        ]
        .map(str::to_owned)
        .to_vec(),
        ..ServedResource::readable(group, "v1", resource, kind, namespaced)
    };
    vec![
        r("", "configmaps", "ConfigMap", true),
        r("", "namespaces", "Namespace", false),
        r("", "nodes", "Node", false),
        r("", "persistentvolumeclaims", "PersistentVolumeClaim", true),
        r("", "pods", "Pod", true),
        r("", "secrets", "Secret", true),
        r("", "services", "Service", true),
        r("apps", "daemonsets", "DaemonSet", true),
        r("apps", "deployments", "Deployment", true),
        r("apps", "statefulsets", "StatefulSet", true),
        r("batch", "cronjobs", "CronJob", true),
        r("batch", "jobs", "Job", true),
        r("events.k8s.io", "events", "Event", true),
        r("networking.k8s.io", "ingresses", "Ingress", true),
    ]
}

fn to_dynamic<T: Serialize>(object: &T) -> Result<DynamicObject, ClusterError> {
    serde_json::to_value(object)
        .and_then(serde_json::from_value)
        .map_err(|e| ClusterError::Decode(e.to_string()))
}

fn all<T: Serialize>(items: &[T]) -> Result<Vec<DynamicObject>, ClusterError> {
    items.iter().map(to_dynamic).collect()
}

/// The objects of `namespace` (every namespace when `None`) in a map of objects per namespace.
fn in_namespace<T: Serialize>(
    objects: &BTreeMap<String, Vec<T>>,
    namespace: Option<&str>,
) -> Result<Vec<DynamicObject>, ClusterError> {
    objects
        .iter()
        .filter(|(ns, _)| namespace.is_none_or(|n| n == ns.as_str()))
        .flat_map(|(_, items)| items)
        .map(to_dynamic)
        .collect()
}

impl State {
    /// What admission answers a write of `what` (`kind/name`), dry run or not.
    fn refusal(&self, what: &str, dry_run: bool) -> Option<KubernetesStatus> {
        self.admission_refusals
            .get(what)
            .or_else(|| (!dry_run).then(|| self.write_refusals.get(what)).flatten())
            .cloned()
    }

    /// The typed objects of `resource` in one namespace's objects.
    fn builtin_in(
        &self,
        resource: &ResourceRef,
        objects: &NamespaceObjects,
    ) -> Result<Vec<DynamicObject>, ClusterError> {
        Ok(
            match (resource.group.as_str(), resource.resource.as_str()) {
                ("", "configmaps") => all(&objects.config_maps)?,
                ("", "persistentvolumeclaims") => all(&objects.persistent_volume_claims)?,
                ("", "pods") => all(&objects.pods)?,
                ("", "services") => all(&objects.services)?,
                ("", "secrets") => {
                    let mut out = Vec::new();
                    for summary in &objects.secrets {
                        let mut object = to_dynamic(&summary.secret)?;
                        let key = format!(
                            "{}/{}",
                            objects.namespace,
                            summary.secret.metadata.name.as_deref().unwrap_or_default()
                        );
                        // The values as kube-apiserver returns them (base64); hidden on the way
                        // out.
                        let data: serde_json::Map<String, Value> = self
                            .secret_values
                            .get(&key)
                            .into_iter()
                            .flatten()
                            .map(|(k, v)| {
                                (
                                    k.clone(),
                                    Value::String(String::from_utf8_lossy(v).into_owned()),
                                )
                            })
                            .collect();
                        if !data.is_empty() {
                            object
                                .data
                                .as_object_mut()
                                .map(|o| o.insert("data".to_owned(), Value::Object(data)));
                        }
                        out.push(object);
                    }
                    out
                }
                ("apps", "daemonsets") => all(&objects.daemon_sets)?,
                ("apps", "deployments") => all(&objects.deployments)?,
                ("apps", "statefulsets") => all(&objects.stateful_sets)?,
                ("batch", "cronjobs") => all(&objects.cron_jobs)?,
                ("batch", "jobs") => all(&objects.jobs)?,
                ("networking.k8s.io", "ingresses") => all(&objects.ingresses)?,
                _ => Vec::new(),
            },
        )
    }

    /// Every object of `resource`, built-in or dynamic, in `namespace` (all when `None`).
    fn objects_of(
        &self,
        resource: &ResourceRef,
        namespace: Option<&str>,
    ) -> Result<Vec<DynamicObject>, ClusterError> {
        let mut out = Vec::new();
        for objects in self.objects.values() {
            if namespace.is_none_or(|n| n == objects.namespace) {
                out.extend(self.builtin_in(resource, objects)?);
            }
        }
        match (resource.group.as_str(), resource.resource.as_str()) {
            ("", "namespaces") if namespace.is_none() => out.extend(all(&self.namespaces)?),
            ("", "nodes") if namespace.is_none() => out.extend(all(&self.nodes)?),
            ("events.k8s.io", "events") => {
                for (ns, events) in &self.events {
                    if namespace.is_none_or(|n| n == ns) {
                        out.extend(all(events)?);
                    }
                }
            }
            _ => {}
        }
        let api_version = resource.api_version();
        out.extend(
            self.dynamic
                .iter()
                .filter(|o| {
                    o.types
                        .as_ref()
                        .is_some_and(|t| t.api_version == api_version && t.kind == resource.kind)
                        && namespace.is_none_or(|n| o.metadata.namespace.as_deref() == Some(n))
                })
                .cloned(),
        );
        Ok(out)
    }

    /// Whether `group_version` is served: a built-in kind's or one in [`State::served`].
    fn serves(&self, group_version: &str) -> bool {
        builtin_resources()
            .iter()
            .chain(&self.served)
            .any(|r| r.as_ref().api_version() == group_version)
    }

    /// The kind served as `plural` in `api_version`, or empty when none is.
    fn kind_of(&self, api_version: &str, plural: &str) -> String {
        builtin_resources()
            .iter()
            .chain(&self.served)
            .find(|r| r.resource == plural && r.as_ref().api_version() == api_version)
            .map_or_else(String::new, |r| r.kind.clone())
    }

    /// The object `target` names, as JSON: from [`State::dynamic`], or the typed objects of its
    /// namespace (a Secret without its values).
    fn stored(&self, target: &ObjectTarget) -> Result<Option<Value>, ClusterError> {
        if let Some(object) = self.dynamic.iter().find(|o| is_target(o, target)) {
            return json(object).map(Some);
        }
        if target.kind == "Namespace" {
            return find(&self.namespaces, &target.name);
        }
        let Some(objects) = self.objects.get(&target.namespace) else {
            return Ok(None);
        };
        let name = target.name.as_str();
        match target.kind.as_str() {
            "Deployment" => find(&objects.deployments, name),
            "Pod" => find(&objects.pods, name),
            "StatefulSet" => find(&objects.stateful_sets, name),
            "DaemonSet" => find(&objects.daemon_sets, name),
            "Job" => find(&objects.jobs, name),
            "CronJob" => find(&objects.cron_jobs, name),
            "Service" => find(&objects.services, name),
            "ConfigMap" => find(&objects.config_maps, name),
            "PersistentVolumeClaim" => find(&objects.persistent_volume_claims, name),
            "Ingress" => find(&objects.ingresses, name),
            "Secret" => objects
                .secrets
                .iter()
                .find(|s| s.secret.metadata.name.as_deref() == Some(name))
                .map(|s| json(&s.secret))
                .transpose(),
            "HTTPRoute" => Ok(objects
                .http_routes
                .iter()
                .find(|o| o.pointer("/metadata/name").and_then(Value::as_str) == Some(name))
                .cloned()),
            _ => Ok(None),
        }
    }

    /// Puts `object` where [`State::stored`] finds it, replacing what was there. An object of a
    /// kind held nowhere else goes to [`State::dynamic`].
    fn store(&mut self, target: &ObjectTarget, object: Value) -> Result<(), ClusterError> {
        if let Some(slot) = self.dynamic.iter_mut().find(|o| is_target(o, target)) {
            *slot = decode(object)?;
            return Ok(());
        }
        if target.kind == "Namespace" {
            return put(&mut self.namespaces, &target.name, object);
        }
        if target.kind == "Secret" {
            keep_string_data(self, target, &object);
        }
        let namespace = target.namespace.clone();
        let objects = self
            .objects
            .entry(namespace.clone())
            .or_insert_with(|| NamespaceObjects {
                namespace,
                ..NamespaceObjects::default()
            });
        let name = target.name.as_str();
        match target.kind.as_str() {
            "Deployment" => put(&mut objects.deployments, name, object),
            "Pod" => put(&mut objects.pods, name, object),
            "StatefulSet" => put(&mut objects.stateful_sets, name, object),
            "DaemonSet" => put(&mut objects.daemon_sets, name, object),
            "Job" => put(&mut objects.jobs, name, object),
            "CronJob" => put(&mut objects.cron_jobs, name, object),
            "Service" => put(&mut objects.services, name, object),
            "ConfigMap" => put(&mut objects.config_maps, name, object),
            "PersistentVolumeClaim" => put(&mut objects.persistent_volume_claims, name, object),
            "Ingress" => put(&mut objects.ingresses, name, object),
            "Secret" => {
                let key_count = ["data", "stringData"]
                    .iter()
                    .filter_map(|field| object.get(field).and_then(Value::as_object))
                    .map(serde_json::Map::len)
                    .sum();
                let mut secret: Secret = decode(object)?;
                secret.data = None;
                secret.string_data = None;
                match objects
                    .secrets
                    .iter_mut()
                    .find(|s| s.secret.metadata.name.as_deref() == Some(name))
                {
                    Some(summary) => summary.secret = secret,
                    None => objects.secrets.push(SecretSummary { secret, key_count }),
                }
                Ok(())
            }
            "HTTPRoute" => {
                let routes = &mut objects.http_routes;
                routes
                    .retain(|o| o.pointer("/metadata/name").and_then(Value::as_str) != Some(name));
                routes.push(object);
                Ok(())
            }
            _ => {
                self.dynamic.push(decode(object)?);
                Ok(())
            }
        }
    }

    /// Takes away the object [`State::stored`] finds for `target`.
    fn remove(&mut self, target: &ObjectTarget) {
        fn named<T: Metadata<Ty = ObjectMeta>>(items: &mut Vec<T>, name: &str) {
            items.retain(|o| o.metadata().name.as_deref() != Some(name));
        }
        self.dynamic.retain(|o| !is_target(o, target));
        let Some(objects) = self.objects.get_mut(&target.namespace) else {
            return;
        };
        let name = target.name.as_str();
        match target.kind.as_str() {
            "Deployment" => named(&mut objects.deployments, name),
            "Pod" => named(&mut objects.pods, name),
            "StatefulSet" => named(&mut objects.stateful_sets, name),
            "DaemonSet" => named(&mut objects.daemon_sets, name),
            "Job" => named(&mut objects.jobs, name),
            "CronJob" => named(&mut objects.cron_jobs, name),
            "Service" => named(&mut objects.services, name),
            "ConfigMap" => named(&mut objects.config_maps, name),
            "PersistentVolumeClaim" => named(&mut objects.persistent_volume_claims, name),
            "Ingress" => named(&mut objects.ingresses, name),
            "Secret" => objects
                .secrets
                .retain(|s| s.secret.metadata.name.as_deref() != Some(name)),
            "HTTPRoute" => objects
                .http_routes
                .retain(|o| o.pointer("/metadata/name").and_then(Value::as_str) != Some(name)),
            _ => {}
        }
    }
}

impl MemoryCluster {
    /// A cluster holding `state`.
    #[must_use]
    pub fn new(state: State) -> Self {
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }

    /// Changes the state, for example to take rights away.
    pub fn update(&self, change: impl FnOnce(&mut State)) {
        if let Ok(mut state) = self.state.lock() {
            change(&mut state);
        }
    }

    /// Reads the state.
    pub fn inspect<T>(&self, read: impl FnOnce(&State) -> T) -> Option<T> {
        self.state.lock().ok().map(|state| read(&state))
    }

    fn lock(&self) -> Result<MutexGuard<'_, State>, ClusterError> {
        self.state
            .lock()
            .map_err(|e| ClusterError::Transport(format!("in-memory cluster poisoned: {e}")))
    }

    /// The state, unless the cluster-wide read of `resource` is refused.
    fn wide(&self, resource: &str) -> Result<MutexGuard<'_, State>, ClusterError> {
        let state = self.allowed(None)?;
        if state.forbidden_resources.contains(resource) {
            return Err(forbidden(resource));
        }
        Ok(state)
    }

    /// The state, unless reading `resource` in `namespace` (cluster-wide when `None`) is refused.
    fn readable(
        &self,
        resource: &ResourceRef,
        namespace: Option<&str>,
    ) -> Result<MutexGuard<'_, State>, ClusterError> {
        let state = self.allowed(namespace)?;
        let refused = if namespace.is_none() {
            &state.forbidden_resources
        } else {
            &state.forbidden_in_namespaces
        };
        if refused.contains(&resource.key()) {
            return Err(forbidden(&resource.key()));
        }
        Ok(state)
    }

    fn allowed(&self, namespace: Option<&str>) -> Result<MutexGuard<'_, State>, ClusterError> {
        let state = self.lock()?;
        if let Some(seconds) = state.busy {
            return Err(ClusterError::Status(KubernetesStatus {
                retry_after_seconds: Some(seconds),
                ..KubernetesStatus::new(429, "TooManyRequests", "too many requests")
            }));
        }
        if state.forbid_everything
            || namespace.is_some_and(|n| state.forbidden_namespaces.contains(n))
        {
            return Err(forbidden(namespace.unwrap_or("the cluster")));
        }
        Ok(state)
    }
}

impl Cluster for MemoryCluster {
    fn whoami(&self) -> Answer<'_, Identity> {
        Box::pin(async move { Ok(self.lock()?.identity.clone()) })
    }

    fn namespaces(&self) -> Answer<'_, Vec<Namespace>> {
        Box::pin(async move { Ok(self.allowed(None)?.namespaces.clone()) })
    }

    fn namespace_objects<'a>(&'a self, namespace: &'a str) -> Answer<'a, NamespaceObjects> {
        Box::pin(async move {
            let state = self.allowed(Some(namespace))?;
            let mut objects =
                state
                    .objects
                    .get(namespace)
                    .cloned()
                    .unwrap_or_else(|| NamespaceObjects {
                        namespace: namespace.to_owned(),
                        ..NamespaceObjects::default()
                    });
            for resource in &state.forbidden_in_namespaces {
                match resource.as_str() {
                    "deployments.apps" => objects.deployments.clear(),
                    "statefulsets.apps" => objects.stateful_sets.clear(),
                    "daemonsets.apps" => objects.daemon_sets.clear(),
                    "jobs.batch" => objects.jobs.clear(),
                    "cronjobs.batch" => objects.cron_jobs.clear(),
                    "pods" => objects.pods.clear(),
                    "services" => objects.services.clear(),
                    "configmaps" => objects.config_maps.clear(),
                    "secrets" => objects.secrets.clear(),
                    "persistentvolumeclaims" => objects.persistent_volume_claims.clear(),
                    "ingresses.networking.k8s.io" => objects.ingresses.clear(),
                    "httproutes.gateway.networking.k8s.io" => objects.http_routes.clear(),
                    // Not part of a namespace's objects: refused where it is read.
                    _ => continue,
                }
                objects.unreadable.push(Unreadable {
                    resource: resource.clone(),
                    status: KubernetesStatus::new(
                        403,
                        "Forbidden",
                        format!("{resource} is forbidden"),
                    ),
                });
            }
            drop(state);
            Ok(objects)
        })
    }

    fn namespace_workloads<'a>(&'a self, namespace: &'a str) -> Answer<'a, NamespaceObjects> {
        Box::pin(async move {
            let all = self.namespace_objects(namespace).await?;
            Ok(NamespaceObjects {
                namespace: all.namespace,
                deployments: all.deployments,
                stateful_sets: all.stateful_sets,
                daemon_sets: all.daemon_sets,
                jobs: all.jobs,
                cron_jobs: all.cron_jobs,
                pods: all.pods,
                ..NamespaceObjects::default()
            })
        })
    }

    fn version(&self) -> Answer<'_, String> {
        Box::pin(async move { Ok(self.lock()?.facts.version.clone()) })
    }

    fn readyz(&self) -> Answer<'_, String> {
        Box::pin(async move {
            let state = self.wide("/readyz")?;
            Ok(if state.readyz.is_empty() {
                "[+]ping ok\n[+]log ok\n[+]etcd ok\nreadyz check passed\n".to_owned()
            } else {
                state.readyz.clone()
            })
        })
    }

    fn kubeconfig_name(&self) -> Option<String> {
        self.lock()
            .ok()
            .map(|s| s.name.clone())
            .filter(|name| !name.is_empty())
    }

    fn pod_logs<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        options: &'a LogOptions,
    ) -> Answer<'a, String> {
        Box::pin(async move {
            let key = format!("{namespace}/{pod}");
            let state = self.allowed(Some(namespace))?;
            let exists = state.objects.get(namespace).is_some_and(|o| {
                o.pods
                    .iter()
                    .any(|p| p.metadata.name.as_deref() == Some(pod))
            });
            if !state.logs.contains_key(&key) && !exists {
                return Err(not_found(&key));
            }
            if !options.previous {
                return Ok(state.logs.get(&key).cloned().unwrap_or_default());
            }
            state.previous_logs.get(&key).cloned().ok_or_else(|| {
                // The kubelet's refusal, as kube-apiserver passes it on.
                let container = options.container.as_deref().unwrap_or(pod);
                ClusterError::Status(KubernetesStatus::new(
                    400,
                    "BadRequest",
                    format!("previous terminated container {container:?} in pod {pod:?} not found"),
                ))
            })
        })
    }

    fn follow_pod_logs<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        options: &'a LogOptions,
    ) -> Answer<'a, LogLines> {
        Box::pin(async move {
            let text = self.pod_logs(namespace, pod, options).await?;
            let lines: Vec<Result<String, ClusterError>> =
                text.lines().map(|l| Ok(l.to_owned())).collect();
            let lines: LogLines = Box::pin(futures_util::stream::iter(lines));
            Ok(lines)
        })
    }

    /// An echo shell: it echoes what is typed, as a TTY does, answers each line with the line,
    /// and exits on `exit [code]` or end of input. Any command but `/bin/sh` or `sh`, or any
    /// command in a container of a pod of [`State::without_shell`] that is not an ephemeral
    /// container, is not found; `nsenter … sh …` is a root shell on the pod's node, prompting
    /// `root@<node>:~# `.
    fn exec<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        request: &'a ExecRequest,
    ) -> Answer<'a, ExecSession> {
        Box::pin(async move {
            let key = format!("{namespace}/{pod}");
            let has_shell = {
                let state = self.allowed(Some(namespace))?;
                if state.exec_refused.contains(&key) {
                    return Err(forbidden(&format!("pods/exec {key}")));
                }
                let found = state.objects.get(namespace).and_then(|objects| {
                    objects
                        .pods
                        .iter()
                        .find(|p| p.metadata.name.as_deref() == Some(pod))
                });
                let Some(found) = found else {
                    return Err(not_found(&key));
                };
                let ephemeral = found
                    .spec
                    .as_ref()
                    .and_then(|s| s.ephemeral_containers.as_ref())
                    .is_some_and(|list| list.iter().any(|c| c.name == request.container));
                let host = found
                    .spec
                    .as_ref()
                    .and_then(|s| s.node_name.clone())
                    .filter(|_| {
                        request.command.first().map(String::as_str) == Some("nsenter")
                            && request.command.iter().any(|a| a == "sh")
                    });
                let shell = (ephemeral || !state.without_shell.contains(&key))
                    && matches!(
                        request.command.first().map(String::as_str),
                        Some("/bin/sh" | "sh")
                    );
                drop(state);
                host.map_or_else(
                    || shell.then(|| "$ ".to_owned()),
                    |node| Some(format!("root@{node}:~# ")),
                )
            };
            let (input, shell_input) = tokio::io::duplex(4096);
            let (mut shell_output, output) = tokio::io::duplex(4096);
            let (exited, exit) = tokio::sync::oneshot::channel();
            tokio::spawn(async move {
                let status = match has_shell {
                    Some(prompt) => echo_shell(shell_input, &mut shell_output, &prompt).await,
                    None => ExitStatus::CommandNotFound,
                };
                drop(shell_output);
                let _receiver_gone = exited.send(status);
            });
            let state = Arc::clone(&self.state);
            Ok(ExecSession {
                input: Box::pin(input),
                output: Box::pin(output),
                resize: Box::new(move |size| {
                    if let Ok(mut state) = state.lock() {
                        state.terminal_sizes.push(size);
                    }
                }),
                exit: Box::pin(async move {
                    exit.await.unwrap_or_else(|_closed| {
                        ExitStatus::Failed("the in-memory shell stopped".to_owned())
                    })
                }),
            })
        })
    }

    /// Records the run ([`State::runs`]); refused as `exec` is, and `sh` is the only command found.
    fn run<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        container: &'a str,
        command: &'a [String],
        input: &'a [u8],
    ) -> Answer<'a, ExitStatus> {
        Box::pin(async move {
            let key = format!("{namespace}/{pod}");
            let mut state = self.allowed(Some(namespace))?;
            if state.exec_refused.contains(&key) {
                return Err(forbidden(&format!("pods/exec {key}")));
            }
            let exists = state.objects.get(namespace).is_some_and(|objects| {
                objects
                    .pods
                    .iter()
                    .any(|p| p.metadata.name.as_deref() == Some(pod))
            });
            if !exists {
                return Err(not_found(&key));
            }
            state.runs.push(Run {
                pod: key,
                container: container.to_owned(),
                command: command.to_vec(),
                input: input.to_vec(),
            });
            drop(state);
            Ok(
                if matches!(command.first().map(String::as_str), Some("/bin/sh" | "sh")) {
                    ExitStatus::Exited(0)
                } else {
                    ExitStatus::CommandNotFound
                },
            )
        })
    }

    /// Runs one of [`crate::files`]' commands on the pod's [`State::files`] once standard input
    /// has ended; refused as `exec` is, and not found in a container of a pod of
    /// [`State::without_shell`] that is not an ephemeral container.
    fn stream<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        container: &'a str,
        command: &'a [String],
    ) -> Answer<'a, ExecStream> {
        Box::pin(async move {
            use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
            let key = format!("{namespace}/{pod}");
            let found = {
                let mut state = self.allowed(Some(namespace))?;
                if state.exec_refused.contains(&key) {
                    return Err(forbidden(&format!("pods/exec {key}")));
                }
                let found = state.objects.get(namespace).and_then(|objects| {
                    objects
                        .pods
                        .iter()
                        .find(|p| p.metadata.name.as_deref() == Some(pod))
                });
                let Some(found) = found else {
                    return Err(not_found(&key));
                };
                let ephemeral = found
                    .spec
                    .as_ref()
                    .and_then(|s| s.ephemeral_containers.as_ref())
                    .is_some_and(|list| list.iter().any(|c| c.name == container));
                let has_tools = ephemeral || !state.without_shell.contains(&key);
                state
                    .streams
                    .push((format!("{key}/{container}"), command.to_vec()));
                drop(state);
                has_tools
            };
            let (input, mut process_input) = tokio::io::duplex(64 * 1024);
            let (mut process_output, output) = tokio::io::duplex(64 * 1024);
            let (mut process_error, error) = tokio::io::duplex(64 * 1024);
            let (exited, exit) = tokio::sync::oneshot::channel();
            let state = Arc::clone(&self.state);
            let command = command.to_vec();
            tokio::spawn(async move {
                let mut stdin = Vec::new();
                let _read = process_input.read_to_end(&mut stdin).await;
                let status = if found {
                    let mut files = state.lock().map_or_else(
                        |_| MemoryFiles::default(),
                        |s| s.files.get(&key).cloned().unwrap_or_default(),
                    );
                    let (status, out, err) = files.run(&command, &stdin).await;
                    if let Ok(mut state) = state.lock() {
                        state.files.insert(key, files);
                    }
                    let _out = process_output.write_all(&out).await;
                    let _err = process_error.write_all(&err).await;
                    status
                } else {
                    ExitStatus::CommandNotFound
                };
                drop(process_output);
                drop(process_error);
                let _receiver_gone = exited.send(status);
            });
            Ok(ExecStream {
                input: Box::pin(input),
                output: Box::pin(output),
                error: Box::pin(error),
                exit: Box::pin(async move {
                    exit.await.unwrap_or_else(|_closed| {
                        ExitStatus::Failed("the in-memory process stopped".to_owned())
                    })
                }),
            })
        })
    }

    fn reveal_secret<'a>(
        &'a self,
        namespace: &'a str,
        name: &'a str,
    ) -> Answer<'a, BTreeMap<String, Vec<u8>>> {
        Box::pin(async move {
            let key = format!("{namespace}/{name}");
            let mut state = self.allowed(Some(namespace))?;
            state.secret_reads += 1;
            state
                .secret_values
                .get(&key)
                .cloned()
                .ok_or_else(|| not_found(&key))
        })
    }

    fn cluster_facts(&self) -> Answer<'_, ClusterFacts> {
        Box::pin(async move { Ok(self.allowed(None)?.facts.clone()) })
    }

    fn cluster_pods(&self, limit: usize) -> Answer<'_, ClusterPods> {
        Box::pin(async move {
            let all: Vec<_> = self
                .wide("pods")?
                .objects
                .values()
                .flat_map(|o| o.pods.clone())
                .collect();
            let mut pods = Vec::new();
            for pod in &all {
                let phase = pod.status.as_ref().and_then(|s| s.phase.as_deref());
                if matches!(phase, Some("Succeeded" | "Failed")) {
                    continue;
                }
                let value =
                    serde_json::to_value(pod).map_err(|e| ClusterError::Decode(e.to_string()))?;
                pods.push(crate::wide::slim_pod(value)?);
            }
            let truncated = pods.len() > limit;
            pods.truncate(limit);
            Ok(ClusterPods { pods, truncated })
        })
    }

    fn cluster_events(&self, limit: usize) -> Answer<'_, ClusterEvents> {
        Box::pin(async move {
            let mut events: Vec<Event> = self
                .wide("events.events.k8s.io")?
                .events
                .values()
                .flatten()
                .cloned()
                .collect();
            let truncated = events.len() > limit;
            events.truncate(limit);
            Ok(ClusterEvents { events, truncated })
        })
    }

    fn serves<'a>(&'a self, group_version: &'a str) -> Answer<'a, Option<bool>> {
        Box::pin(async move { Ok(Some(self.lock()?.serves(group_version))) })
    }

    fn served_resources(&self) -> Answer<'_, Vec<ServedResource>> {
        Box::pin(async move {
            let mut served = builtin_resources();
            served.extend(self.lock()?.served.iter().cloned());
            served.sort();
            Ok(served)
        })
    }

    fn list_objects<'a>(
        &'a self,
        resource: &'a ResourceRef,
        namespace: Option<&'a str>,
    ) -> Answer<'a, Option<Vec<DynamicObject>>> {
        Box::pin(async move {
            // Not installed is known from discovery before any permission matters, as on a cluster.
            if !self.lock()?.serves(&resource.api_version()) {
                return Ok(None);
            }
            let mut items = self
                .readable(resource, namespace)?
                .objects_of(resource, namespace)?;
            for object in &mut items {
                hide_secret_values(resource, object);
            }
            Ok(Some(items))
        })
    }

    fn list_metadata<'a>(
        &'a self,
        resource: &'a ResourceRef,
        namespace: Option<&'a str>,
    ) -> Answer<'a, Option<Vec<ObjectMeta>>> {
        Box::pin(async move {
            Ok(self.list_objects(resource, namespace).await?.map(|items| {
                items
                    .into_iter()
                    .map(|o| {
                        let mut metadata = o.metadata;
                        hide_metadata(resource, &mut metadata);
                        metadata
                    })
                    .collect()
            }))
        })
    }

    fn list_served_metadata<'a>(
        &'a self,
        resource: &'a ResourceRef,
        namespace: &'a str,
    ) -> Answer<'a, Option<Vec<ObjectMeta>>> {
        Box::pin(async move { self.list_metadata(resource, Some(namespace)).await })
    }

    fn access_rules<'a>(&'a self, namespace: &'a str) -> Answer<'a, AccessRules> {
        Box::pin(async move {
            // The review itself is open to everyone; what it says follows this cluster's refusals.
            // A Namespace whose writes are refused leaves the person reading everything in it.
            let (refused, read_only) = {
                let state = self.lock()?;
                if let Some(rules) = state.access_rules.get(namespace) {
                    return rules.clone().ok_or_else(|| {
                        ClusterError::Status(KubernetesStatus::new(
                            500,
                            "InternalError",
                            "the review failed",
                        ))
                    });
                }
                (
                    state.forbid_everything || state.forbidden_namespaces.contains(namespace),
                    state
                        .forbidden_writes
                        .contains(&format!("Namespace/{namespace}")),
                )
            };
            Ok(if refused {
                AccessRules::default()
            } else if read_only {
                AccessRules {
                    rules: vec![AccessRule {
                        verbs: vec!["get".to_owned(), "list".to_owned(), "watch".to_owned()],
                        groups: vec!["*".to_owned()],
                        resources: vec!["*".to_owned()],
                        names: Vec::new(),
                    }],
                    incomplete: false,
                }
            } else {
                AccessRules::everything()
            })
        })
    }

    fn list_kind<'a>(
        &'a self,
        resource: &'a ResourceRef,
        namespace: Option<&'a str>,
    ) -> Answer<'a, Vec<DynamicObject>> {
        Box::pin(async move {
            let state = self.readable(resource, namespace)?;
            let mut items = match (resource.group.as_str(), resource.resource.as_str()) {
                ("apps", "replicasets") => in_namespace(&state.replica_sets, namespace)?,
                ("apps", "controllerrevisions") => {
                    in_namespace(&state.controller_revisions, namespace)?
                }
                ("", "resourcequotas") => in_namespace(&state.resource_quotas, namespace)?,
                ("storage.k8s.io", "storageclasses") => match &state.storage_classes {
                    Some(classes) => all(classes)?,
                    None => return Err(forbidden(&resource.key())),
                },
                _ => state.objects_of(resource, namespace)?,
            };
            drop(state);
            for object in &mut items {
                hide_secret_values(resource, object);
            }
            Ok(items)
        })
    }

    fn get_object<'a>(
        &'a self,
        resource: &'a ResourceRef,
        namespace: Option<&'a str>,
        name: &'a str,
    ) -> Answer<'a, Option<DynamicObject>> {
        Box::pin(async move {
            let mut resource = resource.clone();
            if resource.kind.is_empty() {
                resource.kind = self
                    .lock()?
                    .kind_of(&resource.api_version(), &resource.resource);
            }
            let resource = &resource;
            let Some(items) = self.list_objects(resource, namespace).await? else {
                return Ok(None);
            };
            items
                .into_iter()
                .find(|o| o.metadata.name.as_deref() == Some(name))
                .map(Some)
                .ok_or_else(|| not_found(&format!("{} {name}", resource.key())))
        })
    }

    // ── Writes ───────────────────────────────────────────────────────────────────────────────

    /// Writes as kube-apiserver does, as far as Zeabur's writes reach (checked against k3s by
    /// the write contract, `tests/contract/`):
    ///
    /// - A creation is refused with `409 AlreadyExists` when the name is taken, and stored with
    ///   `metadata.generation` 1 for the kinds that keep one (`GENERATION_KINDS`).
    /// - A JSON Patch is applied to the object held, all or nothing: a failed operation, `test` or
    ///   other, is answered `422 Invalid`; an object that does not exist, `404 NotFound`.
    /// - A field the kind's schema does not know is refused as `fieldValidation=Strict` refuses it:
    ///   `400 BadRequest` for a creation, `422 Invalid` for a patch [k8s-validation].
    /// - A deletion is refused with `409 Conflict` when its preconditions do not hold, `404
    ///   NotFound` when the object does not exist; otherwise the object is taken away at once.
    ///   Nothing is garbage collected: what the object owns stays.
    /// - A dry run stops there. Otherwise the object is stored with the next `resourceVersion` (and
    ///   a `uid` when it is new), and the next `generation` when its `spec` changed
    ///   [k8s-generation], and the write is recorded in [`State::writes`].
    ///
    /// A Secret's values are held apart ([`State::secret_values`]), so a patch of a Secret has
    /// its `test` operations on `metadata` checked and is otherwise only recorded.
    ///
    /// [k8s-validation]: https://kubernetes.io/docs/reference/using-api/api-concepts/#field-validation
    /// [k8s-generation]: https://github.com/kubernetes/community/blob/master/contributors/devel/sig-architecture/api-conventions.md#metadata
    fn write<'a>(&'a self, request: &'a WriteRequest) -> Answer<'a, Written> {
        Box::pin(async move {
            let target = &request.target;
            // kube-apiserver authorises a request for `namespaces/{name}` in namespace `name`
            // (`RequestInfoFactory`, k8s.io/apiserver/pkg/endpoints/request/requestinfo.go).
            let namespace = if target.kind == "Namespace" {
                &target.name
            } else {
                &target.namespace
            };
            let mut state = self.allowed(Some(namespace))?;
            if state.cluster_wide_denied && creates_namespace(request) {
                return Err(forbidden("namespaces"));
            }
            let what = format!("{}/{}", target.kind, target.name);
            if state.forbidden_writes.contains(&what) {
                return Err(forbidden(&what));
            }
            if let Some(refusal) = state.refusal(&what, request.dry_run) {
                return Err(ClusterError::Status(refusal));
            }
            let current = state.stored(target)?;
            let version = current
                .as_ref()
                .and_then(|o| o.pointer("/metadata/resourceVersion"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            if let WriteBody::Delete(preconditions) = &request.body {
                let Some(current) = current else {
                    return Err(not_found(&format!("{} \"{}\"", target.plural, target.name)));
                };
                check_preconditions(target, &current, preconditions)?;
                if request.dry_run {
                    state.dry_runs += 1;
                } else {
                    state.remove(target);
                    if target.kind == "Secret" {
                        state
                            .secret_values
                            .remove(&format!("{}/{}", target.namespace, target.name));
                    }
                    state.writes.push(request.clone());
                }
                drop(state);
                return Ok(Written {
                    resource_version: version,
                });
            }
            let mut object = match (&request.body, &current) {
                (WriteBody::Delete(_), _) => return Err(not_applied()),
                (WriteBody::Create(_), Some(_)) => {
                    return Err(ClusterError::Status(KubernetesStatus::new(
                        409,
                        "AlreadyExists",
                        format!("{} \"{}\" already exists", target.plural, target.name),
                    )));
                }
                (WriteBody::Create(body), None) => created(target, body)?,
                (WriteBody::EphemeralContainers(_), _)
                    if state.without_ephemeral_containers || target.kind != "Pod" =>
                {
                    return Err(not_found(
                        "the server could not find the requested resource",
                    ));
                }
                (WriteBody::JsonPatch(_) | WriteBody::EphemeralContainers(_), None) => {
                    return Err(not_found(&format!("{} \"{}\"", target.plural, target.name)));
                }
                (WriteBody::JsonPatch(operations), Some(current)) => {
                    patched(target, current, operations)?
                }
                (WriteBody::EphemeralContainers(operations), Some(current)) => {
                    with_ephemeral_containers(current, operations)?
                }
            };
            if request.dry_run {
                state.dry_runs += 1;
                drop(state);
                return Ok(Written {
                    resource_version: version,
                });
            }
            let next = version
                .as_deref()
                .and_then(|v| v.parse::<u64>().ok())
                .map_or_else(|| "1".to_owned(), |v| v.saturating_add(1).to_string());
            if let Some(metadata) = object.get_mut("metadata").and_then(Value::as_object_mut) {
                metadata.insert("resourceVersion".to_owned(), Value::String(next.clone()));
                if !metadata.contains_key("uid") {
                    state.uids = state.uids.saturating_add(1);
                    metadata.insert(
                        "uid".to_owned(),
                        Value::String(format!("uid-{}", state.uids)),
                    );
                }
            }
            state.store(target, object)?;
            state.writes.push(request.clone());
            drop(state);
            Ok(Written {
                resource_version: Some(next),
            })
        })
    }

    // ── Reads for releases, storage and YAML edits ───────────────────────────────────────────

    /// Allowed unless every call is refused, the namespace is, the request is cluster-wide and
    /// [`State::cluster_wide_denied`], it is among [`State::refused_rights`], or it names an
    /// object whose writes are refused ([`State::forbidden_writes`]); admission refusals are
    /// allowed, as on a cluster.
    fn review_access<'a>(&'a self, request: &'a Denied) -> Answer<'a, bool> {
        Box::pin(async move {
            let state = self.lock()?;
            let namespace = request.namespace.as_deref();
            let refused_object = request.name.as_ref().is_some_and(|name| {
                state.forbidden_writes.iter().any(|key| {
                    key.rsplit_once('/')
                        .is_some_and(|(_, refused)| refused == name)
                })
            });
            let refused_right = state
                .refused_rights
                .contains(&format!("{} {}", request.verb, request.resource));
            Ok(!(state.forbid_everything
                || refused_right
                || (namespace.is_none() && state.cluster_wide_denied)
                || namespace.is_some_and(|n| state.forbidden_namespaces.contains(n))
                || refused_object))
        })
    }

    /// The Service's [`ProxyAnswer`]; `404` as kube-apiserver gives for a Service that does not
    /// exist.
    fn proxy_get<'a>(
        &'a self,
        service: &'a ServicePort,
        path_and_query: &'a str,
    ) -> Answer<'a, Value> {
        Box::pin(async move {
            let mut state = self.allowed(Some(&service.namespace))?;
            state
                .proxy_requests
                .push(format!("{service}{path_and_query}"));
            let answer = state
                .proxied
                .get(&service.to_string())
                .copied()
                .ok_or_else(|| not_found(&format!("services \"{}\"", service.name)));
            drop(state);
            answer?(path_and_query).map_err(ClusterError::Status)
        })
    }
}

/// The kinds kube-apiserver keeps a `metadata.generation` for, of those Zeabur writes: the
/// workloads and Ingresses, not ConfigMaps, Services or PersistentVolumeClaims.
const GENERATION_KINDS: [&str; 6] = [
    "Deployment",
    "StatefulSet",
    "DaemonSet",
    "Job",
    "CronJob",
    "Ingress",
];

/// The shell of [`MemoryCluster`]'s `exec`, until `exit` or the end of `input`. Write errors
/// end it as end of input does: nobody is reading any more.
async fn echo_shell(
    mut input: impl tokio::io::AsyncRead + Unpin,
    output: &mut (impl tokio::io::AsyncWrite + Unpin),
    prompt: &str,
) -> ExitStatus {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let mut line = Vec::new();
    let mut buffer = [0_u8; 1024];
    if output.write_all(prompt.as_bytes()).await.is_err() {
        return ExitStatus::Exited(0);
    }
    loop {
        let read = match input.read(&mut buffer).await {
            Ok(0) | Err(_) => return ExitStatus::Exited(0),
            Ok(read) => read,
        };
        let mut echo = Vec::new();
        for &byte in buffer.iter().take(read) {
            if byte != b'\r' && byte != b'\n' {
                line.push(byte);
                echo.push(byte);
                continue;
            }
            let text = String::from_utf8_lossy(&line).trim().to_owned();
            line.clear();
            if let Some(code) = text.strip_prefix("exit") {
                let _closing = output.write_all(&echo).await;
                let _closing = output.write_all(b"\r\n").await;
                return ExitStatus::Exited(code.trim().parse().unwrap_or(0));
            }
            if text.is_empty() {
                echo.extend_from_slice(format!("\r\n{prompt}").as_bytes());
            } else {
                echo.extend_from_slice(format!("\r\n{text}\r\n{prompt}").as_bytes());
            }
        }
        if output.write_all(&echo).await.is_err() {
            return ExitStatus::Exited(0);
        }
    }
}

/// Whether the dynamic object `object` is the one `target` names.
fn is_target(object: &DynamicObject, target: &ObjectTarget) -> bool {
    object
        .types
        .as_ref()
        .is_some_and(|t| t.api_version == target.api_version && t.kind == target.kind)
        && object.metadata.namespace.as_deref() == Some(target.namespace.as_str())
        && object.metadata.name.as_deref() == Some(target.name.as_str())
}

fn json<T: Serialize>(object: &T) -> Result<Value, ClusterError> {
    serde_json::to_value(object).map_err(|e| ClusterError::Decode(e.to_string()))
}

fn decode<T: DeserializeOwned>(object: Value) -> Result<T, ClusterError> {
    serde_json::from_value(object).map_err(|e| ClusterError::Decode(e.to_string()))
}

/// The object called `name` in `items`, as JSON.
fn find<T>(items: &[T], name: &str) -> Result<Option<Value>, ClusterError>
where
    T: Metadata<Ty = ObjectMeta> + Serialize,
{
    items
        .iter()
        .find(|o| o.metadata().name.as_deref() == Some(name))
        .map(json)
        .transpose()
}

/// Puts `object` in `items` in place of the object called `name`, or after the others.
/// A `POST` of a Namespace, which kube-apiserver authorises cluster-wide.
fn creates_namespace(request: &WriteRequest) -> bool {
    request.target.kind == "Namespace" && matches!(request.body, WriteBody::Create(_))
}

fn put<T>(items: &mut Vec<T>, name: &str, object: Value) -> Result<(), ClusterError>
where
    T: Metadata<Ty = ObjectMeta> + DeserializeOwned,
{
    let object: T = decode(object)?;
    match items
        .iter_mut()
        .find(|o| o.metadata().name.as_deref() == Some(name))
    {
        Some(slot) => *slot = object,
        None => items.push(object),
    }
    Ok(())
}

/// A new object as kube-apiserver stores it: in the target's namespace, with generation 1 for
/// the kinds that keep one; refused (`400`) when its kind's schema does not know a field.
fn created(target: &ObjectTarget, body: &Value) -> Result<Value, ClusterError> {
    if let Some(field) = unknown_field(&target.kind, body) {
        return Err(ClusterError::Status(KubernetesStatus::new(
            400,
            "BadRequest",
            format!(
                "{} in version \"{}\" cannot be handled as a {}: {field}",
                target.kind, target.api_version, target.kind
            ),
        )));
    }
    let mut object = body.clone();
    if let Some(metadata) = object.get_mut("metadata").and_then(Value::as_object_mut) {
        metadata.insert(
            "namespace".to_owned(),
            Value::String(target.namespace.clone()),
        );
        if GENERATION_KINDS.contains(&target.kind.as_str()) {
            metadata.insert("generation".to_owned(), Value::from(1));
        }
    }
    Ok(object)
}

/// A written Secret's `stringData`, kept as its values for [`Cluster::reveal_secret`].
fn keep_string_data(state: &mut State, target: &ObjectTarget, object: &Value) {
    let values: BTreeMap<String, Vec<u8>> = object
        .get("stringData")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.as_bytes().to_vec())))
        .collect();
    if !values.is_empty() {
        state
            .secret_values
            .insert(format!("{}/{}", target.namespace, target.name), values);
    }
}

/// `current` after the JSON Patch `operations`, with the next generation when its `spec`
/// changed; `422` when an operation fails or the result has a field its kind does not know.
fn patched(
    target: &ObjectTarget,
    current: &Value,
    operations: &Value,
) -> Result<Value, ClusterError> {
    let operations = operations.as_array().map_or(&[][..], Vec::as_slice);
    if target.kind == "Secret" {
        // The values are not here to patch; only what a plan tests of the metadata is checked.
        let tests: Vec<Value> = operations
            .iter()
            .filter(|op| {
                op.get("op").and_then(Value::as_str) == Some("test")
                    && op
                        .get("path")
                        .and_then(Value::as_str)
                        .is_some_and(|p| p.starts_with("/metadata/"))
            })
            .cloned()
            .collect();
        return apply_patch(current, &tests).map_err(|_dropped_by_kube_apiserver| not_applied());
    }
    let mut object =
        apply_patch(current, operations).map_err(|_dropped_by_kube_apiserver| not_applied())?;
    if let Some(field) = unknown_field(&target.kind, &object) {
        return Err(invalid(&field));
    }
    if object.get("spec") != current.get("spec")
        && let Some(generation) = object.pointer_mut("/metadata/generation")
        && let Some(value) = generation.as_i64()
    {
        *generation = Value::from(value.saturating_add(1));
    }
    Ok(object)
}

/// `current` with the ephemeral containers the JSON Patch `operations` make, as the
/// `ephemeralcontainers` subresource keeps them [k8s-pod-strategy]: every other change is
/// dropped, and `422` when the patch does not apply, a container already there changes or goes,
/// or a new one has no image or a name another container of the pod has.
///
/// [k8s-pod-strategy]: https://github.com/kubernetes/kubernetes/blob/master/pkg/registry/core/pod/strategy.go
fn with_ephemeral_containers(current: &Value, operations: &Value) -> Result<Value, ClusterError> {
    let operations = operations.as_array().map_or(&[][..], Vec::as_slice);
    let patched = apply_patch(current, operations).map_err(|_not_applied| not_applied())?;
    let list = |object: &Value, path: &str| -> Vec<Value> {
        object
            .pointer(path)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let before = list(current, "/spec/ephemeralContainers");
    let after = list(&patched, "/spec/ephemeralContainers");
    if after.len() < before.len() || after.iter().zip(&before).any(|(a, b)| a != b) {
        return Err(invalid(
            "spec.ephemeralContainers: Forbidden: existing ephemeral containers may not be \
             changed",
        ));
    }
    let mut names: BTreeSet<String> = ["/spec/containers", "/spec/initContainers"]
        .iter()
        .flat_map(|path| list(current, path))
        .chain(before.iter().cloned())
        .filter_map(|c| c.get("name")?.as_str().map(str::to_owned))
        .collect();
    for added in after.iter().skip(before.len()) {
        let name = added.get("name").and_then(Value::as_str).unwrap_or("");
        if name.is_empty() || !names.insert(name.to_owned()) {
            return Err(invalid(&format!(
                "spec.ephemeralContainers[].name: Duplicate value: {name:?}"
            )));
        }
        if added
            .get("image")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            return Err(invalid("spec.ephemeralContainers[].image: Required value"));
        }
    }
    let mut object = current.clone();
    if let Some(spec) = object.get_mut("spec").and_then(Value::as_object_mut) {
        spec.insert("ephemeralContainers".to_owned(), Value::Array(after));
    }
    if let Some(field) = unknown_field("Pod", &object) {
        return Err(invalid(&field));
    }
    Ok(object)
}

/// Refuses a deletion whose preconditions `current` does not meet, as kube-apiserver does: `409
/// Conflict` [k8s-delete-options] [k8s-store].
///
/// [k8s-delete-options]: https://kubernetes.io/docs/reference/kubernetes-api/common-definitions/delete-options/
/// [k8s-store]: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/apiserver/pkg/registry/generic/registry/store.go
fn check_preconditions(
    target: &ObjectTarget,
    current: &Value,
    preconditions: &Preconditions,
) -> Result<(), ClusterError> {
    let field = |name: &str| {
        current
            .pointer(&format!("/metadata/{name}"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    for (name, wanted) in [
        ("uid", &preconditions.uid),
        ("resourceVersion", &preconditions.resource_version),
    ] {
        if let Some(wanted) = wanted
            && *wanted != field(name)
        {
            return Err(ClusterError::Status(KubernetesStatus::new(
                409,
                "Conflict",
                format!(
                    "Operation cannot be fulfilled on {} \"{}\": Precondition failed: {name} in \
                     precondition: {wanted}, {name} in object meta: {}",
                    target.plural,
                    target.name,
                    field(name)
                ),
            )));
        }
    }
    Ok(())
}

/// kube-apiserver's answer to a JSON Patch it cannot apply (a failed `test`, a `remove` of a
/// path that is gone): `422 Invalid` with a generic message and no details [k8s-patch-handler],
/// as k3s v1.36.5 sends it.
///
/// [k8s-patch-handler]: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/apiserver/pkg/endpoints/handlers/patch.go
fn not_applied() -> ClusterError {
    ClusterError::Status(KubernetesStatus::new(
        422,
        "Invalid",
        "the server rejected our request due to an error in our request",
    ))
}

/// kube-apiserver's answer to a JSON Patch whose result is invalid: `422 Invalid` naming the
/// field in its details [k8s-patch-handler].
///
/// [k8s-patch-handler]: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/apiserver/pkg/endpoints/handlers/patch.go
fn invalid(why: &str) -> ClusterError {
    ClusterError::Status(KubernetesStatus {
        names_invalid_field: true,
        ..KubernetesStatus::new(422, "Invalid", why)
    })
}

/// The first field of `object` that the schema of `kind` does not know, as strict field
/// validation names it (`strict decoding error: unknown field "spec.x"`); `None` when every
/// field is known or the kind is not one the in-memory cluster types.
///
/// The object is read into its k8s-openapi type, which drops unknown fields, and written back:
/// a field that did not survive is unknown.
fn unknown_field(kind: &str, object: &Value) -> Option<String> {
    fn through<T: DeserializeOwned + Serialize>(object: &Value) -> Result<Value, String> {
        let typed: T = serde_json::from_value(object.clone()).map_err(|e| e.to_string())?;
        serde_json::to_value(typed).map_err(|e| e.to_string())
    }
    fn missing(sent: &Value, kept: &Value, path: &str) -> Option<String> {
        match (sent, kept) {
            (Value::Object(sent), Value::Object(kept)) => sent
                .iter()
                .filter(|(_, value)| !value.is_null())
                .find_map(|(key, value)| {
                    let here = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    kept.get(key)
                        .map_or_else(|| Some(here.clone()), |kept| missing(value, kept, &here))
                }),
            (Value::Array(sent), Value::Array(kept)) => sent
                .iter()
                .zip(kept)
                .enumerate()
                .find_map(|(index, (sent, kept))| missing(sent, kept, &format!("{path}[{index}]"))),
            _ => None,
        }
    }
    let kept = match kind {
        "Deployment" => through::<Deployment>(object),
        "StatefulSet" => through::<StatefulSet>(object),
        "DaemonSet" => through::<DaemonSet>(object),
        "Job" => through::<Job>(object),
        "CronJob" => through::<CronJob>(object),
        "Service" => through::<Service>(object),
        "ConfigMap" => through::<ConfigMap>(object),
        "Secret" => through::<Secret>(object),
        "PersistentVolumeClaim" => through::<PersistentVolumeClaim>(object),
        "Ingress" => through::<Ingress>(object),
        "Namespace" => through::<Namespace>(object),
        _ => return None,
    };
    match kept {
        Ok(kept) => missing(object, &kept, "")
            .map(|field| format!("strict decoding error: unknown field \"{field}\"")),
        Err(error) => Some(error),
    }
}

#[cfg(test)]
mod tests {
    use k8s_openapi::api::core::v1::Secret;

    use super::*;

    fn cluster() -> MemoryCluster {
        let secret: Secret = serde_json::from_value(serde_json::json!({
            "metadata": {"name": "db", "namespace": "shop"}
        }))
        .unwrap();
        MemoryCluster::new(State {
            objects: BTreeMap::from([(
                "shop".to_owned(),
                NamespaceObjects {
                    namespace: "shop".to_owned(),
                    secrets: vec![SecretSummary {
                        secret,
                        key_count: 1,
                    }],
                    ..NamespaceObjects::default()
                },
            )]),
            secret_values: BTreeMap::from([(
                "shop/db".to_owned(),
                BTreeMap::from([("password".to_owned(), b"aHVudGVyMg==".to_vec())]),
            )]),
            ..State::default()
        })
    }

    #[tokio::test]
    async fn an_api_that_is_not_served_is_not_installed_even_when_everything_is_refused() {
        let cluster = cluster();
        cluster.update(|s| s.forbid_everything = true);
        let plans = ResourceRef::new("upgrade.cattle.io", "v1", "plans", "Plan");
        assert_eq!(cluster.list_objects(&plans, None).await, Ok(None));
        let secrets = ResourceRef::new("", "v1", "secrets", "Secret");
        assert!(cluster.list_objects(&secrets, Some("shop")).await.is_err());
    }

    #[tokio::test]
    async fn a_cluster_wide_read_can_be_refused_by_resource() {
        let cluster = cluster();
        let node_metrics = ResourceRef::new("metrics.k8s.io", "v1beta1", "nodes", "NodeMetrics");
        cluster.update(|s| {
            s.served.push(ServedResource::readable(
                "metrics.k8s.io",
                "v1beta1",
                "nodes",
                "NodeMetrics",
                false,
            ));
            s.forbidden_resources.insert(node_metrics.key());
        });
        assert_eq!(
            cluster
                .list_objects(&node_metrics, None)
                .await
                .unwrap_err()
                .code(),
            Some(403)
        );
        assert_eq!(
            cluster.serves("metrics.k8s.io/v1beta1").await,
            Ok(Some(true))
        );
    }
}
