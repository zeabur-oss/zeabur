//! The planner for deleting a project: its Namespace and everything in it, as
//! `kubectl delete namespace <name>` does.
//!
//! - **The writes.** First a `DELETE` of each database object (CloudNativePG `Cluster`, Percona
//!   `PerconaServerMongoDB`, …) with foreground propagation, then a `DELETE` of the Namespace, each
//!   with preconditions on the object's `uid` and `resourceVersion` (the `delete` module). The
//!   namespace controller then deletes every object in the namespace and removes the Namespace last
//!   [k8s-ns-delete].
//! - **Why databases go first.** An operator puts finalizers on its database objects and removes
//!   them when it has cleaned up. If the namespace controller deletes the operator's other objects
//!   in the same pass, or the operator does not run, the finalizers stay and the Namespace stays
//!   `Terminating` [k8s-finalizers]. The API deletes the database objects, waits for them to go,
//!   then deletes the Namespace. When a finalizer stays, the Namespace stays `Terminating`, and the
//!   interface lists the objects that hold it.
//! - **Why in the foreground.** With foreground propagation a database object stays until the
//!   garbage collector has deleted the pods it owns [k8s-gc-foreground], so the wait for the object
//!   is a wait for its pods too. A database pod can have a long grace period (1800s for
//!   CloudNativePG), and the namespace controller checks a Namespace again only after half the
//!   longest grace period of its pods, or at its 5-minute resync [k8s-ns-controller]. Without the
//!   pods, the Namespace goes within seconds.
//! - **What goes and what stays** is told in one [`Warning::DeletesProject`]: the services, the
//!   databases, the volumes and what their reclaim policy does with the data [k8s-reclaim], and the
//!   person's tools pods for the project. It also tells where the project's backups are kept: a
//!   backup whose S3 endpoint is a Service of the project (`<service>`, `<service>.<project>`,
//!   `<service>.<project>.svc…` or its cluster IP) is kept in the project and goes with it; a
//!   backup whose store or Service could not be read is told as unknown, never as staying; any
//!   other backup and everything outside the namespace stay.
//! - **Tools that put objects back.** Objects Helm, Argo CD or Flux manages are deleted with the
//!   namespace, and the tool can create them again ([`Warning::Recreated`]).
//! - **Refused**: a namespace Kubernetes keeps for itself (`kube-` prefix), Zeabur's own namespaces
//!   (its own, its members', its terminals', its node shells'), a namespace of a platform component
//!   (label [`LABEL_BASELINE`]), a namespace that is not a project, and one that is already
//!   `Terminating`.
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
//! [k8s-gc-foreground]: https://kubernetes.io/docs/concepts/architecture/garbage-collection/#foreground-deletion
//! [k8s-ns-controller]: https://github.com/kubernetes/kubernetes/blob/master/pkg/controller/namespace/namespace_controller.go

use std::collections::BTreeMap;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zeabur_graph::workloads;
use zeabur_model::WorkloadRef;
use zeabur_model::adapters::provenance::{Tool, detect_tool};
use zeabur_model::kernel::names::{LABEL_BASELINE, admin_only_namespace, is_system_namespace};

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
    /// The project's backups.
    pub backups: Vec<ProjectBackup>,
    /// Whether listing the project's Services was refused: an endpoint that names no Service
    /// read is then unknown, not outside.
    pub services_unread: bool,
}

/// What a backup covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum BackupKind {
    /// A database.
    Database,
    /// A volume's files.
    Volume,
    /// The project's Kubernetes objects.
    Project,
}

/// A backup of the project, as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectBackup {
    /// What it covers.
    pub kind: BackupKind,
    /// The database, the claim or the project.
    pub name: String,
    /// Where it writes.
    pub store: BackupStore,
}

/// Where a backup writes, as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupStore {
    /// AWS S3: no endpoint is set.
    Aws,
    /// The S3 endpoint.
    Endpoint(String),
    /// The provider's store could not be read, or does not name an endpoint.
    Unknown,
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
    /// Whether one of them is Kubernetes' own (`kubernetes.io/...`): its controller still
    /// runs, so the Pod or node that holds the object is the better thing to fix.
    pub kubernetes: bool,
    /// Whether one of them belongs to anything else.
    pub third_party: bool,
}

impl HeldBy {
    fn new(kind: String, name: String, finalizers: Vec<String>) -> Self {
        let kubernetes = |f: &String| f.starts_with("kubernetes.io/");
        Self {
            kubernetes: finalizers.iter().any(kubernetes),
            third_party: !finalizers.iter().all(kubernetes),
            kind,
            name,
            finalizers,
        }
    }
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
    /// The project's backups, and where each is kept.
    pub backups: Vec<BackupDeletion>,
}

