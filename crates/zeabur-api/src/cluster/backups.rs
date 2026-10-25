//! The cluster's backups through Velero: where they go, when they run, how each went, and the
//! plans that set them up (see [`zeabur_modules::velero::cluster`]).
//!
//! | Route | What it does |
//! |---|---|
//! | `GET /api/v1/cluster/backups` | Velero's locations, schedules and backups, or that it is not installed |
//! | `POST /api/v1/cluster/backups/plan` | what setting the location or the schedule, pausing it, backing up now or restoring a backup writes, and a token; writes nothing |
//! | `POST /api/v1/cluster/backups/apply` | carries out that plan by its token |
//! | `GET /api/v1/cluster/backups/{backup}/download` | the backup's contents as a file (see [`super::backup_download`]) |
//! | `GET /api/v1/cluster/backups/{backup}/deleted-projects` | the projects in the backup that no longer exist (see [`super::restore_copy`]) |
//!
//! A project restore never brings back routes. The domains come back last: `attachDomains`
//! restores only the routes from the backup, and refuses while another project serves one of
//! their hostnames; `moveDomains` moves them from that project.
//!
//! Everything is read and written as the person; RBAC on `velero.io` and on Secrets in Velero's
//! namespace decides. Velero's API not being served is an answer (`not-installed`), not an error.
//! The token is sealed, so the keys a person types travel in it unreadable; it binds the person,
//! Velero's namespace, the request, the name of a backup or restore made now and the
//! `resourceVersion` of every object the plan reads. Apply reads them again, refuses when one
//! moved, dry-runs every write, then writes. A plan's YAML never holds the keys.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::State;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use zeabur_core::problem::{Denied, Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{
    Cluster, ClusterError, ClusterExt as _, DynamicObject, ObjectTarget, ResourceRef, WriteBody,
    WriteRequest,
};
use zeabur_model::PodSecurityLevel;
use zeabur_modules::Reading;
use zeabur_modules::backups::Outcome;
use zeabur_modules::velero::{self, cluster as v};

use crate::cluster::restore_copy::{self, routes};
use crate::cluster::{backup_download, s3_destinations};
use crate::error::{ApiError, get_read, list_read, read_problem};
use crate::handlers::{AppState, Person};
use crate::plan_token::{self as token, LIFETIME_SECONDS, TokenError};
use crate::projects::backups::ProviderState;
use crate::projects::namespace::Permission;
use crate::projects::writes::token_problem;
use crate::views::UnreadableView;

/// Sealed plans of this module start with this, so no other plan's token opens as one.
const PREFIX: &str = "b1.";
pub(crate) const GROUP: &str = "velero.io";
/// The most backups an answer lists, newest first.
const SHOWN: usize = 100;
/// What a plan's YAML shows for the keys.
const HIDDEN: &str = "(hidden)";

/// The cluster's backups as Velero keeps them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterBackups {
    /// Whether Velero is installed and its objects could be read.
    pub state: ProviderState,
    /// Velero's namespace.
    pub namespace: String,
    /// Where backups go.
    pub locations: Vec<v::Location>,
    /// When they run.
    pub schedules: Vec<v::ScheduleView>,
    /// The newest backups, newest first, a hundred at most.
    pub backups: Vec<v::BackupView>,
    /// The newest restores, newest first, a hundred at most.
    pub restores: Vec<v::RestoreView>,
    /// How many backups there are.
    pub total_backups: usize,
    /// Whether the person may create schedules in Velero's namespace.
    pub can_change: Permission,
    /// Whether the person may create restores in Velero's namespace.
    pub can_restore: Permission,
    /// The S3 destination the main location's keys were copied from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys_from: Option<String>,
    /// What could not be read.
    pub unreadable: Vec<UnreadableView>,
}

/// What to plan.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum BackupsPlanRequest {
    /// Set where backups go.
    Location {
        /// The bucket.
        bucket: String,
        /// A folder in the bucket.
        #[serde(default)]
        prefix: String,
        /// The region; `us-east-1` when empty.
        #[serde(default)]
        region: String,
        /// The S3-compatible endpoint, `https://…`; empty for AWS S3.
        #[serde(default)]
        endpoint: String,
        /// The access key ID; empty with the secret to keep the keys the location has.
        #[serde(default, rename = "accessKeyId")]
        access_key_id: String,
        /// The secret access key.
        #[serde(default, rename = "secretAccessKey")]
        secret_access_key: String,
        /// An S3 destination of the cluster's list to copy the keys from, instead of the keys
        /// given.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        destination: Option<String>,
    },
    /// Set Zeabur's schedule.
    Schedule {
        /// Five cron fields, in the Velero server's clock.
        cron: String,
        /// How many days each backup is kept.
        #[serde(rename = "keepDays")]
        keep_days: u32,
        /// The namespaces to back up; empty for every namespace.
        #[serde(default)]
        namespaces: Vec<String>,
    },
    /// Pause Zeabur's schedule, or resume it.
    Pause {
        /// Paused after the change.
        paused: bool,
    },
    /// Back up now with Zeabur's schedule's settings.
    BackupNow,
    /// Restore one namespace of a backup without its routes. While the namespace exists, the
    /// backup goes into the new namespace `into` with its CronJobs paused; when it no longer
    /// exists, back under its own name.
    Restore {
        /// The backup.
        backup: String,
        /// The namespace to restore.
        namespace: String,
        /// The new namespace, needed while `namespace` exists.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        into: Option<String>,
    },
    /// Move every route of `from` to `to`: delete each from `from` and create it in `to`. A route
    /// that Helm, Argo CD or Flux manages stays.
    MoveDomains {
        /// The project the routes leave.
        from: String,
        /// The project they move to.
        to: String,
    },
    /// Restore only the routes of one namespace of a backup into the project `to`. Refused while
    /// another project serves one of their hostnames.
    AttachDomains {
        /// The backup.
        backup: String,
        /// The namespace in the backup.
        namespace: String,
        /// The project the routes go to.
        to: String,
    },
}

