//! The planner for deleting a project: its Namespace and everything in it, as
//! `kubectl delete namespace <name>` does.
//!
//! - **The writes.** First a `DELETE` of each database object (CloudNativePG `Cluster`, Percona
//!   `PerconaServerMongoDB`, …), then a `DELETE` of the Namespace, each with preconditions on the
//!   object's `uid` and `resourceVersion` (the `delete` module). The namespace controller then
//!   deletes every object in the namespace and removes the Namespace last [k8s-ns-delete].
//! - **Why databases go first.** An operator puts finalizers on its database objects and removes
//!   them when it has cleaned up. If the namespace controller deletes the operator's other objects
//!   in the same pass, or the operator does not run, the finalizers stay and the Namespace stays
//!   `Terminating` [k8s-finalizers]. The API deletes the database objects, waits for them to go,
//!   then deletes the Namespace. When a finalizer stays, the Namespace stays `Terminating`, and the
//!   interface lists the objects that hold it.
//! - **What goes and what stays** is told in one [`Warning::DeletesProject`]: the services, the
//!   databases, the volumes and what their reclaim policy does with the data [k8s-reclaim], and the
//!   person's tools pods for the project. Backups in object storage and everything outside the
//!   namespace stay.
//! - **Tools that put objects back.** Objects Helm, Argo CD or Flux manages are deleted with the
//!   namespace, and the tool can create them again ([`Warning::Recreated`]).
//! - **Refused**: a namespace Kubernetes keeps for itself (`kube-` prefix), Zeabur's own namespaces
//!   (its own, its members', its terminals', its node shells'), a namespace of a component the
//!   installer set up (label [`LABEL_BASELINE`]), a namespace that is not a project, and one that
//!   is already `Terminating`.
//!
//! # Force deletion
//!
//! [`Change::ForceDeleteProject`] is for a project whose Namespace stays `Terminating` because a
//! finalizer is never removed, for example when its controller is gone. Each of its writes is a
//! JSON Patch that sets `metadata.finalizers` to `[]` on one object, after a `test` of the
//! finalizers as read. The plan token pins those finalizers too ([`versions`]), so a change to them
//! after the review makes the apply refuse. It writes only an object of [`DeletionInput::held`]
//! that is in the project's namespace and already has a `deletionTimestamp`. It never writes the
//! Namespace, so its own `spec.finalizers` stay. The namespace controller then removes what is left
//! and the Namespace. [`Warning::RemovesFinalizers`] names each object and its finalizers. Refused:
//! the same namespaces as a deletion, a namespace that is not being deleted, and one with no such
//! object.
//!
//! [`Change::ForceDeleteProject`]: crate::Change::ForceDeleteProject
//! [`versions`]: crate::versions
//!
//! [k8s-ns-delete]: https://kubernetes.io/docs/tasks/administer-cluster/namespaces/#deleting-a-namespace
//! [k8s-finalizers]: https://kubernetes.io/docs/concepts/overview/working-with-objects/finalizers/
//! [k8s-reclaim]: https://kubernetes.io/docs/concepts/storage/persistent-volumes/#reclaiming

use std::collections::BTreeMap;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zeabur_graph::workloads;
use zeabur_model::WorkloadRef;
use zeabur_model::adapters::provenance::{Tool, detect_tool};
use zeabur_model::kernel::names::{LABEL_BASELINE, NODE_SHELL_NAMESPACE, is_system_namespace};

use crate::access::meta_of;
use crate::change::{ObjectName, Target, Warning};
use crate::checks::ResourceRef;
use crate::json::{replace, test};
use crate::plan::{ErrorKind, PlanError, Planner, Working, error};

/// An object of the project, as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectObject {
    /// Its resource.
    pub resource: ResourceRef,
    /// The object, with `kind` and `metadata`.
    pub object: Value,
}

/// What deleting the project reads beyond the project's objects.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeletionInput {
    /// Zeabur's own namespaces besides the node shells': its own, its members', its terminals'.
    pub zeabur_namespaces: Vec<String>,
    /// The project's database objects.
    pub databases: Vec<ProjectObject>,
    /// The `reclaimPolicy` of each `StorageClass` the person could read, by name.
    pub class_reclaim: BTreeMap<String, String>,
    /// The cluster's default `StorageClass`, when known.
    pub default_class: Option<String>,
    /// The namespace of tools pods.
    pub tools_namespace: String,
    /// The person's tools pods for the project, by name.
    pub tools_pods: Vec<String>,
    /// For a force deletion: the objects with finalizers among the content the namespace
    /// controller says is left.
    pub held: Vec<ProjectObject>,
}

/// An object whose finalizers a force deletion removes ([`Warning::RemovesFinalizers`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct HeldBy {
    /// The kind.
    pub kind: String,
    /// The name.
    pub name: String,
    /// The finalizers removed.
    pub finalizers: Vec<String>,
}

