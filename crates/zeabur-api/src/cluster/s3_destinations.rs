//! The cluster's list of S3 destinations: buckets that administrators register once, so that
//! backup forms can offer them instead of asking for the endpoint and the keys each time.
//!
//! | Route | What it does |
//! |---|---|
//! | `GET /api/v1/cluster/s3-destinations` | the destinations, without their keys, and what the person may do with them |
//! | `POST /api/v1/cluster/s3-destinations/plan` | what saving or removing a destination writes, and a token; writes nothing |
//! | `POST /api/v1/cluster/s3-destinations/apply` | carries out that plan by its token |
//!
//! A destination is two objects named after it in the namespace [`NAMESPACE`], both labelled
//! [`LABEL`]: a ConfigMap with the endpoint, the bucket, the region and the folder, and a Secret
//! with the keys (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`). Everything is read and written as
//! the person, so RBAC in that namespace decides who may register a destination and who may use
//! its keys. Zeabur's chart lets everyone signed in read the ConfigMaps there, and nobody the
//! Secrets.
//!
//! The list is only a picker. A backup never reads a destination: a plan copies the keys into a
//! Secret of the backup's own namespace (`open` and `keys`), and annotates that Secret with
//! [`LABEL`] and the destination's name. A change of the destination's keys does not reach the
//! copies, and backups keep running when Zeabur or the list is removed.
//!
//! The plan token is sealed, so the keys a person types travel in it unreadable. It binds the
//! person, the request and the `resourceVersion` of every object the plan reads. Apply reads
//! them again, refuses when one moved, dry-runs every write, then writes. A plan's YAML never
//! holds a key.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use zeabur_core::problem::{Denied, Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{
    Cluster, ClusterExt as _, DynamicObject, ObjectTarget, Preconditions, ResourceRef, WriteBody,
    WriteRequest,
};
use zeabur_model::Origin;
use zeabur_plan::{Change, CopyS3KeysChange, PlanRequest};

use crate::error::{ApiError, get_read, read_problem};
use crate::handlers::{AppState, Everyday, Person};
use crate::plan_token::{self as token, LIFETIME_SECONDS, TokenError};
use crate::projects::namespace::Permission;
use crate::projects::writes::token_problem;
use crate::views::UnreadableView;

/// The namespace the destinations are kept in.
pub const NAMESPACE: &str = "zeabur-s3-destinations";
/// The label of a destination's objects, and the annotation of a Secret its keys were copied
/// into; the value is the destination's name.
pub const LABEL: &str = "zeabur.com/s3-destination";
const ACCESS_KEY_ID: &str = "AWS_ACCESS_KEY_ID";
const SECRET_ACCESS_KEY: &str = "AWS_SECRET_ACCESS_KEY";
const PREFIX: &str = "s3d1.";
const HIDDEN: &str = "(hidden)";

/// The cluster's S3 destinations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct S3Destinations {
    /// The namespace they are kept in.
    pub namespace: String,
    /// The destinations, by name.
    pub items: Vec<S3Destination>,
    /// Whether the person may add, change and remove destinations.
    pub can_change: Permission,
    /// Whether the person may read the keys, which copying them into a project needs.
    pub can_use_keys: Permission,
    /// What could not be read.
    pub unreadable: Vec<UnreadableView>,
}

/// One S3 destination, without its keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct S3Destination {
    /// Its name.
    pub name: String,
    /// The S3-compatible endpoint, `https://…`.
    pub endpoint: String,
    /// The bucket.
    pub bucket: String,
    /// The region; empty when the service takes any.
    pub region: String,
    /// A folder in the bucket that backups go under; empty for the bucket's root.
    pub prefix: String,
    /// The ConfigMap.
    pub origin: Origin,
}

/// A destination's fields, and its keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct S3DestinationFields {
    /// Its name: lower-case letters, digits and `-`, at most 63.
    pub name: String,
    /// The S3-compatible endpoint, `https://…`.
    pub endpoint: String,
    /// The bucket.
    pub bucket: String,
    /// The region.
    #[serde(default)]
    pub region: String,
    /// A folder in the bucket.
    #[serde(default)]
    pub prefix: String,
    /// The access key ID; empty with the secret to keep the keys the destination has.
    #[serde(default)]
    pub access_key_id: String,
    /// The secret access key.
    #[serde(default)]
    pub secret_access_key: String,
}