impl std::fmt::Debug for BackupsPlanRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Location {
                bucket,
                prefix,
                region,
                endpoint,
                access_key_id,
                destination,
                ..
            } => f
                .debug_struct("Location")
                .field("bucket", bucket)
                .field("prefix", prefix)
                .field("region", region)
                .field("endpoint", endpoint)
                .field("access_key_id", access_key_id)
                .field("destination", destination)
                .finish_non_exhaustive(),
            Self::Schedule {
                cron,
                keep_days,
                namespaces,
            } => f
                .debug_struct("Schedule")
                .field("cron", cron)
                .field("keep_days", keep_days)
                .field("namespaces", namespaces)
                .finish(),
            Self::Pause { paused } => f.debug_struct("Pause").field("paused", paused).finish(),
            Self::BackupNow => f.write_str("BackupNow"),
            Self::Restore {
                backup,
                namespace,
                into,
            } => f
                .debug_struct("Restore")
                .field("backup", backup)
                .field("namespace", namespace)
                .field("into", into)
                .finish(),
            Self::MoveDomains { from, to } => f
                .debug_struct("MoveDomains")
                .field("from", from)
                .field("to", to)
                .finish(),
            Self::AttachDomains {
                backup,
                namespace,
                to,
            } => f
                .debug_struct("AttachDomains")
                .field("backup", backup)
                .field("namespace", namespace)
                .field("to", to)
                .finish(),
        }
    }
}

/// What a write does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum BackupWriteAction {
    /// Creates the object.
    Create,
    /// Changes it.
    Patch,
    /// Deletes it.
    Delete,
}

/// One object a plan writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BackupWrite {
    /// What it does.
    pub action: BackupWriteAction,
    /// The object's kind.
    pub kind: String,
    /// Its name.
    pub name: String,
    /// Its namespace.
    pub namespace: String,
    /// The object as it is, YAML; absent for a creation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_yaml: Option<String>,
    /// The object as it will be, YAML; a Secret's keys are not in it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_yaml: Option<String>,
}

/// A hostname that another project serves.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = HeldDomain)]
pub struct Held {
    /// The hostname.
    pub host: String,
    /// The project whose route serves it.
    pub project: String,
}

/// A plan, and the token that applies it.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BackupsPlan {
    /// Send this to `apply` to carry out exactly this plan.
    pub token: String,
    /// When the token stops working, RFC 3339.
    pub expires_at: String,
    /// The writes, in order; empty when nothing changes.
    pub writes: Vec<BackupWrite>,
    /// The backup a `backupNow` plan creates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup: Option<String>,
    /// The restore a `restore` or `attachDomains` plan creates; absent while `held` is not
    /// empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore: Option<String>,
    /// What a `restore` plan copies, and what to check before it runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy: Option<restore_copy::Copy>,
    /// The hostnames a `moveDomains` or `attachDomains` plan moves or brings back.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hosts: Vec<String>,
    /// The hostnames of an `attachDomains` plan that another project serves. While there are
    /// any, the plan writes nothing and apply refuses it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub held: Vec<Held>,
    /// The routes a `moveDomains` plan leaves, because a tool manages them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub managed: Vec<String>,
}

/// The token of a plan to apply.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct BackupsApply {
    /// The token `plan` returned.
    pub token: String,
}

/// What an apply wrote.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BackupsApplied {
    /// The objects written, `Kind/name`.
    pub written: Vec<String>,
    /// The backup a `backupNow` plan created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup: Option<String>,
    /// The restore a `restore` or `attachDomains` plan created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Claims {
    person: String,
    groups: Vec<String>,
    namespace: String,
    request: BackupsPlanRequest,
    versions: BTreeMap<String, String>,
    /// The backup or restore the plan creates.
    made: Option<String>,
    expires_at: i64,
}

/// What Velero's objects are, read as the person.
pub(crate) struct Objects {
    pub(crate) namespace: String,
    pub(crate) locations: Vec<DynamicObject>,
    schedules: Vec<DynamicObject>,
}

pub(crate) enum Found {
    Read(Objects),
    NotInstalled,
}

pub(crate) fn not_installed() -> ApiError {
    ApiError::from(Problem::new(
        Kind::NotFound,
        "Velero is not installed in the cluster, so backups cannot be set up.",
    ))
}

async fn list(
    cluster: &dyn Cluster,
    read: &zeabur_modules::Read,
    namespace: Option<&str>,
) -> Result<Option<Vec<DynamicObject>>, ApiError> {
    cluster
        .list_objects(&read.resource(), namespace)
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("Velero's {}", read.resource),
                list_read(GROUP, read.resource, namespace),
            )
        })
}

/// The locations and schedules, from which Velero's namespace follows.
pub(crate) async fn objects(
    cluster: &dyn Cluster,
    namespace: Option<&str>,
) -> Result<Found, ApiError> {
    let Some(locations) = list(cluster, &v::LOCATIONS, namespace).await? else {
        return Ok(Found::NotInstalled);
    };
    let schedules = list(cluster, &velero::SCHEDULES, namespace)
        .await?
        .unwrap_or_default();
    let namespace = namespace.map_or_else(
        || v::velero_namespace(&locations, &schedules),
        str::to_owned,
    );
    Ok(Found::Read(Objects {
        locations: in_namespace(locations, &namespace),
        schedules: in_namespace(schedules, &namespace),
        namespace,
    }))
}

fn in_namespace(objects: Vec<DynamicObject>, namespace: &str) -> Vec<DynamicObject> {
    objects
        .into_iter()
        .filter(|o| o.metadata.namespace.as_deref() == Some(namespace))
        .collect()
}

async fn reading(
    cluster: &dyn Cluster,
    read: &zeabur_modules::Read,
    unreadable: &mut Vec<UnreadableView>,
) -> Result<Option<Vec<DynamicObject>>, ApiError> {
    let resource = read.resource();
    match crate::cluster::modules::read(cluster, read)
        .await
        .map_err(|e| ApiError::from(e.into_problem(&resource.key())))?
    {
        Reading::Objects(objects) => Ok(Some(objects)),
        Reading::NotInstalled => Ok(None),
        Reading::Refused(refusal) => {
            unreadable.push(refusal);
            Ok(Some(Vec::new()))
        }
    }
}

fn empty(state: ProviderState, unreadable: Vec<UnreadableView>) -> ClusterBackups {
    ClusterBackups {
        state,
        namespace: v::DEFAULT_NAMESPACE.to_owned(),
        locations: Vec::new(),
        schedules: Vec::new(),
        backups: Vec::new(),
        restores: Vec::new(),
        total_backups: 0,
        can_change: Permission::Unknown,
        can_restore: Permission::Unknown,
        keys_from: None,
        unreadable,
    }
}

