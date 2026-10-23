//! From a request to a plan: the writes, what they change, who owns those fields now, which
//! workloads it affects, and what to know first.
//!
//! # How each change is written
//!
//! One method for every change to an existing object, and one for every new object:
//!
//! - **An existing object** gets one RFC 6902 JSON Patch [rfc6902]. Its first operation tests
//!   `metadata.resourceVersion`; every value it replaces or removes is tested first, and every edit
//!   of a list entry tests that the entry is the one meant. JSON Patch is atomic, so the patch
//!   applies whole or not at all, and a patch made against an older object fails its tests: of many
//!   applies of one plan, one wins. A restart sets the `kubectl.kubernetes.io/restartedAt` template
//!   annotation as `kubectl rollout restart` does [kubectl-restart] (testing the old value when
//!   there is one), joining the workload's other edits so one object gets one write.
//! - **A new object** (an Ingress or HTTPRoute for a new domain, a claim for a new volume) is
//!   created whole with `POST`, which fails if an object of that name exists by then.
//! - Both are sent with `fieldManager=zeabur` [k8s-field-manager], so `managedFields` names
//!   `zeabur` as the one that changed those fields, after a dry run with `fieldValidation=Strict`.
//!   Server-side apply is not used: it can only remove a field the applier owns, and Ingress
//!   `rules` and `HTTPRoute` `hostnames` are atomic lists, so applying one entry would take the
//!   whole list from its owner.
//! - **A deletion** is a `DELETE` with background propagation and preconditions on the object's
//!   `uid` and `resourceVersion` (the `delete` module).
//! - **Objects Helm, Argo CD or Flux manage** may be restarted but not edited or deleted: the tool
//!   would restore the edit on its next sync, so the edit belongs in its values or Git repository.
//!   A field one of them owns is never changed ([`crate::fields::reapplying_tool`]).
//!
//! [rfc6902]: https://www.rfc-editor.org/rfc/rfc6902
//! [k8s-field-manager]: https://kubernetes.io/docs/reference/using-api/server-side-apply/#field-management
//! [kubectl-restart]: https://github.com/kubernetes/kubectl/blob/master/pkg/polymorphichelpers/objectrestarter.go

use std::collections::{BTreeMap, BTreeSet};

use base64::Engine as _;
use k8s_openapi::api::apps::v1::ReplicaSet;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde_json::{Value, json};
use zeabur_adapters::provenance::{Tool, detect_tool};
use zeabur_graph::{Snapshot, Workload, workloads};
use zeabur_kernel::names::{Conventions, GatewayRef};
use zeabur_model::{HttpRoute, PodSecurityLevel, Project, SourceKind, WorkloadKind, WorkloadRef};

use crate::change::{
    Change, DataEdit, DataKind, FieldOwners, HttpOnlyReason, Plan, PlanRequest, PlannedObject,
    RouteKind, RouteRef, Target, Warning, Write,
};
use crate::checks::ResourceRef;
use crate::create::ServedKind;
use crate::fields::{Step, owners, reapplying_tool};
use crate::json::{add, apply_patch, pointer, remove, replace, test};
use crate::releases::Release;
use crate::rules::{
    EnvEdit, diff, env_entry, managed_code, route_name, tool_name, validate_env, validate_hostname,
};

/// The kinds a plan may create, as `(apiVersion, kind, plural)`.
///
/// Creation is kept to kinds a person asks for through Zeabur's own forms; a kind joins the list
/// with the form that needs it:
///
/// - ready-made NetworkPolicy objects;
/// - a database and its backups: CloudNativePG's `Cluster`, `ScheduledBackup` and `Backup`, the
///   Barman Cloud plugin's `ObjectStore`, the Percona Operator for MongoDB's `PerconaServerMongoDB`
///   and `PerconaServerMongoDBBackup`, MOCO's `MySQLCluster` and `BackupPolicy`, mariadb-operator's
///   `MariaDB` and `Backup`, the Dragonfly Operator's `Dragonfly`, and the Secret holding the
///   backup bucket's access keys or the database's passwords;
/// - object storage: the `SeaweedFS` Operator's `Seaweed`, `Bucket`, and the `S3Identity`,
///   `S3Credentials`, `S3Policy` and `S3PolicyBinding` that give services an access key;
/// - a restore into a new database: Percona's `PerconaServerMongoDBRestore` (the other operators
///   restore from fields of the new database);
/// - a volume's backups: K8up's `Schedule`, `Backup` and `PodConfig`, and the Secret with the
///   bucket's keys and the repository password;
/// - a service from a container image: its Deployment, Service, Ingress, volume claim and Secrets;
/// - a service from Git: the same, and Zeabur Builder's `Build`.
///
/// The object is created as the person, so kube-apiserver's RBAC and admission decide, as for
/// `kubectl create`; the plan only keeps it in the project's namespace.
pub const CREATABLE: &[(&str, &str, &str)] = &[
    ("networking.k8s.io/v1", "NetworkPolicy", "networkpolicies"),
    ("v1", "Secret", "secrets"),
    ("v1", "PersistentVolumeClaim", "persistentvolumeclaims"),
    ("v1", "Service", "services"),
    ("apps/v1", "Deployment", "deployments"),
    ("batch/v1", "Job", "jobs"),
    ("networking.k8s.io/v1", "Ingress", "ingresses"),
    ("builder.zeabur.com/v1alpha1", "Build", "builds"),
    ("postgresql.cnpg.io/v1", "Cluster", "clusters"),
    (
        "postgresql.cnpg.io/v1",
        "ScheduledBackup",
        "scheduledbackups",
    ),
    ("postgresql.cnpg.io/v1", "Backup", "backups"),
    ("barmancloud.cnpg.io/v1", "ObjectStore", "objectstores"),
    (
        "psmdb.percona.com/v1",
        "PerconaServerMongoDB",
        "perconaservermongodbs",
    ),
    (
        "psmdb.percona.com/v1",
        "PerconaServerMongoDBBackup",
        "perconaservermongodbbackups",
    ),
    (
        "psmdb.percona.com/v1",
        "PerconaServerMongoDBRestore",
        "perconaservermongodbrestores",
    ),
    ("moco.cybozu.com/v1beta2", "MySQLCluster", "mysqlclusters"),
    ("moco.cybozu.com/v1beta2", "BackupPolicy", "backuppolicies"),
    ("k8s.mariadb.com/v1alpha1", "MariaDB", "mariadbs"),
    ("k8s.mariadb.com/v1alpha1", "Backup", "backups"),
    ("dragonflydb.io/v1alpha1", "Dragonfly", "dragonflies"),
    ("seaweed.seaweedfs.com/v1", "Seaweed", "seaweeds"),
    ("seaweed.seaweedfs.com/v1", "Bucket", "buckets"),
    ("seaweed.seaweedfs.com/v1", "S3Identity", "s3identities"),
    ("seaweed.seaweedfs.com/v1", "S3Credentials", "s3credentials"),
    ("seaweed.seaweedfs.com/v1", "S3Policy", "s3policies"),
    (
        "seaweed.seaweedfs.com/v1",
        "S3PolicyBinding",
        "s3policybindings",
    ),
    ("k8up.io/v1", "Schedule", "schedules"),
    ("k8up.io/v1", "Backup", "backups"),
    ("k8up.io/v1", "PodConfig", "podconfigs"),
];

/// The plural of a kind [`CREATABLE`] lists.
#[must_use]
pub fn creatable(api_version: &str, kind: &str) -> Option<&'static str> {
    CREATABLE
        .iter()
        .find(|(v, k, _)| *v == api_version && *k == kind)
        .map(|(_, _, plural)| *plural)
}

/// The annotation `kubectl rollout restart` sets on the pod template.
pub const RESTARTED_AT: &str = "kubectl.kubernetes.io/restartedAt";

