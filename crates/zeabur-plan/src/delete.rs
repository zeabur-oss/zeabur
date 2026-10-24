//! The planner for deletions: one object of the project, as `kubectl delete` deletes it.
//!
//! - **Which objects.** Any namespaced object of the project the object views list, read as the
//!   person. Refused: Namespaces (`tenant_level`: above a project), RBAC objects (`privilege`),
//!   Gateways (`shared_infrastructure`: every project's routes attach to them), and Endpoints and
//!   `EndpointSlices` (`controller_managed`: the endpoints controllers write them from a Service)
//!   [k8s-endpoints].
//! - **Only the object read.** The write is a `DELETE` whose `DeleteOptions.preconditions` name the
//!   object's `uid` and `resourceVersion` [k8s-delete-options]: an object changed since the plan,
//!   or deleted and made again under the same name, is refused with `409` instead of deleted. A
//!   request may name the `resourceVersion` itself (an Undo of a creation names the version the
//!   creation returned), and is refused when the object has moved past it.
//! - **What goes with it.** Propagation is `Background`, `kubectl delete`'s default: the object
//!   goes at once and the garbage collector deletes what it owns afterwards [k8s-gc]. The plan
//!   lists those dependents from the objects it read, by `ownerReferences` `uid`, owners before
//!   what they own: a Deployment's ReplicaSets and their Pods, a CronJob's Jobs and theirs.
//! - **Objects a tool manages** (Helm, Argo CD, Flux) are refused, naming the tool: Argo CD and
//!   Flux create the object again on their next sync, and a Helm release would still list it, so it
//!   is removed from the chart or the Git repository instead.
//! - **Objects a controller owns** (an `ownerReference` with `controller: true`) are refused,
//!   naming the owner: the controller makes them again (a Deployment its ReplicaSet, a ReplicaSet
//!   its Pods), so deleting or scaling the owner is what removes them. The exception is a Pod:
//!   deleting a Pod its controller owns is how one Pod is replaced, and the plan says a new one
//!   takes its place. A Pod's tool marks are copied from its owner's template, so a controlled Pod
//!   is not refused as managed; the tool manages its owner.
//! - **A PersistentVolumeClaim** may take its data with it: its PersistentVolume's reclaim policy
//!   decides, `Delete` (the volume goes) or `Retain` (it stays, released) [k8s-reclaim]. The plan
//!   names the policy when the person could read the volume, and says it is unknown otherwise.
//! - **A Percona MongoDB** (`PerconaServerMongoDB`) takes its data claims with it, as a
//!   CloudNativePG cluster does through `ownerReferences`. The operator deletes them when the
//!   object carries the finalizer `percona.com/delete-psmdb-pvc` [psmdb-finalizers], and the plan
//!   lists them with what goes with it; without the finalizer the plan deletes each claim itself,
//!   after the object. Backups in object storage stay.
//! - **A `SeaweedFS` store** (`Seaweed`) keeps its claims: the operator's StatefulSets retain them
//!   [seaweed-retain]. The plan deletes each claim itself, after the object.
//! - **A ConfigMap, Secret or claim in use**: the plan names the workloads that still read or mount
//!   it, whose new pods will not start without it; a claim a pod uses stays until no pod does
//!   [k8s-in-use].
//! - **A service** (a workload deleted with `owned`) takes with it what it made and nothing else
//!   uses: its Kubernetes Services, routes, ConfigMaps, Secrets and claims, each deleted the same
//!   way after it (the `owned` module). The workload is not counted among the users of what goes
//!   with it.
//! - **An Ingress** takes the Certificates cert-manager made for it, and their Secrets.
//! - **A NetworkPolicy**'s effect on reachability (which services are no longer isolated, which
//!   connections open) is added by the API, which composes the network-policy module.
//!
//! [k8s-endpoints]: https://kubernetes.io/docs/concepts/services-networking/endpoint-slices/
//! [k8s-delete-options]: https://kubernetes.io/docs/reference/kubernetes-api/common-definitions/delete-options/
//! [k8s-gc]: https://kubernetes.io/docs/concepts/architecture/garbage-collection/
//! [k8s-reclaim]: https://kubernetes.io/docs/concepts/storage/persistent-volumes/#reclaiming
//! [k8s-in-use]: https://kubernetes.io/docs/concepts/storage/persistent-volumes/#storage-object-in-use-protection
//! [psmdb-finalizers]: https://docs.percona.com/percona-operator-for-mongodb/operator.html#metadata
//! [seaweed-retain]: https://github.com/seaweedfs/seaweedfs-operator/blob/seaweedfs-operator-0.1.43/internal/controller/pv_reclaim.go