/// The cluster's backups through Velero: its locations with whether Velero reaches them, its
/// schedules, and the newest hundred backups with how each went. Read as the person; Velero not
/// installed is `state: not-installed`, and a refused read is named in `unreadable`.
#[utoipa::path(get, path = "/api/v1/cluster/backups", tag = "cluster",
    operation_id = "getClusterBackups",
    responses((status = 200, body = ClusterBackups), (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(Person(cluster): Person) -> Result<Json<ClusterBackups>, ApiError> {
    let cluster = cluster.as_ref();
    let mut unreadable = Vec::new();
    let Some(schedules) = reading(cluster, &velero::SCHEDULES, &mut unreadable).await? else {
        return Ok(Json(empty(ProviderState::NotInstalled, unreadable)));
    };
    let locations = reading(cluster, &v::LOCATIONS, &mut unreadable)
        .await?
        .unwrap_or_default();
    let backups = reading(cluster, &velero::BACKUPS, &mut unreadable)
        .await?
        .unwrap_or_default();
    let refused = unreadable.len();
    let volumes = reading(cluster, &v::POD_VOLUME_BACKUPS, &mut unreadable)
        .await?
        .unwrap_or_default();
    let restores = reading(cluster, &v::RESTORES, &mut unreadable)
        .await?
        .unwrap_or_default();
    if refused == 3 {
        return Ok(Json(empty(ProviderState::Refused, unreadable)));
    }
    let namespace = v::velero_namespace(&locations, &schedules);
    let bytes = v::volume_bytes(&volumes);
    let mut listed: Vec<v::BackupView> = backups.iter().map(|b| v::backup(b, &bytes)).collect();
    listed.sort_by(|a, b| b.time().cmp(&a.time()).then_with(|| a.name.cmp(&b.name)));
    let total_backups = listed.len();
    listed.truncate(SHOWN);
    let mut restores: Vec<v::RestoreView> = restores
        .iter()
        .filter(|r| r.metadata.namespace.as_deref() == Some(namespace.as_str()))
        .map(v::restore)
        .collect();
    // Newest first; one Velero has not started yet is the newest.
    restores.sort_by(|a, b| {
        (b.started.is_none(), &b.started, &b.name).cmp(&(a.started.is_none(), &a.started, &a.name))
    });
    restores.truncate(SHOWN);
    let mut locations: Vec<v::Location> = locations.iter().map(v::location).collect();
    locations.sort_by(|a, b| (!a.default, &a.name).cmp(&(!b.default, &b.name)));
    let keys_from = keys_from(cluster, &namespace, &locations).await;
    let mut schedules: Vec<v::ScheduleView> = schedules.iter().map(v::schedule).collect();
    schedules.sort_by(|a, b| (!a.zeabur, &a.name).cmp(&(!b.zeabur, &b.name)));
    let can_change = may_create(cluster, "schedules", &namespace).await;
    let can_restore = may_create(cluster, "restores", &namespace).await;
    Ok(Json(ClusterBackups {
        state: ProviderState::Installed,
        namespace,
        locations,
        schedules,
        backups: listed,
        restores,
        total_backups,
        can_change,
        can_restore,
        keys_from,
        unreadable,
    }))
}

async fn may_create(cluster: &dyn Cluster, resource: &str, namespace: &str) -> Permission {
    match cluster
        .review_access(&right("create", resource, namespace, None))
        .await
    {
        Ok(true) => Permission::Allowed,
        Ok(false) => Permission::Denied,
        Err(_) => Permission::Unknown,
    }
}

/// The S3 destination the main location's keys were copied from; `None` when unknown.
async fn keys_from(
    cluster: &dyn Cluster,
    namespace: &str,
    locations: &[v::Location],
) -> Option<String> {
    let main = locations
        .iter()
        .find(|l| l.default)
        .or_else(|| locations.iter().find(|l| l.name == v::LOCATION))?;
    let secret = cluster
        .find_object(&secrets(), Some(namespace), main.credential.as_deref()?)
        .await
        .ok()??;
    s3_destinations::copied_from(&secret.metadata)
}

fn secrets() -> ResourceRef {
    ResourceRef::new("", "v1", "secrets", "Secret")
}

pub(crate) fn right(verb: &str, resource: &str, namespace: &str, name: Option<&str>) -> Denied {
    let group = if resource == "secrets" { "" } else { GROUP };
    right_in(group, verb, resource, namespace, name)
}

fn right_in(
    group: &str,
    verb: &str,
    resource: &str,
    namespace: &str,
    name: Option<&str>,
) -> Denied {
    Denied {
        verb: verb.to_owned(),
        group: group.to_owned(),
        resource: resource.to_owned(),
        name: name.map(str::to_owned),
        namespace: (!namespace.is_empty()).then(|| namespace.to_owned()),
    }
}

/// One write of a plan.
struct Planned {
    target: ObjectTarget,
    body: WriteBody,
    action: BackupWriteAction,
    before: Option<Value>,
    shown: Value,
}

/// What a plan reads and writes.
struct Computed {
    versions: BTreeMap<String, String>,
    writes: Vec<Planned>,
    /// What a restore copies, before the backup's list and the room are read.
    copy: Option<restore_copy::Copy>,
    /// The hostnames a move moves or an attach brings back, and the routes a move leaves.
    moved: (Vec<String>, Vec<String>),
    /// The hostnames of an attach that another project serves.
    held: Vec<Held>,
    /// The resource modifier the restore reads, which the restore owns once it exists.
    modifier: Option<ObjectTarget>,
}

pub(crate) fn target(kind: &str, namespace: &str, name: &str) -> ObjectTarget {
    let (api_version, plural) = match kind {
        "Secret" => ("v1", "secrets"),
        "BackupStorageLocation" => ("velero.io/v1", "backupstoragelocations"),
        "Schedule" => ("velero.io/v1", "schedules"),
        "Restore" => ("velero.io/v1", "restores"),
        "ConfigMap" => ("v1", "configmaps"),
        "DownloadRequest" => ("velero.io/v1", "downloadrequests"),
        "Namespace" => ("v1", "namespaces"),
        "Ingress" => ("networking.k8s.io/v1", "ingresses"),
        "HTTPRoute" => ("gateway.networking.k8s.io/v1", "httproutes"),
        _ => ("velero.io/v1", "backups"),
    };
    ObjectTarget {
        api_version: api_version.to_owned(),
        kind: kind.to_owned(),
        plural: plural.to_owned(),
        namespace: namespace.to_owned(),
        name: name.to_owned(),
    }
}

fn version(object: Option<&Value>) -> String {
    object
        .and_then(|o| o.pointer("/metadata/resourceVersion"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn value(object: &DynamicObject) -> Result<Value, ApiError> {
    serde_json::to_value(object).map_err(|e| {
        ApiError::from(Problem::new(
            Kind::Internal,
            format!("Zeabur could not read a Velero object: {e}"),
        ))
    })
}

fn invalid(detail: String) -> ApiError {
    ApiError::from(Problem::new(Kind::Invalid, detail))
}

/// The change of `before` into `after`: a creation, or a patch of `spec` that refuses an object
/// changed since it was read; `None` when nothing changes.
fn change(
    kind: &str,
    namespace: &str,
    name: &str,
    before: Option<Value>,
    after: Value,
) -> Option<Planned> {
    let target = target(kind, namespace, name);
    match before {
        None => Some(Planned {
            target,
            body: WriteBody::Create(after.clone()),
            action: BackupWriteAction::Create,
            before: None,
            shown: after,
        }),
        Some(before) if before.get("spec") == after.get("spec") => None,
        Some(before) => {
            let operations = json!([
                {"op": "test", "path": "/metadata/resourceVersion",
                 "value": version(Some(&before))},
                {"op": "add", "path": "/spec", "value": after.get("spec")},
            ]);
            Some(Planned {
                target,
                body: WriteBody::JsonPatch(operations),
                action: BackupWriteAction::Patch,
                before: Some(before),
                shown: after,
            })
        }
    }
}

/// The location Zeabur edits: the default one, else the one named `default`.
pub(crate) fn chosen_location(locations: &[DynamicObject]) -> Option<&DynamicObject> {
    locations
        .iter()
        .find(|l| l.data.pointer("/spec/default").and_then(Value::as_bool) == Some(true))
        .or_else(|| {
            locations
                .iter()
                .find(|l| l.metadata.name.as_deref() == Some(v::LOCATION))
        })
}

fn zeabur_schedule(schedules: &[DynamicObject]) -> Option<&DynamicObject> {
    schedules
        .iter()
        .find(|s| s.metadata.name.as_deref() == Some(v::SCHEDULE))
}

fn no_schedule() -> ApiError {
    ApiError::from(Problem::new(
        Kind::NotFound,
        format!(
            "Velero has no schedule {:?}; set the schedule up first.",
            v::SCHEDULE
        ),
    ))
}

async fn location_writes(
    cluster: &dyn Cluster,
    objects: &Objects,
    request: &v::LocationRequest,
    keys: Option<(&str, &str)>,
    destination: Option<&str>,
    versions: &mut BTreeMap<String, String>,
) -> Result<Vec<Planned>, ApiError> {
    if let Some(problem) = v::location_problem(request) {
        return Err(invalid(problem));
    }
    let namespace = objects.namespace.as_str();
    let current = chosen_location(&objects.locations).map(value).transpose()?;
    if current.is_none() && keys.is_none() {
        return Err(invalid(
            "Enter the access key ID and the secret access key of the bucket.".to_owned(),
        ));
    }
    let mut writes = Vec::new();
    if let Some((access, secret)) = keys {
        let existing = cluster
            .find_object(&secrets(), Some(namespace), v::CREDENTIALS)
            .await
            .map_err(|e| {
                read_problem(
                    e,
                    &format!("Secret {} in {namespace}", v::CREDENTIALS),
                    get_read("", "secrets", Some(namespace), v::CREDENTIALS),
                )
            })?
            .map(|s| value(&s))
            .transpose()?;
        versions.insert(
            format!("Secret/{}", v::CREDENTIALS),
            version(existing.as_ref()),
        );
        let file = v::credentials_file(access, secret);
        let annotated = |mut secret: Value| {
            if let Some(name) = destination {
                s3_destinations::mark(&mut secret, name, v::CREDENTIALS_KEY);
            }
            secret
        };
        let shown = annotated(v::credentials_secret(namespace, HIDDEN));
        let (body, action) = existing.as_ref().map_or_else(
            || {
                (
                    WriteBody::Create(annotated(v::credentials_secret(namespace, &file))),
                    BackupWriteAction::Create,
                )
            },
            |existing| {
                let mut operations = vec![
                    json!({"op": "test", "path": "/metadata/resourceVersion",
                           "value": version(Some(existing))}),
                    json!({"op": "add", "path": "/stringData",
                           "value": { v::CREDENTIALS_KEY: &file }}),
                ];
                operations.extend(s3_destinations::mark_ops(
                    existing,
                    destination,
                    v::CREDENTIALS_KEY,
                ));
                (
                    WriteBody::JsonPatch(Value::Array(operations)),
                    BackupWriteAction::Patch,
                )
            },
        );
        writes.push(Planned {
            target: target("Secret", namespace, v::CREDENTIALS),
            body,
            action,
            before: None,
            shown,
        });
    }
    let name = current
        .as_ref()
        .and_then(|c| c.pointer("/metadata/name"))
        .and_then(Value::as_str)
        .unwrap_or(v::LOCATION)
        .to_owned();
    versions.insert(
        format!("BackupStorageLocation/{name}"),
        version(current.as_ref()),
    );
    if current.is_none() {
        writes.extend(backup_key_write(cluster, namespace, versions).await?);
    }
    let after = v::location_object(namespace, current.as_ref(), request, keys.is_some());
    writes.extend(change(
        "BackupStorageLocation",
        namespace,
        &name,
        current,
        after,
    ));
    Ok(writes)
}

/// Creates the SSE-C key of a new location when it does not exist. An existing key stays, because
/// the backups it encrypted need it.
async fn backup_key_write(
    cluster: &dyn Cluster,
    namespace: &str,
    versions: &mut BTreeMap<String, String>,
) -> Result<Option<Planned>, ApiError> {
    let existing = cluster
        .find_object(&secrets(), Some(namespace), v::BACKUP_KEY)
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("Secret {} in {namespace}", v::BACKUP_KEY),
                get_read("", "secrets", Some(namespace), v::BACKUP_KEY),
            )
        })?
        .map(|s| value(&s))
        .transpose()?;
    versions.insert(
        format!("Secret/{}", v::BACKUP_KEY),
        version(existing.as_ref()),
    );
    if existing.is_some() {
        return Ok(None);
    }
    // 24 random bytes in base64url are the 32 bytes the AWS plugin needs.
    let key = zeabur_core::crypto::random::<24>()
        .map(|b| URL_SAFE_NO_PAD.encode(b))
        .map_err(|_random| {
            ApiError::from(Problem::new(
                Kind::Internal,
                "The system random number generator failed, so Zeabur could not make the backup key.",
            ))
        })?;
    Ok(Some(Planned {
        target: target("Secret", namespace, v::BACKUP_KEY),
        body: WriteBody::Create(v::backup_key_secret(namespace, &key)),
        action: BackupWriteAction::Create,
        before: None,
        shown: v::backup_key_secret(namespace, HIDDEN),
    }))
}

fn namespaces() -> ResourceRef {
    ResourceRef::new("", "v1", "namespaces", "Namespace")
}

async fn namespace_object(cluster: &dyn Cluster, name: &str) -> Result<Option<Value>, ApiError> {
    cluster
        .find_object(&namespaces(), None, name)
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("namespace {name}"),
                get_read("", "namespaces", None, name),
            )
        })?
        .map(|n| value(&n))
        .transpose()
}