/// What to plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum S3DestinationsPlanRequest {
    /// Add a destination, or change one.
    Save(S3DestinationFields),
    /// Remove a destination. Secrets its keys were copied into stay.
    Remove {
        /// The destination.
        name: String,
    },
}

/// What a write does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum S3DestinationWriteAction {
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
pub struct S3DestinationWrite {
    /// What it does.
    pub action: S3DestinationWriteAction,
    /// The object's kind.
    pub kind: String,
    /// Its name.
    pub name: String,
    /// The object as it is, YAML; absent for a creation and for a Secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_yaml: Option<String>,
    /// The object as it will be, YAML, a Secret's values hidden; absent for a deletion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_yaml: Option<String>,
}

/// A plan, and the token that applies it.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct S3DestinationsPlan {
    /// Send this to `apply` to carry out exactly this plan.
    pub token: String,
    /// When the token stops working, RFC 3339.
    pub expires_at: String,
    /// The writes, in order; empty when nothing changes.
    pub writes: Vec<S3DestinationWrite>,
}

/// The token of a plan to apply.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct S3DestinationsApply {
    /// The token `plan` returned.
    pub token: String,
}

/// What an apply wrote.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct S3DestinationsApplied {
    /// The objects written, `Kind/name`.
    pub written: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Claims {
    person: String,
    groups: Vec<String>,
    request: S3DestinationsPlanRequest,
    versions: BTreeMap<String, String>,
    expires_at: i64,
}

fn resource(kind: &str) -> ResourceRef {
    match kind {
        "Secret" => ResourceRef::new("", "v1", "secrets", "Secret"),
        "Namespace" => ResourceRef::new("", "v1", "namespaces", "Namespace"),
        _ => ResourceRef::new("", "v1", "configmaps", "ConfigMap"),
    }
}

fn invalid(detail: &str) -> ApiError {
    ApiError::from(Problem::new(Kind::Invalid, detail))
}

fn right(verb: &str, plural: &str, name: Option<&str>) -> Denied {
    Denied {
        verb: verb.to_owned(),
        group: String::new(),
        resource: plural.to_owned(),
        name: name.map(str::to_owned),
        namespace: Some(NAMESPACE.to_owned()),
    }
}

async fn permission(cluster: &dyn Cluster, verb: &str, plural: &str) -> Permission {
    match cluster.review_access(&right(verb, plural, None)).await {
        Ok(true) => Permission::Allowed,
        Ok(false) => Permission::Denied,
        Err(_) => Permission::Unknown,
    }
}