use std::collections::BTreeSet;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde_json::Value;
use zeabur_adapters::provenance::{Tool, detect_tool};
use zeabur_graph::{Snapshot, pods_of, workloads};
use zeabur_model::{SourceKind, WorkloadKind, WorkloadRef};

use crate::change::{ObjectName, PlannedObject, Target, Warning, Write};
use crate::checks::ResourceRef;
use crate::plan::{ErrorKind, PlanError, Planner, Working, error, shown, to_json, yaml};
use crate::rules::{managed_code, tool_name};

/// Why `resource` is never deleted here, if it is not.
fn refused(resource: &ResourceRef) -> Option<&'static str> {
    match (resource.group.as_str(), resource.resource.as_str()) {
        ("", "namespaces") => Some("tenant_level"),
        ("rbac.authorization.k8s.io", _) => Some("privilege"),
        ("gateway.networking.k8s.io", "gateways" | "gatewayclasses") => {
            Some("shared_infrastructure")
        }
        ("", "endpoints") | ("discovery.k8s.io", "endpointslices") => Some("controller_managed"),
        _ => None,
    }
}

/// The finalizer that has the Percona operator delete a MongoDB's claims with it.
pub const MONGODB_CLAIMS_FINALIZER: &str = "percona.com/delete-psmdb-pvc";