/// The routes of `namespace` as JSON with their `apiVersion` and `kind`, read as the person.
async fn routes_of(cluster: &dyn Cluster, namespace: &str) -> Result<Vec<Value>, ApiError> {
    let mut out = Vec::new();
    for resource in routes() {
        let listed = cluster
            .list_objects(&resource, Some(namespace))
            .await
            .map_err(|e| {
                read_problem(
                    e,
                    &format!("{} in {namespace}", resource.resource),
                    list_read(&resource.group, &resource.resource, Some(namespace)),
                )
            })?;
        for route in listed.into_iter().flatten() {
            let mut route = value(&route)?;
            if let Some(map) = route.as_object_mut() {
                map.insert("apiVersion".to_owned(), json!(resource.api_version()));
                map.insert("kind".to_owned(), json!(resource.kind));
            }
            out.push(route);
        }
    }
    Ok(out)
}

/// The namespace of a copy: a project like a new one, at the Pod Security level of the
/// original, so its pods run there as they ran in the original.
fn copy_namespace(into: &str, source: &Value) -> Value {
    let mut namespace = zeabur_plan::project_namespace(
        &zeabur_plan::Conventions::ZEABUR,
        into,
        None,
        PodSecurityLevel::Restricted,
    );
    let enforce = source
        .pointer(&format!(
            "/metadata/labels/{}",
            PodSecurityLevel::ENFORCE.replace('/', "~1")
        ))
        .and_then(Value::as_str)
        .and_then(PodSecurityLevel::parse);
    if let Some(labels) = namespace
        .pointer_mut("/metadata/labels")
        .and_then(Value::as_object_mut)
    {
        for key in [
            PodSecurityLevel::ENFORCE,
            PodSecurityLevel::AUDIT,
            PodSecurityLevel::WARN,
        ] {
            match enforce {
                Some(level) => {
                    labels.insert(key.to_owned(), json!(level.as_str()));
                }
                None => {
                    labels.remove(key);
                }
            }
        }
    }
    namespace
}

