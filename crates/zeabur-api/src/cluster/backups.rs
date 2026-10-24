//! The cluster's backups through Velero: where they go, when they run, how each went, and the
//! plans that set them up (see [`zeabur_modules::velero::cluster`]).
//!
//! | Route | What it does |
//! |---|---|
//! | `GET /api/v1/cluster/backups` | Velero's locations, schedules and backups, or that it is not installed |
//! | `POST /api/v1/cluster/backups/plan` | what setting the location or the schedule, pausing it, or backing up now writes, and a token; writes nothing |
//! | `POST /api/v1/cluster/backups/apply` | carries out that plan by its token |
//!
//! Everything is read and written as the person; RBAC on `velero.io` and on Secrets in Velero's
//! namespace decides. Velero's API not being served is an answer (`not-installed`), not an error.
//! The token is sealed, so the keys a person types travel in it unreadable; it binds the person,
//! Velero's namespace, the request, the name of a backup made now and the `resourceVersion` of
//! every object the plan reads. Apply reads them again, refuses when one moved, dry-runs every
//! write, then writes. A plan's YAML never holds the keys.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use zeabur_core::problem::{Denied, Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{
    Cluster, ClusterError, ClusterExt as _, DynamicObject, ObjectTarget, ResourceRef, WriteBody,
    WriteRequest,
};
use zeabur_modules::Reading;
use zeabur_modules::velero::{self, cluster as v};

use crate::cluster::s3_destinations;
use crate::error::{ApiError, get_read, list_read, read_problem};
use crate::handlers::{AppState, Everyday, Person};
use crate::plan_token::{self as token, LIFETIME_SECONDS, TokenError};
use crate::projects::backups::ProviderState;
use crate::projects::namespace::Permission;
use crate::projects::writes::token_problem;
use crate::views::UnreadableView;

/// Sealed plans of this module start with this, so no other plan's token opens as one.
const PREFIX: &str = "b1.";
const GROUP: &str = "velero.io";
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
    /// How many backups there are.
    pub total_backups: usize,
    /// Whether the person may create schedules in Velero's namespace.
    pub can_change: Permission,
    /// The S3 destination the main location's keys were copied from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys_from: Option<String>,
    /// What could not be read.
    pub unreadable: Vec<UnreadableView>,
}

/// What to plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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
}

/// What a write does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum BackupWriteAction {
    /// Creates the object.
    Create,
    /// Changes it.
    Patch,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Claims {
    person: String,
    groups: Vec<String>,
    namespace: String,
    request: BackupsPlanRequest,
    versions: BTreeMap<String, String>,
    backup: Option<String>,
    expires_at: i64,
}

/// What Velero's objects are, read as the person.
struct Objects {
    namespace: String,
    locations: Vec<DynamicObject>,
    schedules: Vec<DynamicObject>,
}

enum Found {
    Read(Objects),
    NotInstalled,
}