fn text(object: &DynamicObject, key: &str) -> String {
    object
        .data
        .pointer(&format!("/data/{key}"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn labelled(
    meta: &zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta,
) -> bool {
    meta.labels.as_ref().is_some_and(|l| l.contains_key(LABEL))
}

fn destination(object: &DynamicObject) -> Option<S3Destination> {
    if !labelled(&object.metadata) {
        return None;
    }
    let name = object.metadata.name.clone()?;
    Some(S3Destination {
        endpoint: text(object, "endpoint"),
        bucket: text(object, "bucket"),
        region: text(object, "region"),
        prefix: text(object, "prefix"),
        origin: Origin::complete("v1", "ConfigMap", Some(NAMESPACE), &name),
        name,
    })
}

/// The destinations, without their keys, and what the person may do with them. Readable by
/// everyone signed in where Zeabur's chart is installed.
#[utoipa::path(get, path = "/api/v1/cluster/s3-destinations", tag = "cluster",
    operation_id = "getS3Destinations",
    responses((status = 200, body = S3Destinations),
              (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(Person(cluster): Person) -> Result<Json<S3Destinations>, ApiError> {
    let cluster = cluster.as_ref();
    let mut unreadable = Vec::new();
    let mut items = match cluster
        .list_kind(&resource("ConfigMap"), Some(NAMESPACE))
        .await
    {
        Ok(objects) => objects.iter().filter_map(destination).collect(),
        Err(error) => {
            let refusal = error
                .into_unreadable("configmaps")
                .map_err(|e| ApiError::from(e.into_problem("the S3 destinations")))?;
            unreadable.push(refusal.into());
            Vec::new()
        }
    };
    items.sort_by(|a: &S3Destination, b| a.name.cmp(&b.name));
    let can_change = permission(cluster, "create", "configmaps")
        .await
        .and(permission(cluster, "create", "secrets").await);
    Ok(Json(S3Destinations {
        namespace: NAMESPACE.to_owned(),
        items,
        can_change,
        can_use_keys: permission(cluster, "get", "secrets").await,
        unreadable,
    }))
}

/// The keys of destination `name`, read as the person: the access key ID and the secret access
/// key.
///
/// # Errors
///
/// `403` naming the read when the person may not read the destination's Secret, `404` when it
/// does not exist, `422` when it holds no keys.
pub(crate) async fn keys(cluster: &dyn Cluster, name: &str) -> Result<(String, String), ApiError> {
    let values = cluster.reveal_secret(NAMESPACE, name).await.map_err(|e| {
        read_problem(
            e,
            &format!("the keys of S3 destination {name}"),
            get_read("", "secrets", Some(NAMESPACE), name),
        )
    })?;
    let key = |key: &str| {
        values
            .get(key)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    key(ACCESS_KEY_ID)
        .zip(key(SECRET_ACCESS_KEY))
        .ok_or_else(|| {
            invalid(&format!(
                "S3 destination {name} has no keys. Add them on the cluster's Backups page."
            ))
        })
}

/// The destination a Secret's keys were copied from, by its annotation.
pub(crate) fn copied_from(
    meta: &zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta,
) -> Option<String> {
    meta.annotations.as_ref()?.get(LABEL).cloned()
}

/// `request` with each `copyS3Keys` change opened into the `create` of its Secret, with the
/// destination's keys read as the person; the request as it is when it has none.
///
/// # Errors
///
/// What [`keys`] refuses, and `422` for an object that is not a Secret.
pub(crate) async fn open(
    cluster: &dyn Cluster,
    request: PlanRequest,
) -> Result<PlanRequest, ApiError> {
    let mut changes = Vec::with_capacity(request.changes.len());
    for change in request.changes {
        changes.push(match change {
            Change::CopyS3Keys(copy) => Change::Create {
                object: copied(cluster, copy).await?,
                yaml: None,
            },
            other @ (Change::Env { .. }
            | Change::Data { .. }
            | Change::AddDomain { .. }
            | Change::RenameDomain { .. }
            | Change::RemoveDomain { .. }
            | Change::Redirect(_)
            | Change::BasicAuth(_)
            | Change::ErrorPageHtml(_)
            | Change::ErrorPages(_)
            | Change::Maintenance(_)
            | Change::Certificate(_)
            | Change::EnvImport(_)
            | Change::PublicPort(_)
            | Change::Restart { .. }
            | Change::Rollback { .. }
            | Change::ExpandVolume { .. }
            | Change::LabelVolume(_)
            | Change::AddVolume { .. }
            | Change::EditMount { .. }
            | Change::RemoveVolume { .. }
            | Change::Probe { .. }
            | Change::ConfigReload { .. }
            | Change::Image { .. }
            | Change::Rebuild { .. }
            | Change::BuildOptions(_)
            | Change::AutoUpdate { .. }
            | Change::Scale { .. }
            | Change::Resources { .. }
            | Change::SecurityProfile { .. }
            | Change::PodSecurity { .. }
            | Change::Edit { .. }
            | Change::Create { .. }
            | Change::DeployKey { .. }
            | Change::Delete { .. }
            | Change::Adopt
            | Change::Unadopt
            | Change::CreateProject { .. }
            | Change::Details { .. }
            | Change::ScheduleTask { .. }
            | Change::RunTask { .. }
            | Change::DebugContainer { .. }
            | Change::ClusterTools) => other,
        });
    }
    Ok(PlanRequest { changes })
}

/// Sets the string at `path` in `object`, making the maps on the way; a field on the way that is
/// not a map is replaced.
pub(crate) fn set(object: &mut Value, path: &[&str], value: String) {
    let Some((last, parents)) = path.split_last() else {
        return;
    };
    let mut at = object;
    for key in parents {
        if !at.get(*key).is_some_and(Value::is_object)
            && let Some(map) = at.as_object_mut()
        {
            map.insert((*key).to_owned(), json!({}));
        }
        let Some(next) = at.get_mut(*key) else {
            return;
        };
        at = next;
    }
    if let Some(map) = at.as_object_mut() {
        map.insert((*last).to_owned(), Value::String(value));
    }
}

async fn copied(cluster: &dyn Cluster, copy: CopyS3KeysChange) -> Result<Value, ApiError> {
    let CopyS3KeysChange {
        destination,
        mut object,
        access_key_id,
        secret_access_key,
    } = copy;
    if object.get("kind").and_then(Value::as_str) != Some("Secret") {
        return Err(invalid("The keys of an S3 destination go into a Secret."));
    }
    let (access, secret) = keys(cluster, &destination).await?;
    set(
        &mut object,
        &["metadata", "annotations", LABEL],
        destination,
    );
    set(&mut object, &["stringData", &access_key_id], access);
    set(&mut object, &["stringData", &secret_access_key], secret);
    Ok(object)
}

fn dns_label(name: &str) -> bool {
    (1..=63).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
}

fn bucket_name(bucket: &str) -> bool {
    (3..=63).contains(&bucket.len())
        && bucket
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
        && bucket.starts_with(|c: char| c.is_ascii_alphanumeric())
        && bucket.ends_with(|c: char| c.is_ascii_alphanumeric())
}

/// What is wrong with `fields`, in words for the person.
fn fields_problem(fields: &S3DestinationFields) -> Option<&'static str> {
    let endpoint = fields.endpoint.trim();
    let host = endpoint
        .strip_prefix("https://")
        .or_else(|| endpoint.strip_prefix("http://"));
    if !dns_label(&fields.name) {
        Some(
            "A name has lower-case letters, digits and -, at most 63, and starts with a letter or digit.",
        )
    } else if host.is_none_or(|h| h.is_empty() || h.contains(char::is_whitespace)) {
        Some("The endpoint starts with https:// or http://.")
    } else if !bucket_name(fields.bucket.trim()) {
        Some("A bucket name has 3 to 63 lower-case letters, digits, dots and -.")
    } else if fields.prefix.contains("..") || fields.prefix.contains(char::is_whitespace) {
        Some("A folder has no spaces and no ..")
    } else {
        None
    }
}

fn config_map(fields: &S3DestinationFields) -> Value {
    let mut data = BTreeMap::from([
        ("endpoint", fields.endpoint.trim().trim_end_matches('/')),
        ("bucket", fields.bucket.trim()),
    ]);
    let region = fields.region.trim();
    let prefix = fields.prefix.trim().trim_matches('/');
    if !region.is_empty() {
        data.insert("region", region);
    }
    if !prefix.is_empty() {
        data.insert("prefix", prefix);
    }
    json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {"name": fields.name, "namespace": NAMESPACE,
                     "labels": {LABEL: fields.name}},
        "data": data,
    })
}

fn secret(name: &str, access: &str, secret: &str) -> Value {
    json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {"name": name, "namespace": NAMESPACE, "labels": {LABEL: name}},
        "type": "Opaque",
        "stringData": {ACCESS_KEY_ID: access, SECRET_ACCESS_KEY: secret},
    })
}

struct Planned {
    target: ObjectTarget,
    body: WriteBody,
    action: S3DestinationWriteAction,
    before: Option<Value>,
    shown: Option<Value>,
}

fn target(kind: &str, name: &str) -> ObjectTarget {
    let resource = resource(kind);
    ObjectTarget {
        api_version: "v1".to_owned(),
        kind: kind.to_owned(),
        plural: resource.resource,
        namespace: if kind == "Namespace" {
            String::new()
        } else {
            NAMESPACE.to_owned()
        },
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

fn create(kind: &str, name: &str, body: Value, shown: Value) -> Planned {
    Planned {
        target: target(kind, name),
        body: WriteBody::Create(body),
        action: S3DestinationWriteAction::Create,
        before: None,
        shown: Some(shown),
    }
}

fn patch(kind: &str, before: &Value, path: &str, value: &Value, shown: Value) -> Planned {
    let name = before
        .pointer("/metadata/name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Planned {
        target: target(kind, name),
        body: WriteBody::JsonPatch(json!([
            {"op": "test", "path": "/metadata/resourceVersion", "value": version(Some(before))},
            {"op": "add", "path": path, "value": value},
        ])),
        action: S3DestinationWriteAction::Patch,
        before: (kind != "Secret").then(|| before.clone()),
        shown: Some(shown),
    }
}

fn delete(kind: &str, before: &Value) -> Planned {
    let field = |pointer: &str| {
        before
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    Planned {
        target: target(kind, &field("/metadata/name").unwrap_or_default()),
        body: WriteBody::Delete(Preconditions {
            uid: field("/metadata/uid"),
            resource_version: field("/metadata/resourceVersion"),
            grace_period_seconds: None,
        }),
        action: S3DestinationWriteAction::Delete,
        before: (kind != "Secret").then(|| before.clone()),
        shown: None,
    }
}

/// What a plan reads: the namespace, and the destination's ConfigMap and Secret.
struct Current {
    namespace: Option<Value>,
    config_map: Option<Value>,
    secret: Option<Value>,
}

impl Current {
    async fn read(cluster: &dyn Cluster, name: &str) -> Result<Self, ApiError> {
        let find = async |kind: &str, namespace: Option<&str>, object: &str| {
            let resource = resource(kind);
            cluster
                .find_object(&resource, namespace, object)
                .await
                .map_err(|e| {
                    read_problem(
                        e,
                        &format!("{kind} {object}"),
                        get_read("", &resource.resource, namespace, object),
                    )
                })?
                .map(|o| serde_json::to_value(o).unwrap_or_default())
                .map_or(Ok(None), |v| Ok::<_, ApiError>(Some(v)))
        };
        Ok(Self {
            namespace: find("Namespace", None, NAMESPACE).await?,
            config_map: find("ConfigMap", Some(NAMESPACE), name).await?,
            secret: find("Secret", Some(NAMESPACE), name).await?,
        })
    }

    fn versions(&self) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("Namespace".to_owned(), version(self.namespace.as_ref())),
            ("ConfigMap".to_owned(), version(self.config_map.as_ref())),
            ("Secret".to_owned(), version(self.secret.as_ref())),
        ])
    }

    fn save(&self, fields: &S3DestinationFields) -> Result<Vec<Planned>, ApiError> {
        if let Some(problem) = fields_problem(fields) {
            return Err(invalid(problem));
        }
        let keys = match (fields.access_key_id.trim(), fields.secret_access_key.trim()) {
            ("", "") if self.secret.is_some() => None,
            ("", _) | (_, "") => {
                return Err(invalid(
                    "Enter the access key ID and the secret access key; leave both empty to keep \
                     the keys the destination has.",
                ));
            }
            keys => Some(keys),
        };
        let mut writes = Vec::new();
        if self.namespace.is_none() {
            let namespace = json!({"apiVersion": "v1", "kind": "Namespace",
                                   "metadata": {"name": NAMESPACE}});
            writes.push(create("Namespace", NAMESPACE, namespace.clone(), namespace));
        }
        let after = config_map(fields);
        match &self.config_map {
            None => writes.push(create("ConfigMap", &fields.name, after.clone(), after)),
            Some(before) if before.get("data") == after.get("data") => {}
            Some(before) => {
                let data = after.get("data").cloned().unwrap_or_default();
                writes.push(patch("ConfigMap", before, "/data", &data, after));
            }
        }
        if let Some((access, key)) = keys {
            let shown = secret(&fields.name, HIDDEN, HIDDEN);
            writes.push(match &self.secret {
                None => create(
                    "Secret",
                    &fields.name,
                    secret(&fields.name, access, key),
                    shown,
                ),
                Some(before) => patch(
                    "Secret",
                    before,
                    "/stringData",
                    &json!({ACCESS_KEY_ID: access, SECRET_ACCESS_KEY: key}),
                    shown,
                ),
            });
        }
        Ok(writes)
    }

    fn remove(&self, name: &str) -> Result<Vec<Planned>, ApiError> {
        let config_map = self.config_map.as_ref().ok_or_else(|| {
            ApiError::from(Problem::new(
                Kind::NotFound,
                format!("There is no S3 destination named {name}."),
            ))
        })?;
        let mut writes = vec![delete("ConfigMap", config_map)];
        writes.extend(self.secret.as_ref().map(|s| delete("Secret", s)));
        Ok(writes)
    }
}

async fn compute(
    cluster: &dyn Cluster,
    request: &S3DestinationsPlanRequest,
) -> Result<(BTreeMap<String, String>, Vec<Planned>), ApiError> {
    let name = match request {
        S3DestinationsPlanRequest::Save(fields) => &fields.name,
        S3DestinationsPlanRequest::Remove { name } => name,
    };
    let current = Current::read(cluster, name).await?;
    let writes = match request {
        S3DestinationsPlanRequest::Save(fields) => current.save(fields)?,
        S3DestinationsPlanRequest::Remove { name } => current.remove(name)?,
    };
    Ok((current.versions(), writes))
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

/// Works out what saving or removing a destination writes, and a token to apply it; writes
/// nothing. The keys never appear in the answer.
#[utoipa::path(post, path = "/api/v1/cluster/s3-destinations/plan", tag = "cluster",
    operation_id = "planS3Destinations", request_body = S3DestinationsPlanRequest,
    responses((status = 200, body = S3DestinationsPlan),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn plan(
    State(state): State<AppState>,
    Person(cluster): Person,
    Everyday(everyday): Everyday,
    Json(request): Json<S3DestinationsPlanRequest>,
) -> Result<Json<S3DestinationsPlan>, ApiError> {
    let sealing = plan_key(&state)?;
    let identity = everyday
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let (versions, writes) = compute(cluster.as_ref(), &request).await?;
    let expires_at = (state.clock)().as_second().saturating_add(LIFETIME_SECONDS);
    let claims = Claims {
        person: identity.username,
        groups: identity.groups,
        request,
        versions,
        expires_at,
    };
    let token = token::sign_as(&sealing, PREFIX, &claims).map_err(|_random| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "The system random number generator failed, so Zeabur could not seal the plan.",
        ))
    })?;
    Ok(Json(S3DestinationsPlan {
        token,
        expires_at: Timestamp::from_second(expires_at)
            .unwrap_or(Timestamp::UNIX_EPOCH)
            .to_string(),
        writes: writes
            .iter()
            .map(|w| S3DestinationWrite {
                action: w.action,
                kind: w.target.kind.clone(),
                name: w.target.name.clone(),
                before_yaml: w.before.as_ref().and_then(yaml),
                after_yaml: w.shown.as_ref().and_then(yaml),
            })
            .collect(),
    }))
}