/// The backup `backup` in Velero's namespace `namespace`, once it has finished and holds
/// `source`; its version goes into `versions`.
async fn finished_backup(
    cluster: &dyn Cluster,
    namespace: &str,
    backup: &str,
    source: &str,
    versions: &mut BTreeMap<String, String>,
) -> Result<(), ApiError> {
    let found = cluster
        .find_object(&velero::BACKUPS.resource(), Some(namespace), backup)
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("backup {backup}"),
                get_read(GROUP, "backups", Some(namespace), backup),
            )
        })?
        .ok_or_else(|| {
            ApiError::from(Problem::new(
                Kind::NotFound,
                format!("Velero has no backup {backup:?}."),
            ))
        })?;
    let view = v::backup(&found, &BTreeMap::new());
    if !matches!(view.outcome, Outcome::Succeeded | Outcome::PartiallyFailed) {
        return Err(invalid(format!(
            "Backup {backup} has not finished, so it cannot be restored."
        )));
    }
    if let Some(problem) = v::restore_problem(&view.namespaces, source) {
        return Err(invalid(problem));
    }
    versions.insert(format!("Backup/{backup}"), version(Some(&value(&found)?)));
    Ok(())
}

/// The restore of `source` from `backup`, named `made`, without routes: into the new project
/// `into` with its CronJobs paused while `source` exists, else under its own name. The backup
/// must have finished.
#[expect(
    clippy::too_many_arguments,
    reason = "one per part of the request and of the plan"
)]
async fn restore_writes(
    cluster: &dyn Cluster,
    namespace: &str,
    backup: &str,
    source: &str,
    into: Option<&str>,
    made: Option<&str>,
    versions: &mut BTreeMap<String, String>,
    computed: &mut Computed,
) -> Result<Vec<Planned>, ApiError> {
    finished_backup(cluster, namespace, backup, source, versions).await?;
    let original = namespace_object(cluster, source).await?;
    versions.insert(format!("Namespace/{source}"), version(original.as_ref()));
    let name = made.unwrap_or_default();
    let mut writes = Vec::new();
    let into = match &original {
        None => None,
        Some(original) => {
            let into = into.map_or("", str::trim);
            if let Some(problem) = v::new_namespace_problem(source, into) {
                return Err(invalid(problem));
            }
            if namespace_object(cluster, into).await?.is_some() {
                return Err(ApiError::from(Problem::new(
                    Kind::Conflict,
                    format!("A namespace named {into} already exists; choose another name."),
                )));
            }
            versions.insert(format!("Namespace/{into}"), String::new());
            writes.extend(change(
                "Namespace",
                "",
                into,
                None,
                copy_namespace(into, original),
            ));
            let modifier = v::paused_cron_jobs(namespace, name, source);
            writes.extend(change("ConfigMap", namespace, name, None, modifier));
            computed.modifier = Some(target("ConfigMap", namespace, name));
            Some(into)
        }
    };
    computed.copy = Some(restore_copy::Copy {
        source: source.to_owned(),
        into: into.unwrap_or(source).to_owned(),
        same_name: into.is_none(),
        inventory: None,
        unread: None,
        room: None,
    });
    let object = v::restore_object(namespace, name, backup, source, into);
    writes.extend(change("Restore", namespace, name, None, object));
    Ok(writes)
}

