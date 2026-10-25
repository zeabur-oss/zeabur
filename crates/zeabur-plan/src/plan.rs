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
use k8s_openapi::api::networking::v1::Ingress;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde_json::{Value, json};
use zeabur_graph::{Snapshot, Workload, workloads};
use zeabur_model::adapters::error_pages::maintenance_of;
use zeabur_model::adapters::gateway::Attachable;
use zeabur_model::adapters::provenance::{Tool, detect_tool};
use zeabur_model::kernel::names::Conventions;
use zeabur_model::{HttpRoute, Project, SourceKind, WorkloadKind, WorkloadRef};

use crate::access::redirect_rule;
use crate::certificates::{
    CLUSTER_ISSUER, CheckedCertificate, ISSUER, IssuerRead, issued_secret_name,
};
use crate::change::{
    AddDomainChange, CertificateSource, Change, DataEdit, DataKind, FieldOwners, GatewayParent,
    HttpOnlyReason, ObjectName, Plan, PlanRequest, PlannedObject, RouteKind, RouteRef, Target,
    Warning, Write,
};
use crate::checks::ResourceRef;
use crate::create::ServedKind;
use crate::error_pages::shows_maintenance;
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
/// - a volume's restore: K8up's `Restore`, into a new claim, into the claim itself or out to the
///   bucket;
/// - a service from a container image: its Deployment, Service, Ingress, volume claim and Secrets;
/// - a service from Git: the same, and Zeabur Builder's `Build`;
/// - the page server of Zeabur Catalog's `error-pages` template: its ConfigMaps, Deployment,
///   Service and NetworkPolicy.
///
/// The object is created as the person, so kube-apiserver's RBAC and admission decide, as for
/// `kubectl create`; the plan only keeps it in the project's namespace.
pub const CREATABLE: &[(&str, &str, &str)] = &[
    ("networking.k8s.io/v1", "NetworkPolicy", "networkpolicies"),
    ("v1", "ConfigMap", "configmaps"),
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
    ("k8up.io/v1", "Restore", "restores"),
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
    /// The Gateways with listeners that accept an `HTTPRoute` from the project's namespace;
    /// `None` when they could not be read.
    pub gateways: Option<&'a [Attachable]>,
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
    /// What deleting the project reads besides its objects; `None` when the request does not
    /// delete it.
    pub deletion: Option<&'a crate::project_deletion::DeletionInput>,
    /// The `ClusterIssuer` the chart names for new hostnames (`domains.clusterIssuer`); `None`:
    /// a new hostname gets HTTPS only where its Ingress already asks cert-manager for it.
    pub cluster_issuer: Option<&'a str>,
    /// The `ClusterIssuers` the request names or a wildcard hostname would use, by name, as
    /// read (see the `certificates` module).
    pub issuers: &'a BTreeMap<String, IssuerRead>,
    /// The person's own certificates the request sets, checked, by hostname.
    pub certificates: &'a BTreeMap<String, CheckedCertificate>,
    /// Some namespace carries [`zeabur_model::kernel::isolation::PLATFORM_LABEL`]: a new project
    /// gets its `NetworkPolicy`.
    pub platform_namespaces: bool,
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
        static NO_ISSUERS: BTreeMap<String, IssuerRead> = BTreeMap::new();
        static NO_CERTIFICATES: BTreeMap<String, CheckedCertificate> = BTreeMap::new();
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
            deletion: None,
            cluster_issuer: None,
            issuers: &NO_ISSUERS,
            certificates: &NO_CERTIFICATES,
            platform_namespaces: false,
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
    /// The objects the request's `create` changes give, read before any change is planned.
    pub(crate) requested: Vec<Value>,
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
    if request.changes.len() > 1 && request.changes.iter().any(Change::is_project_deletion) {
        return Err(error(
            ErrorKind::BadRequest,
            "delete_project_alone",
            "Deleting a project is planned on its own.",
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
        requested: request
            .changes
            .iter()
            .filter_map(crate::create::created_object)
            .filter_map(Result::ok)
            .collect(),
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
            Change::AddDomain(add) => self.add_domain(add),
            Change::Certificate(change) => {
                self.certificate(&change.route, &change.hostname, &change.certificate)
            }
            Change::RenameDomain { route, from, to } => self.rename_domain(route, from, to),
            Change::RemoveDomain { route, hostname } => self.remove_domain(route, hostname),
            Change::Redirect(redirect) => self.redirect(
                &redirect.route,
                &redirect.hostname,
                redirect.to.as_deref(),
                redirect.permanent,
            ),
            Change::BasicAuth(auth) => self.basic_auth(&auth.route, auth.user.as_ref()),
            Change::IpAllow(allow) => self.ip_allow(&allow.route, allow.enabled, &allow.cidrs),
            Change::EgressLimit(limit) => self.egress_limit(limit),
            Change::ErrorPageHtml(pages) => self.error_page_html(pages),
            Change::ErrorPages(pages) => self.error_pages(&pages.route, pages.enabled),
            Change::Maintenance(change) => {
                self.maintenance(&change.route, &change.hostname, change.enabled)
            }
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
            Change::SwapClaim { workload, from, to } => self.swap_claim(workload, from, to),
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
            Change::UploadToken { build, request } => self.upload_token(build, request),
            Change::ArchiveDigest { build, digest } => self.archive_digest(build, digest),
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
            Change::DeleteProject => self.delete_project(),
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
            Change::EditTask {
                cron_job,
                schedule,
                time_zone,
                command,
                concurrency,
                keep_succeeded,
                keep_failed,
                paused,
            } => self.edit_task(&crate::tasks::TaskEdit {
                cron_job,
                schedule: schedule.as_deref(),
                time_zone: time_zone.as_deref(),
                command: command.as_deref(),
                concurrency: *concurrency,
                keep_succeeded: *keep_succeeded,
                keep_failed: *keep_failed,
                paused: *paused,
            }),
            Change::DeployKey { .. } => Err(error(
                ErrorKind::BadRequest,
                "unopened_deploy_key",
                "A deploy key is planned by the server that sealed it.",
            )),
            Change::PageServer(_) | Change::Template(_) => Err(error(
                ErrorKind::BadRequest,
                "unopened_template",
                "A template is planned by the server that carries it.",
            )),
            Change::CopyS3Keys(_) => Err(error(
                ErrorKind::BadRequest,
                "unopened_s3_keys",
                "The keys of an S3 destination are planned by the server that reads them.",
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

    /// The name "Add domain" gives the route of `hostname`, after the first workload behind
    /// `services` (else `fallback`), so deleting the service takes the route along. Refused when a
    /// route has it.
    pub(crate) fn own_route_name(
        &self,
        services: &BTreeSet<String>,
        fallback: &str,
        hostname: &str,
    ) -> Result<String, PlanError> {
        let workload = self.behind(services).into_iter().next();
        let name = route_name(
            workload.as_ref().map_or(fallback, |w| w.name.as_str()),
            hostname,
        );
        let taken =
            self.input
                .snapshot
                .ingresses
                .iter()
                .any(|i| i.metadata.name.as_deref() == Some(name.as_str()))
                || self.http_routes().any(|(_, r)| r.name() == name)
                || self.objects.iter().any(|w| {
                    w.target == ingress_target(&name) || w.target == http_route_target(&name)
                });
        if taken {
            return Err(error(
                ErrorKind::Conflict,
                "route_name_taken",
                format!("A route named {name} already exists."),
            ));
        }
        Ok(name)
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
    fn add_domain(&mut self, change: &AddDomainChange) -> Result<(), PlanError> {
        let reference = &change.workload;
        let certificate = change.certificate.as_ref();
        let hostname = change.hostname.to_ascii_lowercase();
        validate_hostname(&hostname).map_err(|r| {
            error(
                ErrorKind::BadRequest,
                &r.0,
                format!("{hostname:?} is not a hostname."),
            )
        })?;
        if change.parent.is_some() && certificate.is_some() {
            return Err(error(
                ErrorKind::BadRequest,
                "parent_with_certificate",
                "A domain on a Gateway gets its certificate from the Gateway's listener; choose a \
                 certificate only for a domain on an Ingress.",
            ));
        }
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
        let parent = Self::new_route_parent(change);
        if let Some(parent) = parent {
            self.check_parent(parent, &hostname)?;
        }
        match certificate {
            Some(CertificateSource::Issuer { name }) => {
                self.check_chosen_issuer(name, &hostname)?;
            }
            Some(CertificateSource::Own { .. }) => {
                self.checked(&hostname)?;
            }
            None => {}
        }
        self.impact.insert(reference.clone());
        let names: BTreeSet<String> = endpoints.iter().map(|(n, _)| n.clone()).collect();
        let port = port.unwrap_or(80);
        if let Some(parent) = parent {
            if let Some(object) = self.joinable_http_route(&names, &hostname, Some(parent)) {
                return self.join_http_route(&object, &hostname);
            }
            return self.new_route(reference, &hostname, (&service, port), Some(parent), None);
        }
        let joins = |i: &Ingress| match certificate {
            None => true,
            Some(CertificateSource::Issuer { name }) => {
                i.metadata
                    .annotations
                    .as_ref()
                    .and_then(|a| a.get(CLUSTER_ISSUER))
                    == Some(name)
            }
            Some(CertificateSource::Own { .. }) => false,
        };
        // An Ingress that already routes to the workload.
        let ingress = self
            .input
            .snapshot
            .ingresses
            .iter()
            .filter(|i| joins(i) && maintenance_of(i).is_none())
            .find_map(|i| {
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
            self.patch(index, vec![operation])?;
            return self.secure(index, &name, &hostname, certificate);
        }
        // An HTTPRoute that already routes to the workload; a chosen certificate needs an
        // Ingress.
        if certificate.is_none()
            && let Some(object) = self.joinable_http_route(&names, &hostname, None)
        {
            return self.join_http_route(&object, &hostname);
        }
        self.new_route(reference, &hostname, (&service, port), None, certificate)
    }

    /// The Gateway of a new domain's `HTTPRoute`; without one the domain joins or gets an Ingress.
    /// This is the one place that picks the kind of a new route: to prefer `HTTPRoute`, return a
    /// default Gateway here.
    const fn new_route_parent(change: &AddDomainChange) -> Option<&GatewayParent> {
        change.parent.as_ref()
    }

    /// Refuses `parent` when the Gateways could be read and none of its listeners accepts a
    /// route from the project for `hostname`.
    fn check_parent(&self, parent: &GatewayParent, hostname: &str) -> Result<(), PlanError> {
        let Some(gateways) = self.input.gateways else {
            return Ok(());
        };
        let shown = format!("{}/{}", parent.namespace, parent.name);
        let Some(gateway) = gateways
            .iter()
            .find(|g| g.namespace == parent.namespace && g.name == parent.name)
            .filter(|g| {
                parent
                    .section_name
                    .as_ref()
                    .is_none_or(|s| g.listeners.iter().any(|l| l.name == *s))
            })
        else {
            return Err(error(
                ErrorKind::Invalid,
                "gateway_refuses_route",
                format!("Gateway {shown} does not accept routes from this project."),
            ));
        };
        if !gateway.serves(parent.section_name.as_deref(), hostname) {
            return Err(error(
                ErrorKind::Invalid,
                "listener_hostname_mismatch",
                format!("No listener of Gateway {shown} serves {hostname}."),
            ));
        }
        Ok(())
    }

    /// An `HTTPRoute` that sends to Services `names` and can take `hostname`: attached to
    /// `parent` when given, and not showing the maintenance page or redirecting, as the new
    /// hostname would do the same. A route whose Gateway listeners cannot serve the hostname is
    /// left out, as Gateway API ignores such a hostname.
    fn joinable_http_route(
        &self,
        names: &BTreeSet<String>,
        hostname: &str,
        parent: Option<&GatewayParent>,
    ) -> Option<Value> {
        let namespace = self.input.snapshot.namespace.as_str();
        let server = self.page_server().ok();
        self.http_routes()
            .filter(|(_, route)| route.routes_to(|name| names.contains(name)))
            .filter(|(object, _)| {
                redirect_rule(object).is_none() && !shows_maintenance(object, server.as_ref())
            })
            .filter(|(_, route)| {
                parent.is_none_or(|p| {
                    route.spec.parent_refs.iter().any(|r| {
                        r.gateway(namespace) == Some((p.namespace.as_str(), p.name.as_str()))
                            && r.section_name == p.section_name
                    })
                })
            })
            .find(|(_, route)| self.listeners_serve(route, hostname))
            .map(|(object, _)| object.clone())
    }

    /// Whether a Gateway listener of `route` may serve `hostname`: false only when every Gateway
    /// parent is known and none of its listeners serves it.
    fn listeners_serve(&self, route: &HttpRoute, hostname: &str) -> bool {
        let Some(gateways) = self.input.gateways else {
            return true;
        };
        let namespace = self.input.snapshot.namespace.as_str();
        let mut parents = route
            .spec
            .parent_refs
            .iter()
            .filter_map(|r| r.gateway(namespace).map(|g| (g, r.section_name.as_deref())))
            .peekable();
        if parents.peek().is_none() {
            return true;
        }
        parents.any(|((ns, name), section)| {
            gateways
                .iter()
                .find(|g| g.namespace == ns && g.name == name)
                .is_none_or(|g| g.serves(section, hostname))
        })
    }

    fn join_http_route(&mut self, object: &Value, hostname: &str) -> Result<(), PlanError> {
        let meta = metadata(object);
        let name = meta.name.clone().unwrap_or_default();
        Self::refuse_managed(&meta, "HTTPRoute", &name)?;
        if array(object, "/spec/hostnames").is_empty() {
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
        self.patch(index, vec![add("/spec/hostnames/-", json!(hostname))])
    }

    /// A new route for `hostname` to `backend` (a Service and its port): an `HTTPRoute` on
    /// `parent` when given, else an Ingress with the certificate chosen.
    fn new_route(
        &mut self,
        reference: &WorkloadRef,
        hostname: &str,
        (service, port): (&str, i32),
        parent: Option<&GatewayParent>,
        certificate: Option<&CertificateSource>,
    ) -> Result<(), PlanError> {
        let name = route_name(&reference.name, hostname);
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
        if let Some(parent) = parent {
            let mut reference = json!({"name": parent.name, "namespace": parent.namespace});
            if let (Some(section), Some(map)) = (&parent.section_name, reference.as_object_mut()) {
                map.insert("sectionName".to_owned(), json!(section));
            }
            self.objects.push(Working {
                target: http_route_target(&name),
                original: None,
                current: json!({
                    "apiVersion": "gateway.networking.k8s.io/v1",
                    "kind": "HTTPRoute",
                    "metadata": {"name": name, "namespace": namespace},
                    "spec": {
                        "parentRefs": [reference],
                        "hostnames": [hostname],
                        "rules": [{"backendRefs": [{"name": service, "port": port}]}]
                    }
                }),
                operations: Vec::new(),
                owners: Vec::new(),
                deleted: false,
            });
            return Ok(());
        }
        self.objects.push(Working {
            target: ingress_target(&name),
            original: None,
            current: json!({
                "apiVersion": "networking.k8s.io/v1",
                "kind": "Ingress",
                "metadata": {"name": name, "namespace": namespace},
                "spec": {"rules": [{"host": hostname, "http": {"paths": [{
                    "path": "/", "pathType": "Prefix",
                    "backend": {"service": {"name": service, "port": {"number": port}}}
                }]}}]}
            }),
            operations: Vec::new(),
            owners: Vec::new(),
            deleted: false,
        });
        let index = self.objects.len() - 1;
        self.secure(index, &name, hostname, certificate)
    }

    /// Lists `hostname` for HTTPS on Ingress `index`, with the certificate the person chose.
    fn secure(
        &mut self,
        index: usize,
        ingress: &str,
        hostname: &str,
        certificate: Option<&CertificateSource>,
    ) -> Result<(), PlanError> {
        match certificate {
            None => {
                let operations = self.https(index, ingress, hostname)?;
                self.patch(index, operations)
            }
            Some(CertificateSource::Issuer { name }) => {
                self.use_issuer(index, ingress, hostname, name)
            }
            Some(CertificateSource::Own { .. }) => self.use_own(index, ingress, hostname),
        }
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
        let unfit = named
            .as_deref()
            .or(self.input.cluster_issuer)
            .filter(|issuer| !self.issuer_fits(issuer, hostname));
        if unfit.is_some() {
            self.warnings.push(Warning::HttpOnly {
                hostname: hostname.to_owned(),
                reason: HttpOnlyReason::WildcardNeedsDns01,
                route: None,
            });
            return Ok(operations);
        }
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
        let secret = issued_secret_name(ingress, hostname, &tls);
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
        let named = |meta: &ObjectMeta| meta.name.as_deref() == Some(route.name.as_str());
        let object = match route.kind {
            RouteKind::Ingress => self
                .input
                .snapshot
                .ingresses
                .iter()
                .find(|i| named(&i.metadata))
                .map(to_json)
                .transpose()?,
            RouteKind::HttpRoute => self
                .input
                .http_routes
                .iter()
                .find(|o| named(&metadata(o)))
                .cloned(),
        }
        .ok_or_else(|| {
            error(
                ErrorKind::NotFound,
                "unknown_route",
                format!("There is no route named {}.", route.name),
            )
        })?;
        let services = route.kind.services(&object);
        let meta = metadata(&object);
        Ok((object, meta, services))
    }

    fn route_index(&mut self, route: &RouteRef) -> Result<usize, PlanError> {
        let (object, meta, services) = self.route(route)?;
        Self::refuse_managed(&meta, route.kind.kind_name(), &route.name)?;
        let behind = self.behind(&services);
        self.impact.extend(behind);
        let index = self.working(route.kind.target(&route.name), object);
        let field = route.kind.hosts_field();
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
        let (operations, tls_changed) = route.kind.rename_host(&self.current(index), from, &to);
        if operations.is_empty() {
            return Err(error(
                ErrorKind::NotFound,
                "unknown_host",
                format!("{} does not route {from}.", route.name),
            ));
        }
        if tls_changed {
            self.warnings.push(Warning::TlsHostChanged {
                route: route.name.clone(),
                hostname: to,
            });
        }
        self.patch(index, operations)?;
        self.end_maintenance_ingress(route, from)
    }

    pub(crate) fn remove_domain(
        &mut self,
        route: &RouteRef,
        hostname: &str,
    ) -> Result<(), PlanError> {
        if self.last_host(route, hostname)? {
            return self.delete_route(route, hostname);
        }
        let index = self.route_index(route)?;
        let operations = route.kind.remove_host(&self.current(index), hostname);
        if operations.is_empty() {
            return Err(error(
                ErrorKind::NotFound,
                "unknown_host",
                format!("{} does not route {hostname}.", route.name),
            ));
        }
        self.patch(index, operations)?;
        self.end_maintenance_ingress(route, hostname)
    }

    /// Whether `hostname` is all that `route` routes, after the edits so far.
    fn last_host(&self, route: &RouteRef, hostname: &str) -> Result<bool, PlanError> {
        let target = route.kind.target(&route.name);
        let current = match self.objects.iter().find(|w| w.target == target) {
            Some(working) => working.current.clone(),
            None => self.route(route)?.0,
        };
        Ok(route.kind.is_last_host(&current, hostname))
    }

    /// Deletes `route`, whose last hostname is `hostname`. The garbage collector deletes the
    /// maintenance Ingress it owns.
    fn delete_route(&mut self, route: &RouteRef, hostname: &str) -> Result<(), PlanError> {
        let (_, _, services) = self.route(route)?;
        self.delete(&route.kind.resource(), &route.name, None)?;
        let behind = self.behind(&services);
        self.impact.extend(behind);
        self.warnings.push(Warning::RouteDeleted {
            route: ObjectName::of(route.kind.kind_name(), &route.name),
            hostname: hostname.to_owned(),
        });
        Ok(())
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
    Target::of::<Ingress>(name)
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