/// What a plan is computed from.
#[derive(Debug, Clone, Copy)]
pub struct Input<'a> {
    /// The project's objects, as read with the person's credential.
    pub snapshot: &'a Snapshot,
    /// The project's `HTTPRoute` objects, as JSON.
    pub http_routes: &'a [Value],
    /// The cluster's Gateways; `None` when they could not be listed.
    pub gateways: Option<&'a [GatewayRef]>,
    /// The values of the Secrets the request edits, by name, where the person could read them.
    pub secret_values: &'a BTreeMap<String, BTreeMap<String, Vec<u8>>>,
    /// The time the plan is made, RFC 3339; the restart annotation's value.
    pub now: &'a str,
    /// The releases of the workloads the request rolls back.
    pub releases: &'a BTreeMap<WorkloadRef, Vec<Release>>,
    /// The cluster's `StorageClasses`; `None` when they could not be listed.
    pub storage_classes: Option<&'a [StorageClassInfo]>,
    /// The objects the request edits as YAML or deletes, as read.
    pub objects: &'a [Fetched],
    /// The project's ReplicaSets, when the request deletes a Deployment: what it owns besides
    /// its pods.
    pub replica_sets: &'a [ReplicaSet],
    /// The reclaim policy of each PersistentVolume the person could read, by volume name, for
    /// the claims the request deletes.
    pub reclaim_policies: &'a BTreeMap<String, String>,
    /// The project's Namespace, as read, with `managedFields`: needed to adopt it, let it go
    /// or change its details; `None` when the request does none of these.
    pub namespace: Option<&'a Value>,
    /// Whether Stakater Reloader runs in the cluster; `None` when that could not be read.
    pub reloader: Option<bool>,
    /// The kinds the cluster serves, from discovery: what a `create` may make. Empty: only the
    /// kinds [`CREATABLE`] lists.
    pub kinds: &'a [ServedKind],
    /// The `ClusterIssuer` the chart names for new hostnames (`domains.clusterIssuer`); `None`:
    /// a new hostname gets HTTPS only where its Ingress already asks cert-manager for it.
    pub cluster_issuer: Option<&'a str>,
}

impl<'a> Input<'a> {
    /// An input of `snapshot` at `now` and nothing else: no `HTTPRoutes`, Gateways, Secret
    /// values, releases, `StorageClasses`, objects, ReplicaSets, reclaim policies or Namespace.
    /// The rest is set with struct update syntax.
    #[must_use]
    pub fn new(snapshot: &'a Snapshot, now: &'a str) -> Self {
        static NO_SECRET_VALUES: BTreeMap<String, BTreeMap<String, Vec<u8>>> = BTreeMap::new();
        static NO_RELEASES: BTreeMap<WorkloadRef, Vec<Release>> = BTreeMap::new();
        static NO_RECLAIM_POLICIES: BTreeMap<String, String> = BTreeMap::new();
        Self {
            snapshot,
            http_routes: &[],
            gateways: Some(&[]),
            secret_values: &NO_SECRET_VALUES,
            now,
            releases: &NO_RELEASES,
            storage_classes: Some(&[]),
            objects: &[],
            replica_sets: &[],
            reclaim_policies: &NO_RECLAIM_POLICIES,
            namespace: None,
            reloader: None,
            kinds: &[],
            cluster_issuer: None,
        }
    }
}

/// What a plan needs to know of a `StorageClass`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageClassInfo {
    /// Its name.
    pub name: String,
    /// `allowVolumeExpansion`: its claims may grow [k8s-expand].
    ///
    /// [k8s-expand]: https://kubernetes.io/docs/concepts/storage/persistent-volumes/#expanding-persistent-volumes-claims
    pub allow_expansion: bool,
    /// It is the cluster's default (`storageclass.kubernetes.io/is-default-class`).
    pub default: bool,
}

/// An object read for a YAML edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    /// Its resource.
    pub resource: ResourceRef,
    /// Its name.
    pub name: String,
    /// The object, with `managedFields`.
    pub object: Value,
}

/// Why a plan cannot be made.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{code}: {detail}")]
pub struct PlanError {
    /// A stable `snake_case` code.
    pub code: String,
    /// What kind of failure it is.
    pub kind: ErrorKind,
    /// What happened, for a person.
    pub detail: String,
    /// The tool that manages the object or owns the field, when that is why it was refused.
    pub managed_by: Option<Tool>,
    /// The index of the change it concerns in the request.
    pub change: Option<usize>,
    /// The line of that change's YAML it concerns, from 1.
    pub line: Option<u64>,
    /// The field it concerns, as kube-apiserver writes it (`metadata.namespace`).
    pub field: Option<String>,
}

/// The kinds of [`PlanError`], which map onto problem kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// The request is malformed.
    BadRequest,
    /// Something it names does not exist.
    NotFound,
    /// It conflicts with who manages the object or what is already there.
    Conflict,
    /// It asks for something the object cannot take.
    Invalid,
    /// It would adopt a namespace Kubernetes keeps for itself.
    SystemNamespace,
}

pub(crate) fn error(kind: ErrorKind, code: &str, detail: impl Into<String>) -> PlanError {
    PlanError {
        code: code.to_owned(),
        kind,
        detail: detail.into(),
        managed_by: None,
        change: None,
        line: None,
        field: None,
    }
}

/// One object being changed.
pub(crate) struct Working {
    pub(crate) target: Target,
    /// The object as read, with `managedFields`; `None` for a creation.
    pub(crate) original: Option<Value>,
    /// The object after the edits so far.
    pub(crate) current: Value,
    pub(crate) operations: Vec<Value>,
    pub(crate) owners: Vec<FieldOwners>,
    /// The write deletes the object.
    pub(crate) deleted: bool,
}

pub(crate) struct Planner<'a> {
    pub(crate) input: Input<'a>,
    pub(crate) conventions: &'a Conventions,
    pub(crate) project: Project,
    pub(crate) objects: Vec<Working>,
    pub(crate) restarts: BTreeSet<WorkloadRef>,
    pub(crate) impact: BTreeSet<WorkloadRef>,
    pub(crate) warnings: Vec<Warning>,
    /// The objects `create` changes make, with the index of their change.
    pub(crate) created: Vec<(Target, usize)>,
    /// The level a `podSecurity` change asks the project to enforce.
    pub(crate) pod_security: Option<PodSecurityLevel>,
}

/// The plan for `request` against `input`.
///
/// # Errors
///
/// When a change names something that does not exist, edits an object Helm, Argo CD or Flux
/// manages, touches a field one of them owns, or is not valid.
pub fn plan(input: Input<'_>, request: &PlanRequest) -> Result<Plan, PlanError> {
    plan_with(input, request, &Conventions::ZEABUR)
}

/// [`plan`] under the labels, annotations and field manager of `conventions`.
///
/// # Errors
///
/// As [`plan`].
pub fn plan_with(
    input: Input<'_>,
    request: &PlanRequest,
    conventions: &Conventions,
) -> Result<Plan, PlanError> {
    if request.changes.is_empty() {
        return Err(error(
            ErrorKind::BadRequest,
            "no_changes",
            "The plan has no changes.",
        ));
    }
    let namespace_changes = request
        .changes
        .iter()
        .filter(|c| c.is_namespace_change())
        .count();
    if namespace_changes > 0 && namespace_changes < request.changes.len() {
        return Err(error(
            ErrorKind::BadRequest,
            "namespace_change_alone",
            "Adopting a namespace, letting it go and a project's details are planned apart from \
             changes to the objects in it.",
        ));
    }
    if request.changes.len() > 1
        && request
            .changes
            .iter()
            .any(|c| matches!(c, Change::CreateProject { .. }))
    {
        return Err(error(
            ErrorKind::BadRequest,
            "create_project_alone",
            "A new project is planned on its own; add what goes in it once it exists.",
        ));
    }
    let mut planner = Planner {
        input,
        conventions,
        project: zeabur_graph::project_with(input.snapshot, conventions),
        objects: Vec::new(),
        restarts: BTreeSet::new(),
        impact: BTreeSet::new(),
        warnings: Vec::new(),
        created: Vec::new(),
        pod_security: None,
    };
    for index in crate::create::order(&request.changes) {
        let Some(change) = request.changes.get(index) else {
            continue;
        };
        planner.change(change, index).map_err(|e| PlanError {
            change: Some(index),
            ..e
        })?;
    }
    let restricts = request.changes.iter().any(|c| {
        matches!(
            c,
            Change::SecurityProfile {
                profile: PodSecurityLevel::Restricted,
                ..
            }
        )
    });
    planner
        .project_pod_security(request.changes.len() == 1, restricts)
        .map_err(|e| PlanError {
            change: request
                .changes
                .iter()
                .position(|c| matches!(c, Change::PodSecurity { .. })),
            ..e
        })?;
    planner.finish()
}