/// The hostnames among `hosts` that a route outside `to` serves, read as the person across the
/// cluster.
async fn held_elsewhere(
    cluster: &dyn Cluster,
    hosts: &[String],
    to: &str,
) -> Result<Vec<Held>, ApiError> {
    let mut held = Vec::new();
    for resource in routes() {
        let listed = cluster.list_objects(&resource, None).await.map_err(|e| {
            read_problem(
                e,
                &resource.resource,
                list_read(&resource.group, &resource.resource, None),
            )
        })?;
        for route in listed.into_iter().flatten() {
            let project = route.metadata.namespace.clone().unwrap_or_default();
            if project == to {
                continue;
            }
            for host in v::route_hosts(&value(&route)?) {
                if hosts.contains(&host) {
                    held.push(Held {
                        host,
                        project: project.clone(),
                    });
                }
            }
        }
    }
    held.sort();
    held.dedup();
    Ok(held)
}

/// The restore of only the routes of `source` in `backup` into `to`, named `made`; nothing
/// while another project serves one of their hostnames.
#[expect(
    clippy::too_many_arguments,
    reason = "one per part of the request and of the plan"
)]
async fn attach_writes(
    state: &AppState,
    cluster: &dyn Cluster,
    namespace: &str,
    backup: &str,
    source: &str,
    to: &str,
    made: Option<&str>,
    versions: &mut BTreeMap<String, String>,
    computed: &mut Computed,
) -> Result<Vec<Planned>, ApiError> {
    finished_backup(cluster, namespace, backup, source, versions).await?;
    let project = namespace_object(cluster, to).await?.ok_or_else(|| {
        ApiError::from(Problem::new(
            Kind::NotFound,
            format!("There is no namespace {to}."),
        ))
    })?;
    versions.insert(format!("Namespace/{to}"), version(Some(&project)));
    let mut hosts: Vec<String> = backup_download::routes(state, cluster, backup, source)
        .await?
        .iter()
        .flat_map(v::route_hosts)
        .collect();
    hosts.sort();
    hosts.dedup();
    if hosts.is_empty() {
        return Err(invalid(format!(
            "Backup {backup} holds no domains of {source}."
        )));
    }
    computed.held = held_elsewhere(cluster, &hosts, to).await?;
    computed.moved.0 = hosts;
    if !computed.held.is_empty() {
        return Ok(Vec::new());
    }
    let name = made.unwrap_or_default();
    let object = v::domains_restore(namespace, name, backup, source, to);
    Ok(change("Restore", namespace, name, None, object)
        .into_iter()
        .collect())
}