async fn send(cluster: &dyn Cluster, write: &Planned, dry_run: bool) -> Result<(), ApiError> {
    let request = WriteRequest {
        target: write.target.clone(),
        body: write.body.clone(),
        dry_run,
    };
    let verb = match write.action {
        S3DestinationWriteAction::Create => "create",
        S3DestinationWriteAction::Patch => "patch",
        S3DestinationWriteAction::Delete => "delete",
    };
    let target = &write.target;
    let mut denied = right(
        verb,
        &target.plural,
        (write.action != S3DestinationWriteAction::Create).then_some(target.name.as_str()),
    );
    if target.kind == "Namespace" {
        denied.namespace = None;
    }
    cluster
        .write(&request)
        .await
        .map(|_| ())
        .map_err(|error| read_problem(error, &format!("{} {}", target.kind, target.name), denied))
}

/// Carries out a destinations plan by its token: checks it, dry-runs every write, then writes.
#[utoipa::path(post, path = "/api/v1/cluster/s3-destinations/apply", tag = "cluster",
    operation_id = "applyS3Destinations", request_body = S3DestinationsApply,
    responses((status = 200, body = S3DestinationsApplied),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn apply(
    State(state): State<AppState>,
    Person(cluster): Person,
    Everyday(everyday): Everyday,
    Json(request): Json<S3DestinationsApply>,
) -> Result<Json<S3DestinationsApplied>, ApiError> {
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
    let (versions, writes) = compute(cluster.as_ref(), &claims.request).await?;
    if versions != claims.versions {
        return Err(ApiError::from(Problem::new(
            Kind::Conflict,
            "The destination changed after the plan was made.",
        )));
    }
    for write in &writes {
        send(cluster.as_ref(), write, true).await?;
    }
    for write in &writes {
        send(cluster.as_ref(), write, false).await?;
    }
    Ok(Json(S3DestinationsApplied {
        written: writes
            .iter()
            .map(|w| format!("{}/{}", w.target.kind, w.target.name))
            .collect(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_buckets_follow_kubernetes_and_s3() {
        assert!(dns_label("r2-backups"));
        for bad in ["", "-a", "a-", "A", "a_b", &"a".repeat(64)] {
            assert!(!dns_label(bad), "{bad}");
        }
        assert!(bucket_name("my.bucket-1"));
        for bad in ["ab", "My", ".ab", "ab-", "a b"] {
            assert!(!bucket_name(bad), "{bad}");
        }
    }
}