fn not_installed() -> ApiError {
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
async fn objects(cluster: &dyn Cluster, namespace: Option<&str>) -> Result<Found, ApiError> {
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
        total_backups: 0,
        can_change: Permission::Unknown,
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
    if refused == 3 {
        return Ok(Json(empty(ProviderState::Refused, unreadable)));
    }
    let namespace = v::velero_namespace(&locations, &schedules);
    let bytes = v::volume_bytes(&volumes);
    let mut listed: Vec<v::BackupView> = backups.iter().map(|b| v::backup(b, &bytes)).collect();
    listed.sort_by(|a, b| b.time().cmp(&a.time()).then_with(|| a.name.cmp(&b.name)));
    let total_backups = listed.len();
    listed.truncate(SHOWN);
    let mut locations: Vec<v::Location> = locations.iter().map(v::location).collect();
    locations.sort_by(|a, b| (!a.default, &a.name).cmp(&(!b.default, &b.name)));
    let keys_from = keys_from(cluster, &namespace, &locations).await;
    let mut schedules: Vec<v::ScheduleView> = schedules.iter().map(v::schedule).collect();
    schedules.sort_by(|a, b| (!a.zeabur, &a.name).cmp(&(!b.zeabur, &b.name)));
    let can_change = match cluster
        .review_access(&right("create", "schedules", &namespace, None))
        .await
    {
        Ok(true) => Permission::Allowed,
        Ok(false) => Permission::Denied,
        Err(_) => Permission::Unknown,
    };
    Ok(Json(ClusterBackups {
        state: ProviderState::Installed,
        namespace,
        locations,
        schedules,
        backups: listed,
        total_backups,
        can_change,
        keys_from,
        unreadable,
    }))
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

fn right(verb: &str, resource: &str, namespace: &str, name: Option<&str>) -> Denied {
    let group = if resource == "secrets" { "" } else { GROUP };
    Denied {
        verb: verb.to_owned(),
        group: group.to_owned(),
        resource: resource.to_owned(),
        name: name.map(str::to_owned),
        namespace: Some(namespace.to_owned()),
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
}

fn target(kind: &str, namespace: &str, name: &str) -> ObjectTarget {
    let (api_version, plural) = match kind {
        "Secret" => ("v1", "secrets"),
        "BackupStorageLocation" => ("velero.io/v1", "backupstoragelocations"),
        "Schedule" => ("velero.io/v1", "schedules"),
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
fn chosen_location(locations: &[DynamicObject]) -> Option<&DynamicObject> {
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

/// The writes of `request`; `backup` names the backup a `backupNow` plan creates.
async fn compute(
    cluster: &dyn Cluster,
    objects: &Objects,
    request: &BackupsPlanRequest,
    backup: Option<&str>,
) -> Result<Computed, ApiError> {
    let namespace = objects.namespace.as_str();
    let mut versions = BTreeMap::new();
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
            let name = backup.unwrap_or_default();
            let made = v::backup_now(&current, name);
            change("Backup", namespace, name, None, made)
                .into_iter()
                .collect()
        }
    };
    Ok(Computed { versions, writes })
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

/// Works out what setting the backup location or Zeabur's schedule, pausing or resuming it, or
/// backing up now writes, and a token to apply it; writes nothing. The keys of a location never
/// appear in the answer.
#[utoipa::path(post, path = "/api/v1/cluster/backups/plan", tag = "cluster",
    operation_id = "planClusterBackups", request_body = BackupsPlanRequest,
    responses((status = 200, body = BackupsPlan),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn plan(
    State(state): State<AppState>,
    Person(cluster): Person,
    Everyday(everyday): Everyday,
    Json(request): Json<BackupsPlanRequest>,
) -> Result<Json<BackupsPlan>, ApiError> {
    let sealing = plan_key(&state)?;
    let identity = everyday
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let Found::Read(objects) = objects(cluster.as_ref(), None).await? else {
        return Err(not_installed());
    };
    let now = (state.clock)();
    let backup = matches!(request, BackupsPlanRequest::BackupNow)
        .then(|| v::backup_name(v::SCHEDULE, &now.strftime("%Y%m%d%H%M%S").to_string()));
    let computed = compute(cluster.as_ref(), &objects, &request, backup.as_deref()).await?;
    let expires_at = now.as_second().saturating_add(LIFETIME_SECONDS);
    let claims = Claims {
        person: identity.username,
        groups: identity.groups,
        namespace: objects.namespace.clone(),
        request,
        versions: computed.versions,
        backup: backup.clone(),
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
                after_yaml: yaml(&w.shown),
            })
            .collect(),
        backup,
    }))
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
                right(
                    verb,
                    &target.plural,
                    &target.namespace,
                    (write.action == BackupWriteAction::Patch).then_some(target.name.as_str()),
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
    Everyday(everyday): Everyday,
    Json(request): Json<BackupsApply>,
) -> Result<Json<BackupsApplied>, ApiError> {
    let sealing = plan_key(&state)?;
    let claims: Claims = token::open_as(&sealing, PREFIX, &request.token).map_err(token_problem)?;
    if (state.clock)().as_second() >= claims.expires_at {
        return Err(token_problem(TokenError::Expired));
    }
    let identity = everyday
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
    let computed = compute(cluster, &objects, &claims.request, claims.backup.as_deref()).await?;
    if computed.versions != claims.versions {
        return Err(ApiError::from(Problem::new(
            Kind::Conflict,
            "Velero's location, schedule or keys changed after the plan was made.",
        )));
    }
    for write in &computed.writes {
        send(cluster, write, true).await?;
    }
    for write in &computed.writes {
        send(cluster, write, false).await?;
    }
    Ok(Json(BackupsApplied {
        written: computed
            .writes
            .iter()
            .map(|w| format!("{}/{}", w.target.kind, w.target.name))
            .collect(),
        backup: claims.backup,
    }))
}