/// The move of every route of `from` to `to`; each is deleted from `from`, then created in `to`.
async fn move_writes(
    cluster: &dyn Cluster,
    from: &str,
    to: &str,
    versions: &mut BTreeMap<String, String>,
) -> Result<(Vec<Planned>, Vec<String>, Vec<String>), ApiError> {
    if from == to {
        return Err(invalid(
            "Choose another project to move the domains to.".to_owned(),
        ));
    }
    for name in [from, to] {
        if namespace_object(cluster, name).await?.is_none() {
            return Err(ApiError::from(Problem::new(
                Kind::NotFound,
                format!("There is no namespace {name}."),
            )));
        }
    }
    let mut writes = Vec::new();
    let mut hosts = Vec::new();
    let mut managed = Vec::new();
    for before in routes_of(cluster, from).await? {
        let text = |pointer: &str| {
            before
                .pointer(pointer)
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        let kind = text("/kind").unwrap_or_default();
        let name = text("/metadata/name").unwrap_or_default();
        let metadata: ObjectMeta =
            serde_json::from_value(before.get("metadata").cloned().unwrap_or_default())
                .unwrap_or_default();
        if zeabur_model::adapters::provenance::detect_tool(&metadata).is_some() {
            managed.push(format!("{kind}/{name}"));
            continue;
        }
        let taken = cluster
            .find_object(&target(&kind, to, &name).resource(), Some(to), &name)
            .await
            .map_err(|e| {
                read_problem(
                    e,
                    &format!("{kind} {name} in {to}"),
                    get_read(
                        target(&kind, to, &name).resource().group.as_str(),
                        &target(&kind, to, &name).plural,
                        Some(to),
                        &name,
                    ),
                )
            })?;
        if taken.is_some() {
            return Err(ApiError::from(Problem::new(
                Kind::Conflict,
                format!("{to} already has a {kind} named {name}; delete or rename it first."),
            )));
        }
        versions.insert(format!("{kind}/{from}/{name}"), version(Some(&before)));
        hosts.extend(v::route_hosts(&before));
        writes.push(Planned {
            target: target(&kind, from, &name),
            body: WriteBody::Delete(zeabur_kube::Preconditions {
                uid: metadata.uid.clone(),
                resource_version: metadata.resource_version.clone(),
                grace_period_seconds: None,
            }),
            action: BackupWriteAction::Delete,
            before: Some(before.clone()),
            shown: Value::Null,
        });
        let moved = v::moved_route(&before, to);
        writes.extend(change(&kind, to, &name, None, moved));
    }
    hosts.sort();
    hosts.dedup();
    Ok((writes, hosts, managed))
}

/// The writes of `request`; `made` names the backup or restore the plan creates.
#[expect(clippy::too_many_lines, reason = "one arm per request kind")]
async fn compute(
    state: &AppState,
    cluster: &dyn Cluster,
    objects: &Objects,
    request: &BackupsPlanRequest,
    made: Option<&str>,
) -> Result<Computed, ApiError> {
    let namespace = objects.namespace.as_str();
    let mut versions = BTreeMap::new();
    let mut computed = Computed {
        versions: BTreeMap::new(),
        writes: Vec::new(),
        copy: None,
        moved: (Vec::new(), Vec::new()),
        held: Vec::new(),
        modifier: None,
    };
    let schedule = zeabur_schedule(&objects.schedules).map(value).transpose()?;
    let writes = match request {
        BackupsPlanRequest::Location {
            bucket,
            prefix,
            region,
            endpoint,
            access_key_id,
            secret_access_key,
            destination,
        } => {
            let copied = match destination {
                Some(name) => Some(s3_destinations::keys(cluster, name).await?),
                None => None,
            };
            let keys = match (access_key_id.trim(), secret_access_key.trim()) {
                _ if copied.is_some() => copied.as_ref().map(|(a, s)| (a.as_str(), s.as_str())),
                ("", "") => None,
                ("", _) | (_, "") => {
                    return Err(invalid(
                        "Enter both the access key ID and the secret access key, or neither to \
                         keep the keys the location has."
                            .to_owned(),
                    ));
                }
                (access, secret) => Some((access, secret)),
            };
            let location = v::LocationRequest {
                bucket: bucket.clone(),
                prefix: prefix.clone(),
                region: region.clone(),
                endpoint: endpoint.clone(),
            };
            location_writes(
                cluster,
                objects,
                &location,
                keys,
                destination.as_deref(),
                &mut versions,
            )
            .await?
        }
        BackupsPlanRequest::Schedule {
            cron,
            keep_days,
            namespaces,
        } => {
            let request = v::ScheduleRequest {
                cron: cron.clone(),
                keep_days: *keep_days,
                namespaces: namespaces.clone(),
            };
            if let Some(problem) = v::schedule_problem(&request) {
                return Err(invalid(problem));
            }
            versions.insert(
                format!("Schedule/{}", v::SCHEDULE),
                version(schedule.as_ref()),
            );
            let after = v::schedule_object(namespace, schedule.as_ref(), &request);
            change("Schedule", namespace, v::SCHEDULE, schedule, after)
                .into_iter()
                .collect()
        }
        BackupsPlanRequest::Pause { paused } => {
            let current = schedule.ok_or_else(no_schedule)?;
            versions.insert(format!("Schedule/{}", v::SCHEDULE), version(Some(&current)));
            let after = v::paused(&current, *paused);
            change("Schedule", namespace, v::SCHEDULE, Some(current), after)
                .into_iter()
                .collect()
        }
        BackupsPlanRequest::BackupNow => {
            let current = schedule.ok_or_else(no_schedule)?;
            versions.insert(format!("Schedule/{}", v::SCHEDULE), version(Some(&current)));
            let name = made.unwrap_or_default();
            let made = v::backup_now(&current, name);
            change("Backup", namespace, name, None, made)
                .into_iter()
                .collect()
        }
        BackupsPlanRequest::Restore {
            backup,
            namespace: source,
            into,
        } => {
            restore_writes(
                cluster,
                namespace,
                backup,
                source,
                into.as_deref(),
                made,
                &mut versions,
                &mut computed,
            )
            .await?
        }
        BackupsPlanRequest::MoveDomains { from, to } => {
            let (writes, hosts, managed) = move_writes(cluster, from, to, &mut versions).await?;
            computed.moved = (hosts, managed);
            writes
        }
        BackupsPlanRequest::AttachDomains {
            backup,
            namespace: source,
            to,
        } => {
            attach_writes(
                state,
                cluster,
                namespace,
                backup,
                source,
                to,
                made,
                &mut versions,
                &mut computed,
            )
            .await?
        }
    };
    computed.versions = versions;
    computed.writes = writes;
    Ok(computed)
}

fn yaml(object: &Value) -> Option<String> {
    zeabur_graph::manifest::to_yaml(&zeabur_graph::manifest::clean(object)).ok()
}

fn plan_key(state: &AppState) -> Result<std::sync::Arc<token::PlanKey>, ApiError> {
    state.plan_key.clone().ok_or_else(|| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "Zeabur could not make a token key when it started, so it cannot plan changes.",
        ))
    })
}

/// Works out what setting the backup location or Zeabur's schedule, pausing or resuming it,
/// backing up now, restoring a project, or bringing back or moving its domains writes, and a
/// token to apply it; writes nothing. The keys of a location never appear in the answer.
#[utoipa::path(post, path = "/api/v1/cluster/backups/plan", tag = "cluster",
    operation_id = "planClusterBackups", request_body = BackupsPlanRequest,
    responses((status = 200, body = BackupsPlan),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn plan(
    State(state): State<AppState>,
    Person(cluster): Person,
    Json(request): Json<BackupsPlanRequest>,
) -> Result<Json<BackupsPlan>, ApiError> {
    let sealing = plan_key(&state)?;
    let identity = cluster
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let Found::Read(objects) = objects(cluster.as_ref(), None).await? else {
        return Err(not_installed());
    };
    let now = (state.clock)();
    let at = now.strftime("%Y%m%d%H%M%S").to_string();
    let made = match &request {
        BackupsPlanRequest::BackupNow => Some(v::backup_name(v::SCHEDULE, &at)),
        BackupsPlanRequest::Restore { backup, .. }
        | BackupsPlanRequest::AttachDomains { backup, .. } => Some(v::backup_name(backup, &at)),
        BackupsPlanRequest::Location { .. }
        | BackupsPlanRequest::Schedule { .. }
        | BackupsPlanRequest::Pause { .. }
        | BackupsPlanRequest::MoveDomains { .. } => None,
    };
    let mut computed = compute(
        &state,
        cluster.as_ref(),
        &objects,
        &request,
        made.as_deref(),
    )
    .await?;
    if let (Some(copy), BackupsPlanRequest::Restore { backup, .. }) =
        (computed.copy.as_mut(), &request)
    {
        match backup_download::resource_list(&state, cluster.as_ref(), backup).await {
            Ok(list) => copy.inventory = Some(v::inventory(&list, &copy.source)),
            Err(e) => copy.unread = Some(e.0.detail),
        }
        if !copy.same_name {
            restore_copy::read_room(cluster.as_ref(), copy).await;
        }
    }
    let restoring = restores(&request);
    let (backup, restore) = if !restoring {
        (made.clone(), None)
    } else if computed.held.is_empty() {
        (None, made.clone())
    } else {
        (None, None)
    };
    let expires_at = now.as_second().saturating_add(LIFETIME_SECONDS);
    let claims = Claims {
        person: identity.username,
        groups: identity.groups,
        namespace: objects.namespace.clone(),
        request,
        versions: computed.versions,
        made,
        expires_at,
    };
    let token = token::sign_as(&sealing, PREFIX, &claims).map_err(|_random| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "The system random number generator failed, so Zeabur could not seal the plan.",
        ))
    })?;
    Ok(Json(BackupsPlan {
        token,
        expires_at: Timestamp::from_second(expires_at)
            .unwrap_or(Timestamp::UNIX_EPOCH)
            .to_string(),
        writes: computed
            .writes
            .iter()
            .map(|w| BackupWrite {
                action: w.action,
                kind: w.target.kind.clone(),
                name: w.target.name.clone(),
                namespace: w.target.namespace.clone(),
                before_yaml: w.before.as_ref().and_then(yaml),
                after_yaml: (w.action != BackupWriteAction::Delete)
                    .then(|| yaml(&w.shown))
                    .flatten(),
            })
            .collect(),
        backup,
        restore,
        copy: computed.copy,
        hosts: computed.moved.0,
        held: computed.held,
        managed: computed.moved.1,
    }))
}