impl<'a> Planner<'a> {
    #[expect(clippy::too_many_lines, reason = "one arm per change kind")]
    fn change(&mut self, change: &Change, index: usize) -> Result<(), PlanError> {
        match change {
            Change::Env {
                workload,
                container,
                changes,
            } => self.env(workload, container.as_deref(), changes),
            Change::Data {
                kind,
                name,
                changes,
            } => self.data(*kind, name, changes),
            Change::AddDomain { workload, hostname } => self.add_domain(workload, hostname),
            Change::RenameDomain { route, from, to } => self.rename_domain(route, from, to),
            Change::RemoveDomain { route, hostname } => self.remove_domain(route, hostname),
            Change::Redirect(redirect) => self.redirect(
                &redirect.route,
                &redirect.workload,
                &redirect.hostname,
                redirect.to.as_deref(),
                redirect.permanent,
            ),
            Change::BasicAuth(auth) => self.basic_auth(&auth.route, auth.user.as_ref()),
            Change::EnvImport(import) => self.env_import(import),
            Change::PublicPort(public) => self.public_port(public),
            Change::Create { .. } => self.create(change, index),
            Change::Restart { workload } => self.restart_workload(workload),
            Change::Rollback { workload, revision } => self.rollback(workload, *revision),
            Change::ExpandVolume { claim, size } => self.expand_volume(claim, size),
            Change::LabelVolume(change) => {
                self.label_volume(&change.claim, &change.labels, &change.annotations)
            }
            Change::AddVolume {
                workload,
                container,
                volume,
            } => self.add_volume(workload, container.as_deref(), volume),
            Change::EditMount {
                workload,
                container,
                mount_path,
                new_mount_path,
                read_only,
            } => self.edit_mount(
                workload,
                container.as_deref(),
                mount_path,
                new_mount_path.as_deref(),
                *read_only,
            ),
            Change::RemoveVolume {
                workload,
                container,
                mount_path,
            } => self.remove_volume(workload, container.as_deref(), mount_path),
            Change::Probe {
                workload,
                container,
                probe,
                set,
            } => self.probe(workload, container.as_deref(), *probe, set.as_ref()),
            Change::Image {
                workload,
                container,
                image,
            } => self.image(workload, container.as_deref(), image),
            Change::Rebuild { build } => self.rebuild(build),
            Change::BuildOptions(change) => self.build_options(change),
            Change::AutoUpdate {
                workload,
                enabled,
                build,
                webhook,
            } => self.auto_update(workload, *enabled, build.as_deref(), webhook.as_deref()),
            Change::Scale { workload, replicas } => self.scale(workload, *replicas),
            Change::ConfigReload { workload, enabled } => self.config_reload(workload, *enabled),
            Change::SecurityProfile { workload, profile } => {
                self.security_profile(workload, *profile)
            }
            Change::PodSecurity { level } => self.ask_pod_security(*level),
            Change::Resources {
                workload,
                container,
                requests,
                limits,
            } => self.resources(workload, container.as_deref(), requests, limits),
            Change::Edit {
                resource,
                name,
                yaml,
            } => self.edit(resource, name, yaml),
            Change::Delete {
                resource,
                name,
                resource_version,
                owned,
            } => self.delete_change(resource, name, resource_version.as_deref(), *owned),
            Change::CreateProject { display_name } => self.create_project(display_name.as_deref()),
            Change::Adopt => self.adopt(),
            Change::Unadopt => self.unadopt(),
            Change::Details {
                display_name,
                description,
                icon,
            } => self.details(display_name.as_deref(), description.as_deref(), *icon),
            Change::ScheduleTask {
                from,
                container,
                name,
                schedule,
                time_zone,
                command,
                concurrency,
                keep_succeeded,
                keep_failed,
            } => self.schedule_task(&crate::tasks::TaskRequest {
                from,
                container: container.as_deref(),
                name,
                schedule,
                time_zone: time_zone.as_deref(),
                command,
                concurrency: *concurrency,
                keep_succeeded: *keep_succeeded,
                keep_failed: *keep_failed,
            }),
            Change::RunTask { cron_job } => self.run_task(cron_job),
            Change::DeployKey { .. } => Err(error(
                ErrorKind::BadRequest,
                "unopened_deploy_key",
                "A deploy key is planned by the server that sealed it.",
            )),
            Change::DebugContainer { .. } | Change::ClusterTools => Err(error(
                ErrorKind::BadRequest,
                "terminal_change_alone",
                "A debug container and a tools pod are planned on their own, from what the \
                 pod or the person's session says.",
            )),
        }
    }

    fn restart_workload(&mut self, workload: &WorkloadRef) -> Result<(), PlanError> {
        let found = self.workload(workload)?;
        if matches!(found.kind(), WorkloadKind::Job | WorkloadKind::CronJob) {
            return Err(error(
                ErrorKind::Invalid,
                "not_restartable",
                format!(
                    "{} {} runs to completion; it has no pods to replace.",
                    workload.kind.as_str(),
                    workload.name
                ),
            ));
        }
        self.restarts.insert(workload.clone());
        self.impact.insert(workload.clone());
        Ok(())
    }

    // ── Lookups ──────────────────────────────────────────────────────────────────────────────

