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
//! - **A ConfigMap, Secret or claim in use**: the plan names the workloads that still read or mount
//!   it, whose new pods will not start without it; a claim a pod uses stays until no pod does
//!   [k8s-in-use].
//! - **A NetworkPolicy**'s effect on reachability (which services are no longer isolated, which
//!   connections open) is added by the API, which composes the network-policy module.
//!
//! [k8s-endpoints]: https://kubernetes.io/docs/concepts/services-networking/endpoint-slices/
//! [k8s-delete-options]: https://kubernetes.io/docs/reference/kubernetes-api/common-definitions/delete-options/
//! [k8s-gc]: https://kubernetes.io/docs/concepts/architecture/garbage-collection/
//! [k8s-reclaim]: https://kubernetes.io/docs/concepts/storage/persistent-volumes/#reclaiming
//! [k8s-in-use]: https://kubernetes.io/docs/concepts/storage/persistent-volumes/#storage-object-in-use-protection

use std::collections::BTreeSet;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde_json::Value;
use zeabur_adapters::provenance::{Tool, detect_tool};
use zeabur_graph::{pods_of, workloads};
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
        let dependents = self.dependents(&this, meta.uid.as_deref());
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
            let volume = self
                .input
                .snapshot
                .persistent_volume_claim(name)
                .and_then(|c| c.spec.as_ref())
                .and_then(|s| s.volume_name.clone())
                .or_else(|| {
                    object_text(&self.objects, &this, "/spec/volumeName").map(str::to_owned)
                });
            let reclaim_policy = volume
                .as_ref()
                .and_then(|v| self.input.reclaim_policies.get(v))
                .cloned();
            self.warnings.push(Warning::ClaimDataMayBeLost {
                claim: name.to_owned(),
                volume,
                reclaim_policy,
            });
        }
        let users = self.users(&this);
        if !users.is_empty() {
            self.impact.extend(users.iter().cloned());
            self.warnings.push(Warning::StillUsed {
                target: this,
                workloads: users,
            });
        }
        Ok(())
    }

    /// The object to delete, as read; a ConfigMap or Secret the person may list but not get
    /// comes from the project's objects (a Secret without its values).
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