/// Whether `request` creates a `Restore`.
const fn restores(request: &BackupsPlanRequest) -> bool {
    matches!(
        request,
        BackupsPlanRequest::Restore { .. } | BackupsPlanRequest::AttachDomains { .. }
    )
}

/// Makes the `Restore` `name` in `namespace` own the resource modifier `modifier`, so the garbage
/// collector deletes it with the restore.
async fn own_modifier(
    cluster: &dyn Cluster,
    namespace: &str,
    name: &str,
    modifier: &ObjectTarget,
) -> Result<(), ApiError> {
    let restore = cluster
        .find_object(&v::RESTORES.resource(), Some(namespace), name)
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("restore {name}"),
                get_read(GROUP, "restores", Some(namespace), name),
            )
        })?;
    let Some(patch) = restore.as_ref().and_then(v::owned_by) else {
        return Ok(());
    };
    send(
        cluster,
        &Planned {
            target: modifier.clone(),
            body: WriteBody::JsonPatch(patch),
            action: BackupWriteAction::Patch,
            before: None,
            shown: Value::Null,
        },
        false,
    )
    .await
}

async fn send(cluster: &dyn Cluster, write: &Planned, dry_run: bool) -> Result<(), ApiError> {
    let request = WriteRequest {
        target: write.target.clone(),
        body: write.body.clone(),
        dry_run,
    };
    let verb = match write.action {
        BackupWriteAction::Create => "create",
        BackupWriteAction::Patch => "patch",
        BackupWriteAction::Delete => "delete",
    };
    let target = &write.target;
    cluster
        .write(&request)
        .await
        .map(|_| ())
        .map_err(|error: ClusterError| {
            read_problem(
                error,
                &format!("{} {} in {}", target.kind, target.name, target.namespace),
                right_in(
                    target
                        .api_version
                        .split_once('/')
                        .map_or("", |(group, _)| group),
                    verb,
                    &target.plural,
                    &target.namespace,
                    (write.action != BackupWriteAction::Create).then_some(target.name.as_str()),
                ),
            )
        })
}

/// Carries out a backups plan by its token: checks it, dry-runs every write, then writes.
#[utoipa::path(post, path = "/api/v1/cluster/backups/apply", tag = "cluster",
    operation_id = "applyClusterBackups", request_body = BackupsApply,
    responses((status = 200, body = BackupsApplied),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn apply(
    State(state): State<AppState>,
    Person(cluster): Person,
    Json(request): Json<BackupsApply>,
) -> Result<Json<BackupsApplied>, ApiError> {
    let sealing = plan_key(&state)?;
    let claims: Claims = token::open_as(&sealing, PREFIX, &request.token).map_err(token_problem)?;
    if (state.clock)().as_second() >= claims.expires_at {
        return Err(token_problem(TokenError::Expired));
    }
    let identity = cluster
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    if (identity.username.as_str(), identity.groups.as_slice())
        != (claims.person.as_str(), claims.groups.as_slice())
    {
        return Err(ApiError::from(Problem::new(
            Kind::Forbidden,
            "Someone else made this plan, or you made it with other group memberships.",
        )));
    }
    let cluster = cluster.as_ref();
    let Found::Read(objects) = objects(cluster, Some(&claims.namespace)).await? else {
        return Err(not_installed());
    };
    let computed = compute(
        &state,
        cluster,
        &objects,
        &claims.request,
        claims.made.as_deref(),
    )
    .await?;
    if let Some(held) = computed.held.first() {
        return Err(ApiError::from(Problem::new(
            Kind::Conflict,
            format!(
                "{} serves {}; move its domains first.",
                held.project, held.host
            ),
        )));
    }
    if computed.versions != claims.versions {
        return Err(ApiError::from(Problem::new(
            Kind::Conflict,
            "Velero's location, schedule, keys or backup, or a namespace or route of the plan, \
             changed after the plan was made.",
        )));
    }
    let restoring = restores(&claims.request);
    for write in &computed.writes {
        send(cluster, write, true).await?;
    }
    for write in &computed.writes {
        send(cluster, write, false).await?;
    }
    if let (Some(modifier), Some(name)) = (&computed.modifier, &claims.made) {
        own_modifier(cluster, &objects.namespace, name, modifier).await?;
    }
    Ok(Json(BackupsApplied {
        written: computed
            .writes
            .iter()
            .map(|w| format!("{}/{}", w.target.kind, w.target.name))
            .collect(),
        backup: claims.made.clone().filter(|_| !restoring),
        restore: claims.made.filter(|_| restoring),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_leaves_the_secret_access_key_out() {
        let request = BackupsPlanRequest::Location {
            bucket: "backups".to_owned(),
            prefix: String::new(),
            region: String::new(),
            endpoint: String::new(),
            access_key_id: "AKIA".to_owned(),
            secret_access_key: "S3CR3T".to_owned(),
            destination: None,
        };
        assert!(!format!("{request:?}").contains("S3CR3T"));
    }
}