/// What deleting the project takes with it ([`Warning::DeletesProject`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ProjectDeletion {
    /// The project's Namespace.
    pub namespace: String,
    /// Its services: workloads no other object controls.
    pub services: Vec<WorkloadRef>,
    /// Its databases, deleted before the Namespace.
    pub databases: Vec<DatabaseDeletion>,
    /// Its volumes.
    pub volumes: Vec<VolumeDeletion>,
    /// The namespace of tools pods.
    pub tools_namespace: String,
    /// The person's tools pods for the project, deleted after the Namespace.
    pub tools_pods: Vec<String>,
}

/// A database that goes with the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct DatabaseDeletion {
    /// Its kind (`Cluster`).
    pub kind: String,
    /// Its name.
    pub name: String,
}

/// A volume that goes with the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct VolumeDeletion {
    /// The PersistentVolumeClaim.
    pub claim: String,
    /// Its PersistentVolume; absent while the claim is not bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<String>,
    /// Its `StorageClass`; absent when it names none and no default is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_class: Option<String>,
    /// What happens to the data: the PersistentVolume's `persistentVolumeReclaimPolicy` when the
    /// person could read it, else the `StorageClass`'s `reclaimPolicy` (`Delete` when it sets
    /// none); absent when neither could be read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reclaim_policy: Option<String>,
}

fn refusal(code: &str, detail: String) -> PlanError {
    error(ErrorKind::SystemNamespace, code, detail)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "the planner's change kinds are split across modules by area"
)]
impl<'a> Planner<'a> {
    pub(crate) fn delete_project(&mut self) -> Result<(), PlanError> {
        let empty = DeletionInput::default();
        let deletion = self.input.deletion.unwrap_or(&empty);
        let (object, meta) = self.project_namespace(deletion)?;
        let name = meta.name.clone().unwrap_or_default();
        if meta.deletion_timestamp.is_some() {
            return Err(error(
                ErrorKind::Conflict,
                "already_deleting",
                format!("{name} is already being deleted."),
            ));
        }
        let snapshot = self.input.snapshot;
        for database in &deletion.databases {
            let target = Target {
                api_version: database.resource.api_version(),
                kind: text(&database.object, "/kind"),
                plural: database.resource.resource.clone(),
                name: text(&database.object, "/metadata/name"),
            };
            self.push_deletion(target, database.object.clone());
        }
        self.push_deletion(Target::namespace(&name), object.clone());

        let services: Vec<WorkloadRef> = workloads(snapshot)
            .into_iter()
            .filter(|w| zeabur_model::adapters::controller_of(w.meta()).is_none())
            .map(zeabur_graph::Workload::reference)
            .collect();
        let volumes = snapshot
            .persistent_volume_claims
            .iter()
            .map(|claim| {
                let volume = claim.spec.as_ref().and_then(|s| s.volume_name.clone());
                let storage_class = claim
                    .spec
                    .as_ref()
                    .and_then(|s| s.storage_class_name.clone())
                    .or_else(|| deletion.default_class.clone());
                let reclaim_policy = volume
                    .as_ref()
                    .and_then(|v| self.input.reclaim_policies.get(v).cloned())
                    .or_else(|| {
                        storage_class
                            .as_ref()
                            .and_then(|c| deletion.class_reclaim.get(c).cloned())
                    });
                VolumeDeletion {
                    claim: claim.metadata.name.clone().unwrap_or_default(),
                    volume,
                    storage_class,
                    reclaim_policy,
                }
            })
            .collect();
        let databases = deletion
            .databases
            .iter()
            .map(|d| DatabaseDeletion {
                kind: text(&d.object, "/kind"),
                name: text(&d.object, "/metadata/name"),
            })
            .collect();
        self.warnings.push(Warning::DeletesProject(ProjectDeletion {
            namespace: name,
            services,
            databases,
            volumes,
            tools_namespace: deletion.tools_namespace.clone(),
            tools_pods: deletion.tools_pods.clone(),
        }));
        self.warnings.extend(recreated(&meta, snapshot, deletion));
        Ok(())
    }

    pub(crate) fn force_delete_project(&mut self) -> Result<(), PlanError> {
        let empty = DeletionInput::default();
        let deletion = self.input.deletion.unwrap_or(&empty);
        let (_, meta) = self.project_namespace(deletion)?;
        let name = meta.name.clone().unwrap_or_default();
        if meta.deletion_timestamp.is_none() {
            return Err(error(
                ErrorKind::Conflict,
                "not_deleting",
                format!("{name} is not being deleted; delete the project first."),
            ));
        }
        let mut removed = Vec::new();
        for held in &deletion.held {
            let object = &held.object;
            let meta = meta_of(object);
            let finalizers = meta.finalizers.unwrap_or_default();
            let in_project = meta.namespace.as_deref() == Some(name.as_str());
            if finalizers.is_empty() || !in_project || meta.deletion_timestamp.is_none() {
                continue;
            }
            let target = Target {
                api_version: held.resource.api_version(),
                kind: text(object, "/kind"),
                plural: held.resource.resource.clone(),
                name: meta.name.unwrap_or_default(),
            };
            removed.push(HeldBy {
                kind: target.kind.clone(),
                name: target.name.clone(),
                finalizers: finalizers.clone(),
            });
            let index = self.working(target, object.clone());
            self.patch(
                index,
                vec![
                    test("/metadata/finalizers", json!(finalizers)),
                    replace("/metadata/finalizers", json!([])),
                ],
            )?;
        }
        if removed.is_empty() {
            return Err(error(
                ErrorKind::Conflict,
                "nothing_held",
                format!("No object you can read in {name} is being deleted and has finalizers."),
            ));
        }
        self.warnings
            .push(Warning::RemovesFinalizers { objects: removed });
        Ok(())
    }