/// A backup of the project, and whether it goes with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct BackupDeletion {
    /// What it covers.
    pub kind: BackupKind,
    /// The database, the claim or the project.
    pub name: String,
    /// Whether it goes with the project.
    pub fate: BackupFate,
    /// When it goes: what in the project keeps it, the owner of the Service its endpoint names,
    /// else the Service.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept_in: Option<String>,
}

/// Whether a backup goes with the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum BackupFate {
    /// It is kept outside the project.
    Stays,
    /// It is kept in the project's object storage.
    Goes,
    /// Where it is kept could not be read.
    Unknown,
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
            self.push_deletion(target, database.object.clone(), true);
        }
        self.push_deletion(Target::namespace(&name), object.clone(), false);

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
        let backups = deletion
            .backups
            .iter()
            .map(|b| {
                let (fate, kept_in) = match &b.store {
                    BackupStore::Aws => (BackupFate::Stays, None),
                    BackupStore::Unknown => (BackupFate::Unknown, None),
                    BackupStore::Endpoint(e) => {
                        fate_of(e, &name, snapshot, deletion.services_unread)
                    }
                };
                BackupDeletion {
                    kind: b.kind,
                    name: b.name.clone(),
                    fate,
                    kept_in,
                }
            })
            .collect();
        self.warnings.push(Warning::DeletesProject(ProjectDeletion {
            namespace: name,
            services,
            databases,
            volumes,
            tools_namespace: deletion.tools_namespace.clone(),
            tools_pods: deletion.tools_pods.clone(),
            backups,
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
            removed.push(HeldBy::new(
                target.kind.clone(),
                target.name.clone(),
                finalizers.clone(),
            ));
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

    fn push_deletion(&mut self, target: Target, object: Value, foreground: bool) {
        self.objects.push(Working {
            target,
            current: object.clone(),
            original: Some(object),
            operations: Vec::new(),
            owners: Vec::new(),
            deleted: true,
            foreground,
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

/// Whether a backup at `endpoint` goes with the project, and what in the project serves it: the
/// owner of the project's Service it names, else the Service. A short name (`files-s3`,
/// `files-s3.shop`) or a cluster IP goes only when that Service was read; when the Services were
/// not read it is unknown. `files-s3.shop.svc` and longer names the project by its DNS form
/// [k8s-dns].
///
/// [k8s-dns]: https://kubernetes.io/docs/concepts/services-networking/dns-pod-service/#services
fn fate_of(
    endpoint: &str,
    project: &str,
    snapshot: &zeabur_graph::Snapshot,
    services_unread: bool,
) -> (BackupFate, Option<String>) {
    let rest = endpoint.split_once("://").map_or(endpoint, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = authority.strip_prefix('[').map_or_else(
        || authority.split(':').next().unwrap_or_default(),
        |v6| v6.split(']').next().unwrap_or_default(),
    );
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let named = |name: &str| {
        snapshot
            .services
            .iter()
            .find(|s| s.metadata.name.as_deref() == Some(name))
    };
    let looked_up = |found: Option<&k8s_openapi::api::core::v1::Service>| match found {
        Some(service) => (BackupFate::Goes, Some(owner_or_name(service))),
        None if services_unread => (BackupFate::Unknown, None),
        None => (BackupFate::Stays, None),
    };
    if host.parse::<std::net::IpAddr>().is_ok() {
        let found = snapshot.services.iter().find(|s| {
            s.spec.as_ref().is_some_and(|spec| {
                spec.cluster_ips
                    .iter()
                    .flatten()
                    .chain(&spec.cluster_ip)
                    .any(|ip| *ip == host)
            })
        });
        return looked_up(found);
    }
    let labels: Vec<&str> = host.split('.').collect();
    match labels.as_slice() {
        [service] => looked_up(named(service)),
        [service, namespace] if *namespace == project => looked_up(named(service)),
        [service, namespace, "svc", ..] if *namespace == project => (
            BackupFate::Goes,
            Some(named(service).map_or_else(|| (*service).to_owned(), owner_or_name)),
        ),
        _ => (BackupFate::Stays, None),
    }
}

fn owner_or_name(service: &k8s_openapi::api::core::v1::Service) -> String {
    zeabur_model::adapters::controller_of(&service.metadata).map_or_else(
        || service.metadata.name.clone().unwrap_or_default(),
        |owner| owner.name.clone(),
    )
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
    if admin_only_namespace(name).is_some() || deletion.zeabur_namespaces.iter().any(|n| n == name)
    {
        return Err(refusal(
            "zeabur_namespace",
            format!("Zeabur itself runs in {name}; it cannot be deleted here."),
        ));
    }
    if label(LABEL_BASELINE).is_some_and(|v| v == "true") {
        return Err(refusal(
            "component_namespace",
            format!("{name} holds a platform component; remove the component instead."),
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