    pub(crate) fn workload(&self, reference: &WorkloadRef) -> Result<Workload<'a>, PlanError> {
        workloads(self.input.snapshot)
            .into_iter()
            .find(|w| w.kind() == reference.kind && w.name() == reference.name)
            .ok_or_else(|| {
                error(
                    ErrorKind::NotFound,
                    "unknown_workload",
                    format!(
                        "There is no {} named {}.",
                        reference.kind.as_str(),
                        reference.name
                    ),
                )
            })
    }

    /// The working copy of an object, read once.
    pub(crate) fn working(&mut self, target: Target, object: Value) -> usize {
        if let Some(index) = self.objects.iter().position(|w| w.target == target) {
            return index;
        }
        self.objects.push(Working {
            target,
            current: object.clone(),
            original: Some(object),
            operations: Vec::new(),
            owners: Vec::new(),
            deleted: false,
        });
        self.objects.len() - 1
    }

    /// Records who owns `path` in object `index`, refusing when Helm, Argo CD or Flux does.
    pub(crate) fn claim(
        &mut self,
        index: usize,
        path: &[Step],
        shown: &str,
    ) -> Result<(), PlanError> {
        let Some(working) = self.objects.get_mut(index) else {
            return Ok(());
        };
        let managers = working
            .original
            .as_ref()
            .map_or_else(Vec::new, |o| owners(o, path));
        if let Some((manager, tool)) = managers
            .iter()
            .find_map(|m| reapplying_tool(m).map(|tool| (m, tool)))
        {
            return Err(PlanError {
                managed_by: Some(tool),
                ..error(
                    ErrorKind::Conflict,
                    "field_managed_by_tool",
                    format!(
                        "{shown} of {} {} is managed by {manager}, which would restore it; change it there instead.",
                        working.target.kind, working.target.name
                    ),
                )
            });
        }
        if !managers.is_empty() {
            working.owners.push(FieldOwners {
                path: shown.to_owned(),
                managers,
            });
        }
        Ok(())
    }

    /// Object `index` after the edits so far.
    pub(crate) fn current(&self, index: usize) -> Value {
        self.objects
            .get(index)
            .map_or(Value::Null, |w| w.current.clone())
    }

    /// Applies `operations` to object `index` now, so later edits see their effect.
    pub(crate) fn patch(&mut self, index: usize, operations: Vec<Value>) -> Result<(), PlanError> {
        let Some(working) = self.objects.get_mut(index) else {
            return Ok(());
        };
        working.current = apply_patch(&working.current, &operations)
            .map_err(|e| error(ErrorKind::Invalid, "patch_does_not_apply", e.to_string()))?;
        working.operations.extend(operations);
        Ok(())
    }

    pub(crate) fn refuse_managed(
        meta: &ObjectMeta,
        kind: &str,
        name: &str,
    ) -> Result<(), PlanError> {
        detect_tool(meta).map_or(Ok(()), |tool| {
            let place = match tool {
                Tool::Helm => "the release's values",
                Tool::ArgoCd | Tool::Flux => "its Git repository",
            };
            Err(PlanError {
                managed_by: Some(tool),
                ..error(
                    ErrorKind::Conflict,
                    managed_code(tool),
                    format!(
                        "{kind} {name} is managed by {}, which would undo this edit on its next \
                         sync; change it in {place} instead.",
                        tool_name(tool),
                    ),
                )
            })
        })
    }

    // ── Environment ──────────────────────────────────────────────────────────────────────────

    fn env(
        &mut self,
        reference: &WorkloadRef,
        container: Option<&str>,
        edits: &[EnvEdit],
    ) -> Result<(), PlanError> {
        validate_env(edits)
            .map_err(|r| error(ErrorKind::BadRequest, &r.0, "The variables are not valid."))?;
        let found = self.workload(reference)?;
        if found.kind() == WorkloadKind::Job {
            return Err(error(
                ErrorKind::Invalid,
                "immutable_field",
                "A Job's pod template cannot change after it is created.",
            ));
        }
        Self::refuse_managed(found.meta(), reference.kind.as_str(), &reference.name)?;
        let (name, position) = container_at(found, reference, container)?;
        let object = workload_json(found)?;
        let index = self.working(workload_target(found), object);
        self.impact.insert(reference.clone());
        let mut base: Vec<&str> = reference.kind.pod_template_path().to_vec();
        base.extend(["spec", "containers"]);
        let position_text = position.to_string();
        let mut container_path = base.clone();
        container_path.push(&position_text);
        let container_pointer = pointer(&container_path);
        let mut steps = pod_spec_steps(reference.kind);
        steps.extend([Step::field("containers"), Step::named(&name)]);
        for edit in edits {
            let mut path = steps.clone();
            path.extend([Step::field("env"), Step::named(&edit.name)]);
            let shown = format!("env {} of container {name}", edit.name);
            let current_env = self
                .objects
                .get(index)
                .and_then(|w| w.current.pointer(&format!("{container_pointer}/env")))
                .and_then(Value::as_array)
                .cloned();
            let existing = current_env.as_ref().and_then(|env| {
                env.iter()
                    .position(|e| e.get("name").and_then(Value::as_str) == Some(edit.name.as_str()))
            });
            match (edit.op.as_str(), existing) {
                ("set", Some(at)) => {
                    self.claim(index, &path, &shown)?;
                    let entry = format!("{container_pointer}/env/{at}");
                    self.patch(
                        index,
                        vec![
                            test(&format!("{entry}/name"), json!(edit.name)),
                            replace(&entry, env_entry(edit)),
                        ],
                    )?;
                }
                ("set", None) => {
                    let operation = if current_env.is_some() {
                        add(&format!("{container_pointer}/env/-"), env_entry(edit))
                    } else {
                        add(
                            &format!("{container_pointer}/env"),
                            json!([env_entry(edit)]),
                        )
                    };
                    self.patch(index, vec![operation])?;
                }
                (_, Some(at)) => {
                    self.claim(index, &path, &shown)?;
                    let entry = format!("{container_pointer}/env/{at}");
                    self.patch(
                        index,
                        vec![
                            test(&format!("{entry}/name"), json!(edit.name)),
                            remove(&entry),
                        ],
                    )?;
                }
                (_, None) => {
                    return Err(error(
                        ErrorKind::NotFound,
                        "unknown_env_var",
                        format!("Container {name} has no variable {}.", edit.name),
                    ));
                }
            }
        }
        Ok(())
    }

    // ── ConfigMap and Secret keys ────────────────────────────────────────────────────────────

    /// Puts the values of Secret `name` the person could read into `object`'s `data`, encoded;
    /// `false` when they could not read them.
    pub(crate) fn with_secret_values(&self, name: &str, object: &mut Value) -> bool {
        let Some(values) = self.input.secret_values.get(name) else {
            return false;
        };
        let data: serde_json::Map<String, Value> = values
            .iter()
            .map(|(key, bytes)| {
                (
                    key.clone(),
                    json!(base64::engine::general_purpose::STANDARD.encode(bytes)),
                )
            })
            .collect();
        if !data.is_empty()
            && let Some(map) = object.as_object_mut()
        {
            map.insert("data".to_owned(), Value::Object(data));
        }
        true
    }

    #[expect(clippy::too_many_lines, reason = "one change kind, read top to bottom")]
    pub(crate) fn data(
        &mut self,
        kind: DataKind,
        name: &str,
        edits: &[DataEdit],
    ) -> Result<(), PlanError> {
        if edits.is_empty() {
            return Err(error(
                ErrorKind::BadRequest,
                "no_changes",
                "No keys to change.",
            ));
        }
        let (kind_name, plural) = match kind {
            DataKind::ConfigMap => ("ConfigMap", "configmaps"),
            DataKind::Secret => ("Secret", "secrets"),
        };
        let found = match kind {
            DataKind::ConfigMap => self
                .input
                .snapshot
                .config_map(name)
                .map(|o| (o.metadata.clone(), to_json(o))),
            DataKind::Secret => self
                .input
                .snapshot
                .secret(name)
                .map(|o| (o.metadata.clone(), to_json(o))),
        };
        let Some((meta, json_object)) = found else {
            return Err(error(
                ErrorKind::NotFound,
                "unknown_object",
                format!("There is no {kind_name} named {name}."),
            ));
        };
        Self::refuse_managed(&meta, kind_name, name)?;
        let mut json_object = json_object?;
        if kind == DataKind::Secret && !self.with_secret_values(name, &mut json_object) {
            self.warnings.push(Warning::SecretValuesUnknown {
                name: name.to_owned(),
            });
        }
        let target = Target {
            api_version: "v1".to_owned(),
            kind: kind_name.to_owned(),
            plural: plural.to_owned(),
            name: name.to_owned(),
        };
        let index = self.working(target, json_object);
        let known_keys = kind == DataKind::ConfigMap || self.input.secret_values.contains_key(name);
        for edit in edits {
            if edit.key.is_empty()
                || !edit
                    .key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            {
                return Err(error(
                    ErrorKind::BadRequest,
                    "invalid_key",
                    format!("{:?} is not a valid key.", edit.key),
                ));
            }
            let current = self.current(index);
            if current
                .pointer(&pointer(&["binaryData", &edit.key]))
                .is_some()
            {
                return Err(error(
                    ErrorKind::Invalid,
                    "key_in_binary_data",
                    format!("{} holds binary data; edit it as YAML.", edit.key),
                ));
            }
            let key_pointer = pointer(&["data", &edit.key]);
            let exists = current.pointer(&key_pointer).is_some();
            self.claim(
                index,
                &[Step::field("data"), Step::field(&edit.key)],
                &format!("key {}", edit.key),
            )?;
            let encoded = |value: &str| match kind {
                DataKind::ConfigMap => json!(value),
                DataKind::Secret => json!(base64::engine::general_purpose::STANDARD.encode(value)),
            };
            match edit.op.as_str() {
                "set" => {
                    let value = encoded(edit.value.as_deref().unwrap_or_default());
                    if !known_keys {
                        // The Secret's `data` could not be read: adding the whole map would
                        // replace the keys it holds, so add the key alone (kube-apiserver
                        // refuses it if the Secret has no `data` at all).
                        if let Some(working) = self.objects.get_mut(index) {
                            working.operations.push(add(&key_pointer, value.clone()));
                            if let Some(object) = working.current.as_object_mut() {
                                let data = object
                                    .entry("data")
                                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
                                if let Some(data) = data.as_object_mut() {
                                    data.insert(edit.key.clone(), value);
                                }
                            }
                        }
                        continue;
                    }
                    let operation = if current.get("data").is_some_and(Value::is_object) {
                        add(&key_pointer, value)
                    } else {
                        add("/data", json!({ edit.key.clone(): value }))
                    };
                    self.patch(index, vec![operation])?;
                }
                "remove" => {
                    if known_keys && !exists {
                        return Err(error(
                            ErrorKind::NotFound,
                            "unknown_key",
                            format!("{name} has no key {}.", edit.key),
                        ));
                    }
                    if kind == DataKind::Secret && !known_keys {
                        // The key's presence is unknown here; kube-apiserver refuses the patch
                        // if it is missing. Record the removal in the working copy only.
                        if let Some(working) = self.objects.get_mut(index) {
                            working.operations.push(remove(&key_pointer));
                        }
                    } else {
                        self.patch(index, vec![remove(&key_pointer)])?;
                    }
                }
                _ => {
                    return Err(error(
                        ErrorKind::BadRequest,
                        "invalid_data_op",
                        "A key change is set or remove.",
                    ));
                }
            }
        }
        // Who reads it.
        let resource_kind = match kind {
            DataKind::ConfigMap => SourceKind::ConfigMap,
            DataKind::Secret => SourceKind::Secret,
        };
        let mut env_readers = BTreeSet::new();
        let mut reloaded = BTreeSet::new();
        for service in &self.project.services {
            let reference = WorkloadRef {
                kind: service.workload.kind,
                name: service.workload.name.clone(),
            };
            let reads_env = service.containers.iter().any(|c| {
                c.env.iter().any(|e| {
                    e.source.kind == resource_kind && e.source.name.as_deref() == Some(name)
                }) || c.imported_env.iter().any(|e| {
                    e.source.kind == resource_kind && e.source.name.as_deref() == Some(name)
                })
            });
            let mounts = service.containers.iter().any(|c| {
                c.mounts.iter().any(|m| {
                    m.object
                        .as_ref()
                        .is_some_and(|o| o.kind == kind_name && o.name == name)
                })
            });
            if reads_env || mounts {
                self.impact.insert(reference.clone());
            }
            if matches!(
                service.workload.kind,
                WorkloadKind::Job | WorkloadKind::CronJob
            ) {
                continue;
            }
            // Reloader replaces the pods of every workload that follows the object and refers to
            // it, by environment or by volume; one Zeabur saw is not running restarts nothing.
            let follows = self.input.reloader != Some(false)
                && service
                    .config_reload
                    .as_ref()
                    .is_some_and(|r| r.follows(kind == DataKind::Secret, name));
            if follows && (reads_env || mounts) {
                reloaded.insert(reference);
            } else if reads_env {
                env_readers.insert(reference);
            }
        }
        if !reloaded.is_empty() {
            self.warnings.push(Warning::ReloaderRestarts {
                workloads: reloaded.into_iter().collect(),
                confirmed: self.input.reloader == Some(true),
            });
        }
        if !env_readers.is_empty() {
            self.warnings.push(Warning::RestartNeeded {
                workloads: env_readers.into_iter().collect(),
            });
        }
        Ok(())
    }

    // ── Domains ──────────────────────────────────────────────────────────────────────────────

    fn hostname_in_use(&self, hostname: &str) -> bool {
        self.input.snapshot.ingresses.iter().any(|i| {
            i.spec
                .iter()
                .flat_map(|s| s.rules.iter().flatten())
                .any(|r| r.host.as_deref() == Some(hostname))
        }) || self
            .http_routes()
            .any(|(_, r)| r.spec.hostnames.iter().any(|h| h == hostname))
    }

    /// The project's `HTTPRoutes`, each as read and as the typed view of what Zeabur reads.
    fn http_routes(&self) -> impl Iterator<Item = (&'a Value, HttpRoute)> + use<'a, '_> {
        self.input.http_routes.iter().filter_map(|object| {
            serde_json::from_value::<HttpRoute>(object.clone())
                .ok()
                .map(|route| (object, route))
        })
    }

    /// The workloads behind Services `services`.
    pub(crate) fn behind(&self, services: &BTreeSet<String>) -> Vec<WorkloadRef> {
        self.project
            .services
            .iter()
            .filter(|s| s.endpoint_names.iter().any(|e| services.contains(e)))
            .map(|s| WorkloadRef {
                kind: s.workload.kind,
                name: s.workload.name.clone(),
            })
            .collect()
    }

    #[expect(clippy::too_many_lines, reason = "one change kind, read top to bottom")]
    fn add_domain(&mut self, reference: &WorkloadRef, hostname: &str) -> Result<(), PlanError> {
        let hostname = hostname.to_ascii_lowercase();
        validate_hostname(&hostname).map_err(|r| {
            error(
                ErrorKind::BadRequest,
                &r.0,
                format!("{hostname:?} is not a hostname."),
            )
        })?;
        self.workload(reference)?;
        let endpoints: Vec<(String, Option<i32>)> = self
            .project
            .services
            .iter()
            .find(|s| s.workload.kind == reference.kind && s.workload.name == reference.name)
            .map_or_else(Vec::new, |s| {
                self.project
                    .endpoints_of(s)
                    .map(|e| (e.name.clone(), e.ports.first().map(|p| p.port)))
                    .collect()
            });
        let Some((service, port)) = endpoints.first().cloned() else {
            return Err(error(
                ErrorKind::Invalid,
                "no_service",
                format!(
                    "{} has no Kubernetes Service, so nothing can route to it; add a port first.",
                    reference.name
                ),
            ));
        };
        if self.hostname_in_use(&hostname) {
            return Err(error(
                ErrorKind::Conflict,
                "domain_exists",
                format!("{hostname} already routes somewhere in this project."),
            ));
        }
        self.impact.insert(reference.clone());
        let names: BTreeSet<String> = endpoints.iter().map(|(n, _)| n.clone()).collect();
        // An Ingress that already routes to the workload.
        let ingress = self.input.snapshot.ingresses.iter().find_map(|i| {
            let backend = i
                .spec
                .iter()
                .flat_map(|s| s.rules.iter().flatten())
                .flat_map(|r| r.http.iter().flat_map(|h| h.paths.iter()))
                .find_map(|p| {
                    p.backend
                        .service
                        .as_ref()
                        .filter(|s| names.contains(&s.name))
                        .map(|_| p.backend.clone())
                })?;
            Some((i.clone(), backend))
        });
        if let Some((object, backend)) = ingress {
            let name = object.metadata.name.clone().unwrap_or_default();
            Self::refuse_managed(&object.metadata, "Ingress", &name)?;
            let json = to_json(&object)?;
            let index = self.working(ingress_target(&name), json);
            self.claim(index, &[Step::field("spec"), Step::field("rules")], "rules")?;
            let rule = json!({"host": hostname, "http": {"paths": [{"path": "/", "pathType": "Prefix", "backend": backend}]}});
            let has_rules = self
                .objects
                .get(index)
                .and_then(|w| w.current.pointer("/spec/rules"))
                .is_some();
            let operation = if has_rules {
                add("/spec/rules/-", rule)
            } else {
                add("/spec/rules", json!([rule]))
            };
            let mut operations = vec![operation];
            operations.extend(self.https(index, &name, &hostname)?);
            return self.patch(index, operations);
        }
        // An HTTPRoute that already routes to the workload.
        let route = self
            .http_routes()
            .find(|(_, route)| route.routes_to(|name| names.contains(name)));
        if let Some((object, route)) = route {
            let name = route.name().to_owned();
            Self::refuse_managed(&metadata(object), "HTTPRoute", &name)?;
            if route.spec.hostnames.is_empty() {
                return Err(error(
                    ErrorKind::Invalid,
                    "route_matches_every_host",
                    format!(
                        "HTTPRoute {name} lists no hostnames, so it already answers every host; adding one would narrow it."
                    ),
                ));
            }
            let index = self.working(http_route_target(&name), object.clone());
            self.claim(
                index,
                &[Step::field("spec"), Step::field("hostnames")],
                "hostnames",
            )?;
            return self.patch(index, vec![add("/spec/hostnames/-", json!(hostname))]);
        }
        // A new route.
        let name = route_name(&reference.name, &hostname);
        let taken = self
            .input
            .snapshot
            .ingresses
            .iter()
            .any(|i| i.metadata.name.as_deref() == Some(name.as_str()))
            || self.http_routes().any(|(_, r)| r.name() == name);
        if taken {
            return Err(error(
                ErrorKind::Conflict,
                "route_name_taken",
                format!("A route named {name} already exists."),
            ));
        }
        let namespace = self.input.snapshot.namespace.clone();
        let port = port.unwrap_or(80);
        let gateway = if let Some(gateways) = self.input.gateways {
            gateways.first().map(|g| (g.clone(), gateways.len()))
        } else {
            self.warnings.push(Warning::GatewaysUnreadable);
            None
        };
        let (target, body) = if let Some((gateway, of)) = gateway {
            if of > 1 {
                self.warnings.push(Warning::GatewayChosen {
                    namespace: gateway.namespace.clone(),
                    name: gateway.name.clone(),
                    of,
                });
            }
            (
                http_route_target(&name),
                json!({
                    "apiVersion": "gateway.networking.k8s.io/v1",
                    "kind": "HTTPRoute",
                    "metadata": {"name": name, "namespace": namespace},
                    "spec": {
                        "parentRefs": [{"name": gateway.name, "namespace": gateway.namespace}],
                        "hostnames": [hostname],
                        "rules": [{"backendRefs": [{"name": service, "port": port}]}]
                    }
                }),
            )
        } else {
            (
                ingress_target(&name),
                json!({
                    "apiVersion": "networking.k8s.io/v1",
                    "kind": "Ingress",
                    "metadata": {"name": name, "namespace": namespace},
                    "spec": {"rules": [{"host": hostname, "http": {"paths": [{
                        "path": "/", "pathType": "Prefix",
                        "backend": {"service": {"name": service, "port": {"number": port}}}
                    }]}}]}
                }),
            )
        };
        let is_ingress = target.kind == "Ingress";
        self.objects.push(Working {
            target,
            original: None,
            current: body,
            operations: Vec::new(),
            owners: Vec::new(),
            deleted: false,
        });
        if is_ingress {
            let index = self.objects.len() - 1;
            let operations = self.https(index, &name, &hostname)?;
            if let Some(working) = self.objects.get_mut(index) {
                working.current = apply_patch(&working.current, &operations).map_err(|e| {
                    error(ErrorKind::Invalid, "patch_does_not_apply", e.to_string())
                })?;
            }
        }
        Ok(())
    }

    /// The operations that list `hostname` for HTTPS on Ingress `index` (named `ingress`), where
    /// cert-manager can issue its certificate: through the issuer the Ingress names, else the
    /// chart's `ClusterIssuer` when the Ingress lists no certificates of its own. The hostname
    /// gets its own `tls` entry, so a host whose DNS is not ready does not hold up the others'
    /// renewals; the Secret is `<ingress>-tls` while that is free, as a new service's is.
    fn https(
        &mut self,
        index: usize,
        ingress: &str,
        hostname: &str,
    ) -> Result<Vec<Value>, PlanError> {
        const CLUSTER_ISSUER: &str = "cert-manager.io/cluster-issuer";
        const ISSUER: &str = "cert-manager.io/issuer";
        let current = self.current(index);
        let annotations = current
            .pointer("/metadata/annotations")
            .and_then(Value::as_object);
        let named = annotations
            .and_then(|a| a.get(CLUSTER_ISSUER).or_else(|| a.get(ISSUER)))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let tls = array(&current, "/spec/tls");
        let mut operations = Vec::new();
        let issuer = match (named, self.input.cluster_issuer) {
            (Some(issuer), _) => issuer,
            (None, _) if !tls.is_empty() => {
                self.warnings.push(Warning::HttpOnly {
                    hostname: hostname.to_owned(),
                    reason: HttpOnlyReason::CertificatesElsewhere,
                    route: Some(ingress.to_owned()),
                });
                return Ok(operations);
            }
            (None, Some(issuer)) => {
                self.claim(
                    index,
                    &[
                        Step::field("metadata"),
                        Step::field("annotations"),
                        Step::field(CLUSTER_ISSUER),
                    ],
                    "annotations",
                )?;
                operations.push(if annotations.is_some() {
                    add(
                        &pointer(&["metadata", "annotations", CLUSTER_ISSUER]),
                        json!(issuer),
                    )
                } else {
                    add("/metadata/annotations", json!({ CLUSTER_ISSUER: issuer }))
                });
                issuer.to_owned()
            }
            (None, None) => {
                self.warnings.push(Warning::HttpOnly {
                    hostname: hostname.to_owned(),
                    reason: HttpOnlyReason::NoIssuer,
                    route: None,
                });
                return Ok(operations);
            }
        };
        let taken = |secret: &str| {
            tls.iter()
                .any(|t| t.get("secretName").and_then(Value::as_str) == Some(secret))
        };
        let mut secret = format!("{ingress}-tls");
        if taken(&secret) {
            secret = route_name(ingress, hostname);
            secret.truncate(249);
            secret.push_str("-tls");
        }
        self.claim(index, &[Step::field("spec"), Step::field("tls")], "tls")?;
        let entry = json!({"hosts": [hostname], "secretName": secret});
        operations.push(if current.pointer("/spec/tls").is_some() {
            add("/spec/tls/-", entry)
        } else {
            add("/spec/tls", json!([entry]))
        });
        self.warnings.push(Warning::HttpsCertificate {
            hostname: hostname.to_owned(),
            issuer,
        });
        Ok(operations)
    }

    /// The route object `route`, its metadata and the Services it routes to.
    pub(crate) fn route(
        &self,
        route: &RouteRef,
    ) -> Result<(Value, ObjectMeta, BTreeSet<String>), PlanError> {
        let missing = || {
            error(
                ErrorKind::NotFound,
                "unknown_route",
                format!("There is no route named {}.", route.name),
            )
        };
        match route.kind {
            RouteKind::Ingress => {
                let object = self
                    .input
                    .snapshot
                    .ingresses
                    .iter()
                    .find(|i| i.metadata.name.as_deref() == Some(route.name.as_str()))
                    .ok_or_else(missing)?;
                let services = object
                    .spec
                    .iter()
                    .flat_map(|s| {
                        s.rules
                            .iter()
                            .flatten()
                            .flat_map(|r| r.http.iter().flat_map(|h| h.paths.iter()))
                            .filter_map(|p| p.backend.service.as_ref().map(|b| b.name.clone()))
                    })
                    .collect();
                Ok((to_json(object)?, object.metadata.clone(), services))
            }
            RouteKind::HttpRoute => {
                let (object, typed) = self
                    .http_routes()
                    .find(|(_, r)| r.name() == route.name)
                    .ok_or_else(missing)?;
                let services = typed
                    .spec
                    .rules
                    .iter()
                    .flat_map(|r| &r.backend_refs)
                    .map(|b| b.name.clone())
                    .collect();
                Ok((object.clone(), metadata(object), services))
            }
        }
    }

    fn route_index(&mut self, route: &RouteRef) -> Result<usize, PlanError> {
        let (object, meta, services) = self.route(route)?;
        let kind = match route.kind {
            RouteKind::Ingress => "Ingress",
            RouteKind::HttpRoute => "HTTPRoute",
        };
        Self::refuse_managed(&meta, kind, &route.name)?;
        let behind = self.behind(&services);
        self.impact.extend(behind);
        let target = match route.kind {
            RouteKind::Ingress => ingress_target(&route.name),
            RouteKind::HttpRoute => http_route_target(&route.name),
        };
        let index = self.working(target, object);
        let field = match route.kind {
            RouteKind::Ingress => "rules",
            RouteKind::HttpRoute => "hostnames",
        };
        self.claim(index, &[Step::field("spec"), Step::field(field)], field)?;
        Ok(index)
    }

    fn rename_domain(&mut self, route: &RouteRef, from: &str, to: &str) -> Result<(), PlanError> {
        let to = to.to_ascii_lowercase();
        validate_hostname(&to).map_err(|r| {
            error(
                ErrorKind::BadRequest,
                &r.0,
                format!("{to:?} is not a hostname."),
            )
        })?;
        if self.hostname_in_use(&to) {
            return Err(error(
                ErrorKind::Conflict,
                "domain_exists",
                format!("{to} already routes somewhere in this project."),
            ));
        }
        let index = self.route_index(route)?;
        let current = self.current(index);
        let mut operations = Vec::new();
        match route.kind {
            RouteKind::Ingress => {
                for (at, rule) in array(&current, "/spec/rules").iter().enumerate() {
                    if rule.get("host").and_then(Value::as_str) == Some(from) {
                        operations.push(test(&format!("/spec/rules/{at}/host"), json!(from)));
                        operations.push(replace(&format!("/spec/rules/{at}/host"), json!(to)));
                    }
                }
                for (at, tls) in array(&current, "/spec/tls").iter().enumerate() {
                    for (h, host) in tls
                        .get("hosts")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .enumerate()
                    {
                        if host.as_str() == Some(from) {
                            operations
                                .push(replace(&format!("/spec/tls/{at}/hosts/{h}"), json!(to)));
                            self.warnings.push(Warning::TlsHostChanged {
                                route: route.name.clone(),
                                hostname: to.clone(),
                            });
                        }
                    }
                }
            }
            RouteKind::HttpRoute => {
                for (at, host) in array(&current, "/spec/hostnames").iter().enumerate() {
                    if host.as_str() == Some(from) {
                        operations.push(test(&format!("/spec/hostnames/{at}"), json!(from)));
                        operations.push(replace(&format!("/spec/hostnames/{at}"), json!(to)));
                    }
                }
            }
        }
        if operations.is_empty() {
            return Err(error(
                ErrorKind::NotFound,
                "unknown_host",
                format!("{} does not route {from}.", route.name),
            ));
        }
        self.patch(index, operations)
    }

    fn remove_domain(&mut self, route: &RouteRef, hostname: &str) -> Result<(), PlanError> {
        let index = self.route_index(route)?;
        let current = self.current(index);
        let mut operations = Vec::new();
        match route.kind {
            RouteKind::Ingress => {
                let rules = array(&current, "/spec/rules");
                let matching: Vec<usize> = rules
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r.get("host").and_then(Value::as_str) == Some(hostname))
                    .map(|(i, _)| i)
                    .collect();
                if matching.is_empty() {
                    return Err(error(
                        ErrorKind::NotFound,
                        "unknown_host",
                        format!("{} does not route {hostname}.", route.name),
                    ));
                }
                if matching.len() == rules.len()
                    && current.pointer("/spec/defaultBackend").is_none()
                {
                    return Err(error(
                        ErrorKind::Invalid,
                        "last_hostname",
                        format!(
                            "{hostname} is the only host of Ingress {}; delete the Ingress instead.",
                            route.name
                        ),
                    ));
                }
                for at in matching.iter().rev() {
                    operations.push(test(&format!("/spec/rules/{at}/host"), json!(hostname)));
                    operations.push(remove(&format!("/spec/rules/{at}")));
                }
                for (at, tls) in array(&current, "/spec/tls").iter().enumerate().rev() {
                    let hosts = tls
                        .get("hosts")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    if let Some(h) = hosts.iter().position(|v| v.as_str() == Some(hostname)) {
                        if hosts.len() == 1 {
                            operations.push(remove(&format!("/spec/tls/{at}")));
                        } else {
                            operations.push(remove(&format!("/spec/tls/{at}/hosts/{h}")));
                        }
                    }
                }
            }
            RouteKind::HttpRoute => {
                let hostnames = array(&current, "/spec/hostnames");
                let Some(at) = hostnames.iter().position(|v| v.as_str() == Some(hostname)) else {
                    return Err(error(
                        ErrorKind::NotFound,
                        "unknown_host",
                        format!("{} does not route {hostname}.", route.name),
                    ));
                };
                if hostnames.len() == 1 {
                    return Err(error(
                        ErrorKind::Invalid,
                        "last_hostname",
                        format!(
                            "{hostname} is the only hostname of HTTPRoute {}; without it the route would answer every host.",
                            route.name
                        ),
                    ));
                }
                operations.push(test(&format!("/spec/hostnames/{at}"), json!(hostname)));
                operations.push(remove(&format!("/spec/hostnames/{at}")));
            }
        }
        self.patch(index, operations)
    }

    // ── Restarts and the result ──────────────────────────────────────────────────────────────

    fn restart(&mut self, reference: &WorkloadRef) -> Result<(), PlanError> {
        let found = self.workload(reference)?;
        let object = workload_json(found)?;
        let target = workload_target(found);
        let now = self.input.now.to_owned();
        if reference.kind == WorkloadKind::Job {
            // A Job's pod template cannot change after creation:
            // https://kubernetes.io/docs/concepts/workloads/controllers/job/#pod-template
            return Err(error(
                ErrorKind::Invalid,
                "immutable_field",
                "A Job cannot be restarted: its pod template cannot change. Run it again instead.",
            ));
        }
        let index = self.working(target, object);
        let template = reference.kind.pod_template_path();
        let current = self.current(index);
        let mut metadata: Vec<&str> = template.to_vec();
        metadata.push("metadata");
        let metadata_pointer = pointer(&metadata);
        let mut annotation = metadata.clone();
        annotation.extend(["annotations", RESTARTED_AT]);
        let annotation_pointer = pointer(&annotation);
        let operations = match current.pointer(&metadata_pointer) {
            None => vec![add(
                &metadata_pointer,
                json!({"annotations": {RESTARTED_AT: now}}),
            )],
            Some(meta) if meta.get("annotations").is_none() => vec![add(
                &format!("{metadata_pointer}/annotations"),
                json!({RESTARTED_AT: now}),
            )],
            // Restarted before: the value it had is tested, then replaced.
            Some(_) => current.pointer(&annotation_pointer).map_or_else(
                || vec![add(&annotation_pointer, json!(now))],
                |before| {
                    vec![
                        test(&annotation_pointer, before.clone()),
                        replace(&annotation_pointer, json!(now)),
                    ]
                },
            ),
        };
        self.patch(index, operations)
    }

    fn finish(mut self) -> Result<Plan, PlanError> {
        for reference in std::mem::take(&mut self.restarts) {
            self.restart(&reference)?;
        }
        let mut objects = Vec::new();
        for working in self.objects {
            let resource_version = working
                .original
                .as_ref()
                .and_then(|o| o.pointer("/metadata/resourceVersion"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            if working.deleted {
                objects.push(crate::delete::deletion(working)?);
                continue;
            }
            let write = if working.original.is_none() {
                Write::Create {
                    body: working.current.clone(),
                }
            } else {
                let mut operations = Vec::with_capacity(working.operations.len() + 1);
                operations.extend(precondition(working.original.as_ref()));
                operations.extend(working.operations);
                Write::Patch { operations }
            };
            let change = self
                .created
                .iter()
                .find(|(target, _)| *target == working.target)
                .map(|(_, index)| *index);
            let before = working.original.as_ref().map(shown);
            let after = Some(shown(&working.current));
            let changes = diff(working.original.as_ref(), &working.current);
            objects.push(PlannedObject {
                target: working.target,
                before_yaml: yaml(before.as_ref()),
                after_yaml: yaml(after.as_ref()),
                before,
                after,
                changes,
                owners: working.owners,
                resource_version,
                write,
                change,
                raw_after: working.current,
            });
        }
        Ok(Plan {
            objects,
            impact: self.impact.into_iter().collect(),
            warnings: self.warnings,
        })
    }
}

/// The `test` that pins a patch to the object its plan read.
///
/// An object with `metadata.generation` is pinned by it: the server raises the generation on
/// every change to its spec, and only then [k8s-conventions-metadata], so a controller writing
/// the object's status (which bumps `resourceVersion` every few seconds during a rollout
/// [k8s-status-subresource]) does not make the plan stale, while any edit someone else makes to
/// the spec does. Every value the patch replaces or removes is tested as well, which covers
/// edits to metadata. An object without a generation (a ConfigMap, a Secret) is pinned by its
/// `resourceVersion` [k8s-resource-version].
///
/// [k8s-conventions-metadata]: https://github.com/kubernetes/community/blob/master/contributors/devel/sig-architecture/api-conventions.md#metadata
/// [k8s-status-subresource]: https://kubernetes.io/docs/tasks/extend-kubernetes/custom-resources/custom-resource-definitions/#status-subresource
/// [k8s-resource-version]: https://kubernetes.io/docs/reference/using-api/api-concepts/#resource-versions
fn precondition(original: Option<&Value>) -> Option<Value> {
    let original = original?;
    if let Some(generation) = original
        .pointer("/metadata/generation")
        .filter(|g| g.is_i64() || g.is_u64())
    {
        return Some(test("/metadata/generation", generation.clone()));
    }
    original
        .pointer("/metadata/resourceVersion")
        .filter(|v| v.is_string())
        .map(|version| test("/metadata/resourceVersion", version.clone()))
}

/// `object` as a person may see it: the YAML view's trimming and hiding
/// ([`zeabur_graph::manifest::clean`]), so a plan and the object's page show the same text.
pub(crate) fn shown(object: &Value) -> Value {
    zeabur_graph::manifest::clean(object)
}

/// A shown object as the object's YAML view writes it ([`zeabur_graph::manifest::to_yaml`]).
pub(crate) fn yaml(object: Option<&Value>) -> Option<String> {
    object.and_then(|o| zeabur_graph::manifest::to_yaml(o).ok())
}

pub(crate) fn array(object: &Value, path: &str) -> Vec<Value> {
    object
        .pointer(path)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn to_json<T: serde::Serialize>(object: &T) -> Result<Value, PlanError> {
    serde_json::to_value(object)
        .map_err(|e| error(ErrorKind::Invalid, "unreadable_object", e.to_string()))
}

/// The name and index of `container` in `found`'s pod, or of its only container.
pub(crate) fn container_at(
    found: Workload<'_>,
    reference: &WorkloadRef,
    container: Option<&str>,
) -> Result<(String, usize), PlanError> {
    let containers: Vec<&str> = found.pod_spec().map_or_else(Vec::new, |s| {
        s.containers.iter().map(|c| c.name.as_str()).collect()
    });
    let name = match container {
        Some(name) => name.to_owned(),
        None if containers.len() == 1 => containers
            .first()
            .map_or_else(String::new, |c| (*c).to_owned()),
        None => {
            return Err(error(
                ErrorKind::BadRequest,
                "container_required",
                "The workload has several containers; name one.",
            ));
        }
    };
    let Some(position) = containers.iter().position(|c| *c == name) else {
        return Err(error(
            ErrorKind::NotFound,
            "unknown_container",
            format!("{} has no container {name}.", reference.name),
        ));
    };
    Ok((name, position))
}

pub(crate) fn workload_json(workload: Workload<'_>) -> Result<Value, PlanError> {
    match workload {
        Workload::CronJob(o) => to_json(o),
        Workload::DaemonSet(o) => to_json(o),
        Workload::Deployment(o) => to_json(o),
        Workload::Job(o) => to_json(o),
        Workload::StatefulSet(o) => to_json(o),
    }
}

pub(crate) fn workload_target(workload: Workload<'_>) -> Target {
    use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
    use k8s_openapi::api::batch::v1::{CronJob, Job};
    let name = workload.name();
    match workload.kind() {
        WorkloadKind::CronJob => Target::of::<CronJob>(name),
        WorkloadKind::Job => Target::of::<Job>(name),
        WorkloadKind::DaemonSet => Target::of::<DaemonSet>(name),
        WorkloadKind::Deployment => Target::of::<Deployment>(name),
        WorkloadKind::StatefulSet => Target::of::<StatefulSet>(name),
    }
}

/// The metadata of an object read as JSON; empty when it has none that reads.
fn metadata(object: &Value) -> ObjectMeta {
    object
        .get("metadata")
        .cloned()
        .and_then(|m| serde_json::from_value(m).ok())
        .unwrap_or_default()
}

pub(crate) fn ingress_target(name: &str) -> Target {
    Target::of::<k8s_openapi::api::networking::v1::Ingress>(name)
}

pub(crate) fn http_route_target(name: &str) -> Target {
    Target {
        api_version: "gateway.networking.k8s.io/v1".to_owned(),
        kind: "HTTPRoute".to_owned(),
        plural: "httproutes".to_owned(),
        name: name.to_owned(),
    }
}

/// The `managedFields` steps of a workload's pod spec.
pub(crate) fn pod_spec_steps(kind: WorkloadKind) -> Vec<Step> {
    kind.pod_template_path()
        .iter()
        .chain(&["spec"])
        .map(|n| Step::field(n))
        .collect()
}

/// What pins each object a plan writes, by `kind/name`, for the token.
///
/// The pin of a patch is the generation when the object has one, else its `resourceVersion`
/// (the patch's first `test`), so a status write between plan and apply does not make the plan
/// stale. The pin of a deletion is its `resourceVersion`, the precondition kube-apiserver checks
/// (it has none on the generation).
#[must_use]
pub fn versions(plan: &Plan) -> BTreeMap<String, String> {
    plan.objects
        .iter()
        .filter_map(|o| {
            let pin = match &o.write {
                Write::Patch { operations } => operations.first().and_then(|op| {
                    let path = op.get("path")?.as_str()?;
                    let value = op.get("value")?;
                    (op.get("op")?.as_str()? == "test"
                        && (path == "/metadata/generation" || path == "/metadata/resourceVersion"))
                        .then(|| format!("{path}={value}"))
                }),
                Write::Delete {
                    resource_version, ..
                } => Some(format!(
                    "/metadata/resourceVersion={}",
                    json!(resource_version)
                )),
                Write::EphemeralContainers { operations } => operations.first().and_then(|op| {
                    let value = op.get("value")?;
                    (op.get("path")?.as_str()? == "/metadata/uid")
                        .then(|| format!("/metadata/uid={value}"))
                }),
                Write::Create { .. } => None,
            };
            pin.map(|pin| (format!("{}/{}", o.target.kind, o.target.name), pin))
        })
        .collect()
}