    /// The project's Namespace as read, refused when it is not a project of the person's to
    /// delete.
    fn project_namespace(
        &self,
        deletion: &DeletionInput,
    ) -> Result<(&'a Value, ObjectMeta), PlanError> {
        let object = self.input.namespace.ok_or_else(|| {
            error(
                ErrorKind::NotFound,
                "unknown_namespace",
                "The namespace could not be read.",
            )
        })?;
        let meta: ObjectMeta =
            serde_json::from_value(object.get("metadata").cloned().unwrap_or(Value::Null))
                .map_err(|e| error(ErrorKind::Invalid, "unreadable_object", e.to_string()))?;
        let name = meta.name.clone().unwrap_or_default();
        refuse_namespace(&name, &meta, deletion, &self.conventions.project_label)?;
        Ok((object, meta))
    }

    fn push_deletion(&mut self, target: Target, object: Value) {
        self.objects.push(Working {
            target,
            current: object.clone(),
            original: Some(object),
            operations: Vec::new(),
            owners: Vec::new(),
            deleted: true,
        });
    }
}

fn text(object: &Value, pointer: &str) -> String {
    object
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// Refuses a namespace that is not a project of the person's to delete.
fn refuse_namespace(
    name: &str,
    meta: &ObjectMeta,
    deletion: &DeletionInput,
    project_label: &str,
) -> Result<(), PlanError> {
    let label = |key: &str| meta.labels.as_ref().and_then(|l| l.get(key));
    if is_system_namespace(name) {
        return Err(refusal(
            "system_namespace",
            format!("{name} holds Kubernetes' own components; it cannot be deleted here."),
        ));
    }
    if name == NODE_SHELL_NAMESPACE || deletion.zeabur_namespaces.iter().any(|n| n == name) {
        return Err(refusal(
            "zeabur_namespace",
            format!("Zeabur itself runs in {name}; it cannot be deleted here."),
        ));
    }
    if label(LABEL_BASELINE).is_some_and(|v| v == "true") {
        return Err(refusal(
            "component_namespace",
            format!("{name} holds a component the installer set up; remove the component instead."),
        ));
    }
    if label(project_label).is_none() {
        return Err(error(
            ErrorKind::Conflict,
            "not_adopted",
            format!("{name} is not a project."),
        ));
    }
    Ok(())
}

/// The objects Helm, Argo CD or Flux manages, by tool: the tool can create them again.
fn recreated(
    namespace: &ObjectMeta,
    snapshot: &zeabur_graph::Snapshot,
    deletion: &DeletionInput,
) -> Vec<Warning> {
    let mut by_tool: Vec<(Tool, Vec<ObjectName>)> = Vec::new();
    let mut note = |kind: &str, meta: &ObjectMeta| {
        let Some(tool) = detect_tool(meta) else {
            return;
        };
        let object = ObjectName::of(kind, meta.name.as_deref().unwrap_or_default());
        match by_tool.iter_mut().find(|(t, _)| *t == tool) {
            Some((_, objects)) => objects.push(object),
            None => by_tool.push((tool, vec![object])),
        }
    };
    note("Namespace", namespace);
    for workload in workloads(snapshot) {
        note(workload.kind().as_str(), workload.meta());
    }
    for database in &deletion.databases {
        if let Ok(meta) = serde_json::from_value::<ObjectMeta>(
            database
                .object
                .get("metadata")
                .cloned()
                .unwrap_or(Value::Null),
        ) {
            note(&text(&database.object, "/kind"), &meta);
        }
    }
    for service in &snapshot.services {
        note("Service", &service.metadata);
    }
    for ingress in &snapshot.ingresses {
        note("Ingress", &ingress.metadata);
    }
    for map in &snapshot.config_maps {
        note("ConfigMap", &map.metadata);
    }
    for secret in &snapshot.secrets {
        note("Secret", &secret.metadata);
    }
    for claim in &snapshot.persistent_volume_claims {
        note("PersistentVolumeClaim", &claim.metadata);
    }
    by_tool
        .into_iter()
        .map(|(tool, objects)| Warning::Recreated { tool, objects })
        .collect()
}
