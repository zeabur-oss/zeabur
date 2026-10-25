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
//!   `Terminating` [k8s-finalizers]. The API deletes the database objects, waits while their
//!   operators run, then deletes the Namespace. A database whose operator has no ready controller
//!   gets [`OperatorState::NotReady`], and the plan says so.
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
//! The plan never removes a finalizer.
//!
//! [k8s-ns-delete]: https://kubernetes.io/docs/tasks/administer-cluster/namespaces/#deleting-a-namespace
//! [k8s-finalizers]: https://kubernetes.io/docs/concepts/overview/working-with-objects/finalizers/
//! [k8s-reclaim]: https://kubernetes.io/docs/concepts/storage/persistent-volumes/#reclaiming

use std::collections::BTreeMap;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeabur_adapters::provenance::{Tool, detect_tool};
use zeabur_graph::workloads;
use zeabur_kernel::names::{LABEL_BASELINE, NODE_SHELL_NAMESPACE, is_system_namespace};
use zeabur_model::WorkloadRef;

use crate::change::{ObjectName, Target, Warning};
use crate::checks::ResourceRef;
use crate::plan::{ErrorKind, PlanError, Planner, Working, error};

/// Whether the operator of a database runs, as far as the person can see.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum OperatorState {
    /// A controller Deployment of the operator has an available replica.
    Ready,
    /// No controller Deployment of the operator has an available replica.
    NotReady,
    /// The person may not read the operator's Deployments.
    Unknown,
}

/// A database object of the project, as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseObject {
    /// Its resource.
    pub resource: ResourceRef,
    /// The object, with `metadata`.
    pub object: Value,
    /// Whether its operator runs.
    pub operator: OperatorState,
}

/// What deleting the project reads beyond the project's objects.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeletionInput {
    /// Zeabur's own namespaces besides the node shells': its own, its members', its terminals'.
    pub zeabur_namespaces: Vec<String>,
    /// The project's database objects.
    pub databases: Vec<DatabaseObject>,
    /// The `reclaimPolicy` of each `StorageClass` the person could read, by name.
    pub class_reclaim: BTreeMap<String, String>,
    /// The cluster's default `StorageClass`, when known.
    pub default_class: Option<String>,
    /// The namespace of tools pods.
    pub tools_namespace: String,
    /// The person's tools pods for the project, by name.
    pub tools_pods: Vec<String>,
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
    /// Whether its operator runs. Only while it runs does the apply wait for the database to go.
    pub operator: OperatorState,
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
impl Planner<'_> {
    pub(crate) fn delete_project(&mut self) -> Result<(), PlanError> {
        let empty = DeletionInput::default();
        let deletion = self.input.deletion.unwrap_or(&empty);
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
            .filter(|w| zeabur_adapters::controller_of(w.meta()).is_none())
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
                operator: d.operator,
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
    if meta.deletion_timestamp.is_some() {
        return Err(error(
            ErrorKind::Conflict,
            "already_deleting",
            format!("{name} is already being deleted."),
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