/// The data claims of `PerconaServerMongoDB` `cluster` in `snapshot`, by name.
///
/// They are `mongod-data-<cluster>-<replset>-<n>`, from its replica sets' StatefulSets
/// (`-nv-<n>` and `-hidden-<n>` for non-voting and hidden members), and from its config servers
/// (`cfg`) when sharded. A claim a controller owns is left out: it goes with its owner.
#[must_use]
fn mongodb_data_claims(cluster: &Value, snapshot: &Snapshot) -> Vec<String> {
    let name = cluster
        .pointer("/metadata/name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut replsets: Vec<&str> = cluster
        .pointer("/spec/replsets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|r| r.get("name").and_then(Value::as_str))
        .collect();
    if cluster.pointer("/spec/sharding/enabled") == Some(&Value::Bool(true)) {
        replsets.push("cfg");
    }
    let ordinal = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let of_member = |rest: &str| {
        ordinal(rest)
            || ["nv-", "hidden-"]
                .iter()
                .any(|p| rest.strip_prefix(p).is_some_and(ordinal))
    };
    snapshot
        .persistent_volume_claims
        .iter()
        .filter(|c| zeabur_adapters::controller_of(&c.metadata).is_none())
        .filter_map(|c| c.metadata.name.clone())
        .filter(|claim| {
            replsets.iter().any(|rs| {
                claim
                    .strip_prefix(&format!("mongod-data-{name}-{rs}-"))
                    .is_some_and(of_member)
            })
        })
        .collect()
}

/// The data claims of `Seaweed` `store` in `snapshot`, by name.
///
/// They are `mount<d>-<store>-volume-<n>` for the volume servers, and
/// `<store>-master-<store>-master-<n>` and `<store>-filer-<store>-filer-<n>` when the master and
/// filer persist.
#[must_use]
fn seaweed_data_claims(store: &Value, snapshot: &Snapshot) -> Vec<String> {
    let name = store
        .pointer("/metadata/name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let ordinal = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let volume = format!("-{name}-volume-");
    let parts = [
        format!("{name}-master-{name}-master-"),
        format!("{name}-filer-{name}-filer-"),
    ];
    snapshot
        .persistent_volume_claims
        .iter()
        .filter(|c| zeabur_adapters::controller_of(&c.metadata).is_none())
        .filter_map(|c| c.metadata.name.clone())
        .filter(|claim| {
            let disk = claim.strip_prefix("mount").and_then(|rest| {
                let (d, rest) = rest.split_once(&volume)?;
                (ordinal(d) && ordinal(rest)).then_some(())
            });
            disk.is_some()
                || parts
                    .iter()
                    .any(|p| claim.strip_prefix(p.as_str()).is_some_and(ordinal))
        })
        .collect()
}

/// The data claims that go with deleting `object`, a `resource`, by name.
#[must_use]
pub fn data_claims(resource: &ResourceRef, object: &Value, snapshot: &Snapshot) -> Vec<String> {
    match (resource.group.as_str(), resource.resource.as_str()) {
        ("psmdb.percona.com", "perconaservermongodbs") => mongodb_data_claims(object, snapshot),
        ("seaweed.seaweedfs.com", "seaweeds") => seaweed_data_claims(object, snapshot),
        _ => Vec::new(),
    }
}

/// An object the plan read, as the dependents search sees it.
struct Node {
    name: ObjectName,
    uid: Option<String>,
    owners: Vec<String>,
}

impl Node {
    fn of(kind: &str, meta: &ObjectMeta) -> Self {
        Self {
            name: ObjectName {
                kind: kind.to_owned(),
                name: meta.name.clone().unwrap_or_default(),
            },
            uid: meta.uid.clone(),
            owners: meta
                .owner_references
                .iter()
                .flatten()
                .map(|o| o.uid.clone())
                .collect(),
        }
    }
}

impl Planner<'_> {
    #[expect(clippy::too_many_lines, reason = "one change kind, read top to bottom")]
    pub(crate) fn delete(
        &mut self,
        resource: &ResourceRef,
        name: &str,
        expected: Option<&str>,
    ) -> Result<(), PlanError> {
        if let Some(reason) = refused(resource) {
            return Err(error(
                ErrorKind::Invalid,
                reason,
                format!("{} cannot be deleted here ({reason}).", resource.resource),
            ));
        }
        let object = self.deletable_object(resource, name)?;
        let data_claims = data_claims(resource, &object, self.input.snapshot);
        let meta: ObjectMeta = object
            .get("metadata")
            .cloned()
            .and_then(|m| serde_json::from_value(m).ok())
            .unwrap_or_default();
        let kind = object
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let namespace = &self.input.snapshot.namespace;
        if meta.namespace.as_deref() != Some(namespace.as_str()) {
            return Err(error(
                ErrorKind::Invalid,
                "other_namespace",
                format!("{kind} {name} is not an object of project {namespace}."),
            ));
        }
        let Some(version) = meta.resource_version.as_deref() else {
            return Err(error(
                ErrorKind::Invalid,
                "unreadable_object",
                format!("{kind} {name} has no resourceVersion."),
            ));
        };
        if let Some(expected) = expected
            && expected != version
        {
            return Err(error(
                ErrorKind::Conflict,
                "changed_since",
                format!("{kind} {name} changed after version {expected}; review it again."),
            ));
        }
        let controller = zeabur_adapters::controller_of(&meta);
        match controller {
            Some(owner) if kind == "Pod" => self.warnings.push(Warning::PodReplaced {
                pod: name.to_owned(),
                owner: ObjectName {
                    kind: owner.kind.clone(),
                    name: owner.name.clone(),
                },
            }),
            Some(owner) => {
                return Err(error(
                    ErrorKind::Conflict,
                    "owned_by_controller",
                    format!(
                        "{kind} {name} belongs to {} {}, which would make it again; delete or \
                         scale {} {} instead.",
                        owner.kind, owner.name, owner.kind, owner.name
                    ),
                ));
            }
            None => refuse_managed_deletion(&meta, &kind, name)?,
        }
        let target = Target {
            api_version: resource.api_version(),
            kind: kind.clone(),
            plural: resource.resource.clone(),
            name: name.to_owned(),
        };
        if self.objects.iter().any(|w| w.target == target) {
            return Err(deleted_and_changed(&kind, name));
        }
        self.objects.push(Working {
            target,
            original: Some(object.clone()),
            current: object,
            operations: Vec::new(),
            owners: Vec::new(),
            deleted: true,
        });
        let this = ObjectName {
            kind: kind.clone(),
            name: name.to_owned(),
        };
        let mut dependents = self.dependents(&this, meta.uid.as_deref());
        let operator_deletes_claims = meta
            .finalizers
            .iter()
            .flatten()
            .any(|f| f == MONGODB_CLAIMS_FINALIZER);
        if operator_deletes_claims {
            for claim in &data_claims {
                let claim = ObjectName {
                    kind: "PersistentVolumeClaim".to_owned(),
                    name: claim.clone(),
                };
                if !dependents.contains(&claim) {
                    dependents.push(claim);
                }
            }
        }
        if !dependents.is_empty() {
            self.warnings.push(Warning::DeletesDependents {
                target: this.clone(),
                dependents,
            });
        }
        if let Some(workload) = WorkloadKind::parse(&kind) {
            self.impact.insert(WorkloadRef {
                kind: workload,
                name: name.to_owned(),
            });
        }
        if kind == "PersistentVolumeClaim" {
            self.claim_data_warning(&this);
        }
        if !operator_deletes_claims {
            for claim in &data_claims {
                self.delete_data_claim(claim)?;
            }
        }
        let users: Vec<WorkloadRef> = self
            .users(&this)
            .into_iter()
            .filter(|w| !self.deletes(w.kind.as_str(), &w.name) && self.still_mounts(w, &this))
            .collect();
        if !users.is_empty() {
            self.impact.extend(users.iter().cloned());
            self.warnings.push(Warning::StillUsed {
                target: this,
                workloads: users,
            });
        }
        Ok(())
    }

    /// A `delete` change: the object, then, with `owned`, what the service made.
    pub(crate) fn delete_change(
        &mut self,
        resource: &ResourceRef,
        name: &str,
        expected: Option<&str>,
        owned: bool,
    ) -> Result<(), PlanError> {
        self.delete(resource, name, expected)?;
        if owned {
            self.delete_owned(resource, name)?;
        }
        Ok(())
    }

    /// Deletes what workload `name` made and nothing else uses, after the workload itself, and
    /// names what it uses that stays (the `owned` module).
    fn delete_owned(&mut self, resource: &ResourceRef, name: &str) -> Result<(), PlanError> {
        let Some(workload) = crate::owned::workload_of(resource, name) else {
            return Err(error(
                ErrorKind::BadRequest,
                "owned_of_workload_only",
                format!(
                    "Only a workload's objects are deleted with it; {} is not one.",
                    resource.resource
                ),
            ));
        };
        let owned = crate::owned::owned(self.input.snapshot, self.input.http_routes, &workload);
        let at = self.warnings.len();
        let mut objects = Vec::new();
        for (resource, object) in &owned.deleted {
            let kind = kind_of(resource);
            if self.deletes(kind, object) {
                continue;
            }
            self.delete(resource, object, None)?;
            objects.push(ObjectName {
                kind: kind.to_owned(),
                name: object.clone(),
            });
        }
        if !objects.is_empty() {
            self.warnings.insert(
                at,
                Warning::DeletesOwned {
                    target: ObjectName {
                        kind: workload.kind.as_str().to_owned(),
                        name: name.to_owned(),
                    },
                    objects,
                },
            );
        }
        self.warnings.extend(owned.stays);
        Ok(())
    }

    /// Whether the plan deletes object `kind` `name`.
    fn deletes(&self, kind: &str, name: &str) -> bool {
        self.objects
            .iter()
            .any(|w| w.deleted && w.target.kind == kind && w.target.name == name)
    }

    /// Whether `workload` still mounts claim `target` after the changes before this one. An
    /// earlier `removeVolume` in the plan stops the mount.
    fn still_mounts(&self, workload: &WorkloadRef, target: &ObjectName) -> bool {
        if target.kind != "PersistentVolumeClaim" {
            return true;
        }
        let Some(working) = self
            .objects
            .iter()
            .find(|o| o.target.kind == workload.kind.as_str() && o.target.name == workload.name)
        else {
            return true;
        };
        working
            .current
            .pointer("/spec/template/spec/volumes")
            .and_then(Value::as_array)
            .is_some_and(|volumes| {
                volumes.iter().any(|v| {
                    v.pointer("/persistentVolumeClaim/claimName")
                        .and_then(Value::as_str)
                        == Some(target.name.as_str())
                })
            })
    }

    fn claim_data_warning(&mut self, claim: &ObjectName) {
        let volume = self
            .input
            .snapshot
            .persistent_volume_claim(&claim.name)
            .and_then(|c| c.spec.as_ref())
            .and_then(|s| s.volume_name.clone())
            .or_else(|| object_text(&self.objects, claim, "/spec/volumeName").map(str::to_owned));
        let reclaim_policy = volume
            .as_ref()
            .and_then(|v| self.input.reclaim_policies.get(v))
            .cloned();
        self.warnings.push(Warning::ClaimDataMayBeLost {
            claim: claim.name.clone(),
            volume,
            reclaim_policy,
        });
    }

    /// Deletes claim `name` with the database whose data it holds; the database's pods are
    /// going too, so the claim is not said to be still used.
    fn delete_data_claim(&mut self, name: &str) -> Result<(), PlanError> {
        let target = Target {
            api_version: "v1".to_owned(),
            kind: "PersistentVolumeClaim".to_owned(),
            plural: "persistentvolumeclaims".to_owned(),
            name: name.to_owned(),
        };
        if let Some(working) = self.objects.iter().find(|w| w.target == target) {
            return if working.deleted {
                Ok(())
            } else {
                Err(deleted_and_changed(&target.kind, name))
            };
        }
        let Some(claim) = self.input.snapshot.persistent_volume_claim(name) else {
            return Ok(());
        };
        let object = to_json(claim)?;
        self.objects.push(Working {
            target,
            original: Some(object.clone()),
            current: object,
            operations: Vec::new(),
            owners: Vec::new(),
            deleted: true,
        });
        self.claim_data_warning(&ObjectName {
            kind: "PersistentVolumeClaim".to_owned(),
            name: name.to_owned(),
        });
        Ok(())
    }

    /// The object to delete, as read; one the plan did not read alone (a ConfigMap or Secret
    /// the person may list but not get, or what a service is deleted with) comes from the
    /// project's objects (a Secret without its values).
    fn deletable_object(&self, resource: &ResourceRef, name: &str) -> Result<Value, PlanError> {
        let fetched = self
            .input
            .objects
            .iter()
            .find(|f| &f.resource == resource && f.name == name)
            .map(|f| f.object.clone());
        let listed = match (resource.group.as_str(), resource.resource.as_str()) {
            ("", "configmaps") => self
                .input
                .snapshot
                .config_map(name)
                .map(to_json)
                .transpose()?,
            ("", "secrets") => self.input.snapshot.secret(name).map(to_json).transpose()?,
            ("", "services") => self
                .input
                .snapshot
                .services
                .iter()
                .find(|o| o.metadata.name.as_deref() == Some(name))
                .map(to_json)
                .transpose()?,
            ("networking.k8s.io", "ingresses") => self
                .input
                .snapshot
                .ingresses
                .iter()
                .find(|o| o.metadata.name.as_deref() == Some(name))
                .map(to_json)
                .transpose()?,
            ("", "persistentvolumeclaims") => self
                .input
                .snapshot
                .persistent_volume_claim(name)
                .map(to_json)
                .transpose()?,
            ("gateway.networking.k8s.io", "httproutes") => self
                .input
                .http_routes
                .iter()
                .find(|o| o.pointer("/metadata/name").and_then(Value::as_str) == Some(name))
                .cloned(),
            _ => None,
        };
        // A listed object is typed, so it carries its `apiVersion` and `kind`: k8s-openapi writes
        // them whenever it serialises one.
        fetched.or(listed).ok_or_else(|| {
            error(
                ErrorKind::NotFound,
                "unknown_object",
                format!("There is no {} named {name}.", resource.resource),
            )
        })
    }

    /// What the garbage collector deletes after `target`: the objects read that it owns, and
    /// what they own, owners first. A workload's pods are added by the rule that ties them to it,
    /// for a Deployment whose ReplicaSets could not be read.
    fn dependents(&self, target: &ObjectName, uid: Option<&str>) -> Vec<ObjectName> {
        let snapshot = self.input.snapshot;
        let mut nodes: Vec<Node> = Vec::new();
        let mut add = |kind: &str, meta: &ObjectMeta| nodes.push(Node::of(kind, meta));
        for o in &snapshot.deployments {
            add("Deployment", &o.metadata);
        }
        for o in self.input.replica_sets {
            add("ReplicaSet", &o.metadata);
        }
        for o in &snapshot.stateful_sets {
            add("StatefulSet", &o.metadata);
        }
        for o in &snapshot.daemon_sets {
            add("DaemonSet", &o.metadata);
        }
        for o in &snapshot.cron_jobs {
            add("CronJob", &o.metadata);
        }
        for o in &snapshot.jobs {
            add("Job", &o.metadata);
        }
        for o in &snapshot.pods {
            add("Pod", &o.metadata);
        }
        for o in &snapshot.services {
            add("Service", &o.metadata);
        }
        for o in &snapshot.config_maps {
            add("ConfigMap", &o.metadata);
        }
        for o in &snapshot.secrets {
            add("Secret", &o.metadata);
        }
        for o in &snapshot.persistent_volume_claims {
            add("PersistentVolumeClaim", &o.metadata);
        }
        for o in &snapshot.ingresses {
            add("Ingress", &o.metadata);
        }
        for route in self.input.http_routes {
            if let Some(meta) = route
                .get("metadata")
                .cloned()
                .and_then(|m| serde_json::from_value::<ObjectMeta>(m).ok())
            {
                add("HTTPRoute", &meta);
            }
        }
        let mut out: Vec<ObjectName> = Vec::new();
        let mut level: Vec<String> = uid.map(str::to_owned).into_iter().collect();
        let mut seen: BTreeSet<String> = level.iter().cloned().collect();
        while !level.is_empty() {
            let mut found: Vec<&Node> = nodes
                .iter()
                .filter(|n| n.owners.iter().any(|o| level.contains(o)))
                .filter(|n| n.uid.as_ref().is_none_or(|u| !seen.contains(u)))
                .collect();
            found.sort_by(|a, b| a.name.cmp(&b.name));
            level = found.iter().filter_map(|n| n.uid.clone()).collect();
            seen.extend(level.iter().cloned());
            out.extend(found.into_iter().map(|n| n.name.clone()));
        }
        if target.kind == "Ingress" {
            out.extend(certificates_of(snapshot, &target.name));
        }
        if let Some(workload) = workloads(snapshot)
            .into_iter()
            .find(|w| w.kind().as_str() == target.kind && w.name() == target.name)
        {
            for pod in pods_of(snapshot, workload) {
                let name = ObjectName {
                    kind: "Pod".to_owned(),
                    name: pod.name,
                };
                if !out.contains(&name) {
                    out.push(name);
                }
            }
        }
        out
    }

    /// The workloads that read ConfigMap or Secret `target` (into variables or as files) or
    /// mount claim `target`.
    fn users(&self, target: &ObjectName) -> Vec<WorkloadRef> {
        let source = match target.kind.as_str() {
            "ConfigMap" => Some(SourceKind::ConfigMap),
            "Secret" => Some(SourceKind::Secret),
            "PersistentVolumeClaim" => None,
            _ => return Vec::new(),
        };
        let name = target.name.as_str();
        let mut out: BTreeSet<WorkloadRef> = BTreeSet::new();
        for service in &self.project.services {
            let reads = service.containers.iter().any(|c| {
                source.is_some_and(|kind| {
                    c.env
                        .iter()
                        .any(|e| e.source.kind == kind && e.source.name.as_deref() == Some(name))
                        || c.imported_env.iter().any(|e| {
                            e.source.kind == kind && e.source.name.as_deref() == Some(name)
                        })
                }) || c.mounts.iter().any(|m| {
                    m.object
                        .as_ref()
                        .is_some_and(|o| o.kind == target.kind && o.name == name)
                        || (target.kind == "PersistentVolumeClaim"
                            && m.claim_name.as_deref() == Some(name))
                })
            });
            if reads {
                out.insert(WorkloadRef {
                    kind: service.workload.kind,
                    name: service.workload.name.clone(),
                });
            }
        }
        out.into_iter().collect()
    }
}

/// The Certificates cert-manager made for Ingress `name` and their Secrets: a Certificate is
/// named after the Secret the Ingress lists and owned by the Ingress, and the Secret is owned
/// by its Certificate when cert-manager runs with `--enable-certificate-owner-ref`, as the
/// catalog installs it [cert-manager-ingress].
///
/// [cert-manager-ingress]: https://cert-manager.io/docs/usage/ingress/
fn certificates_of(snapshot: &Snapshot, name: &str) -> Vec<ObjectName> {
    let listed = snapshot
        .ingresses
        .iter()
        .filter(|i| i.metadata.name.as_deref() == Some(name))
        .flat_map(|i| i.spec.iter().flat_map(|s| s.tls.iter().flatten()))
        .filter_map(|t| t.secret_name.as_deref());
    let mut out = Vec::new();
    for secret_name in listed {
        let Some(secret) = snapshot.secret(secret_name) else {
            continue;
        };
        let Some(certificate) =
            zeabur_adapters::controller_of(&secret.metadata).filter(|c| c.kind == "Certificate")
        else {
            continue;
        };
        out.push(ObjectName {
            kind: "Certificate".to_owned(),
            name: certificate.name.clone(),
        });
        out.push(ObjectName {
            kind: "Secret".to_owned(),
            name: secret_name.to_owned(),
        });
    }
    out
}

/// The kind of the objects of `resource`, for the kinds a service is deleted with.
fn kind_of(resource: &ResourceRef) -> &'static str {
    match resource.resource.as_str() {
        "services" => "Service",
        "ingresses" => "Ingress",
        "httproutes" => "HTTPRoute",
        "configmaps" => "ConfigMap",
        "secrets" => "Secret",
        _ => "PersistentVolumeClaim",
    }
}

/// A text field of the object `name` among the working objects.
fn object_text<'v>(objects: &'v [Working], name: &ObjectName, pointer: &str) -> Option<&'v str> {
    objects
        .iter()
        .find(|w| w.target.kind == name.kind && w.target.name == name.name)
        .and_then(|w| w.current.pointer(pointer))
        .and_then(Value::as_str)
}

/// Refuses deleting an object Helm, Argo CD or Flux manages.
fn refuse_managed_deletion(meta: &ObjectMeta, kind: &str, name: &str) -> Result<(), PlanError> {
    detect_tool(meta).map_or(Ok(()), |tool| {
        let place = match tool {
            Tool::Helm => "the release's chart or values",
            Tool::ArgoCd | Tool::Flux => "its Git repository",
        };
        Err(PlanError {
            managed_by: Some(tool),
            ..error(
                ErrorKind::Conflict,
                managed_code(tool),
                format!(
                    "{kind} {name} is managed by {}, which would make it again or still list it; \
                     remove it from {place} instead.",
                    tool_name(tool),
                ),
            )
        })
    })
}

fn deleted_and_changed(kind: &str, name: &str) -> PlanError {
    error(
        ErrorKind::BadRequest,
        "deleted_and_changed",
        format!("A plan cannot both change and delete {kind} {name}."),
    )
}

/// The planned deletion of `working`.
pub(crate) fn deletion(working: Working) -> Result<PlannedObject, PlanError> {
    if !working.operations.is_empty() {
        return Err(deleted_and_changed(
            &working.target.kind,
            &working.target.name,
        ));
    }
    let text = |pointer: &str| {
        working
            .original
            .as_ref()
            .and_then(|o| o.pointer(pointer))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let uid = text("/metadata/uid");
    let resource_version = text("/metadata/resourceVersion").unwrap_or_default();
    let before = working.original.as_ref().map(shown);
    Ok(PlannedObject {
        before_yaml: yaml(before.as_ref()),
        before,
        after: None,
        after_yaml: None,
        changes: Vec::new(),
        owners: Vec::new(),
        resource_version: Some(resource_version.clone()),
        write: Write::Delete {
            uid,
            resource_version,
        },
        change: None,
        raw_after: Value::Null,
        target: working.target,
    })
}
