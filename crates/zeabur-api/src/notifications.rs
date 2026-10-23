//! Notifications: the channels a project, or the cluster, sends alerts to, and the plans that add,
//! change, test and remove them (see [`zeabur_modules::alertmanager`]).
//!
//! | Route | What it does |
//! |---|---|
//! | `GET /api/v1/projects/{p}/notifications` | the project's channels and the events they receive, or that alerting is not installed |
//! | `POST /api/v1/projects/{p}/notifications/plan` | what adding, changing, testing or removing a channel writes, and a token; writes nothing |
//! | `POST /api/v1/projects/{p}/notifications/apply` | carries out that plan by its token |
//! | `GET /api/v1/cluster/notifications` | the same for the cluster's channels, in `observability` |
//! | `POST /api/v1/cluster/notifications/plan` | the same |
//! | `POST /api/v1/cluster/notifications/apply` | the same |
//!
//! Everything is read and written as the person; RBAC on `vmalertmanagerconfigs`, Secrets and
//! Jobs in the namespace decides. A channel is a `VMAlertmanagerConfig` and a Secret. A test is a
//! Job the alert rules see, so Zeabur never calls Alertmanager and needs no rights of its own.
//!
//! The token is sealed, so the webhook URLs and tokens a person types travel in it unreadable; it
//! binds the person, the namespace, the request and the `resourceVersion` of every object the
//! plan reads. Apply reads them again, refuses when one moved, dry-runs every write, then
//! writes. A plan's YAML never holds a secret.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use zeabur_core::problem::{Denied, Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{
    Cluster, ClusterError, ClusterExt as _, DynamicObject, ObjectTarget, Preconditions,
    ResourceRef, WriteBody, WriteRequest,
};
use zeabur_modules::alertmanager::{
    self as am, AlertmanagerChannel, AlertmanagerChannelSettings, AlertmanagerEvent,
};

use crate::error::{ApiError, get_read, list_read, read_problem};
use crate::handlers::{AppState, Everyday, Person};
use crate::plan_token::{self as token, LIFETIME_SECONDS, TokenError};
use crate::projects::backups::ProviderState;
use crate::projects::namespace::Permission;
use crate::projects::writes::token_problem;
use crate::views::UnreadableView;

/// Sealed plans of this module start with this, so no other plan's token opens as one.
const PREFIX: &str = "n1.";
/// What a plan's YAML shows for a secret.
const HIDDEN: &str = "(hidden)";

/// The channels of a project or of the cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Notifications {
    /// Whether the operator's API is served and the channels could be read.
    pub state: ProviderState,
    /// The namespace the channels are in.
    pub namespace: String,
    /// The events a channel here can receive.
    pub events: Vec<AlertmanagerEvent>,
    /// The channels, Zeabur's first, by name.
    pub channels: Vec<AlertmanagerChannel>,
    /// Whether the catalog's rule set is installed; for the cluster only, absent when it could
    /// not be read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules_installed: Option<bool>,
    /// Whether the person may add, change and remove channels.
    pub can_change: Permission,
    /// Whether the person may send a test.
    pub can_test: Permission,
    /// What could not be read.
    pub unreadable: Vec<UnreadableView>,
}

/// What to plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum NotificationsPlanRequest {
    /// Add a channel.
    Add {
        /// Its name.
        name: String,
        /// The events it receives.
        events: Vec<AlertmanagerEvent>,
        /// Where it sends, secrets included.
        settings: AlertmanagerChannelSettings,
    },
    /// Change which events a channel receives.
    Events {
        /// The channel.
        name: String,
        /// The events it receives after the change.
        events: Vec<AlertmanagerEvent>,
    },
    /// Remove a channel and its Secret.
    Remove {
        /// The channel.
        name: String,
    },
    /// Send a test to a channel.
    Test {
        /// The channel.
        name: String,
    },
}

impl NotificationsPlanRequest {
    fn name(&self) -> &str {
        match self {
            Self::Add { name, .. }
            | Self::Events { name, .. }
            | Self::Remove { name }
            | Self::Test { name } => name,
        }
    }
}

/// What a write does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum NotificationWriteAction {
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
pub struct NotificationWrite {
    /// What it does.
    pub action: NotificationWriteAction,
    /// The object's kind.
    pub kind: String,
    /// Its name.
    pub name: String,
    /// Its namespace.
    pub namespace: String,
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
pub struct NotificationsPlan {
    /// Send this to `apply` to carry out exactly this plan.
    pub token: String,
    /// When the token stops working, RFC 3339.
    pub expires_at: String,
    /// The writes, in order; empty when nothing changes.
    pub writes: Vec<NotificationWrite>,
}

/// The token of a plan to apply.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct NotificationsApply {
    /// The token `plan` returned.
    pub token: String,
}

/// What an apply wrote.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct NotificationsApplied {
    /// The objects written, `Kind/name`.
    pub written: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Claims {
    person: String,
    groups: Vec<String>,
    namespace: String,
    request: NotificationsPlanRequest,
    versions: BTreeMap<String, String>,
    job: Option<String>,
    expires_at: i64,
}

/// Whose channels: a project's or the cluster's, which receive different events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Project,
    Cluster,
}

impl Scope {
    const fn events(self) -> &'static [AlertmanagerEvent] {
        match self {
            Self::Project => &AlertmanagerEvent::PROJECT,
            Self::Cluster => &AlertmanagerEvent::CLUSTER,
        }
    }
}

fn secrets() -> ResourceRef {
    ResourceRef::new("", "v1", "secrets", "Secret")
}

fn not_installed() -> ApiError {
    ApiError::from(Problem::new(
        Kind::NotFound,
        "Alerting is not installed in the cluster, so there are no notification channels.",
    ))
}

fn invalid(detail: String) -> ApiError {
    ApiError::from(Problem::new(Kind::Invalid, detail))
}

fn conflict(detail: String) -> ApiError {
    ApiError::from(Problem::new(Kind::Conflict, detail))
}

fn right(verb: &str, resource: &str, namespace: &str, name: Option<&str>) -> Denied {
    let group = match resource {
        "secrets" => "",
        "jobs" => "batch",
        _ => am::CONFIGS.group,
    };
    Denied {
        verb: verb.to_owned(),
        group: group.to_owned(),
        resource: resource.to_owned(),
        name: name.map(str::to_owned),
        namespace: Some(namespace.to_owned()),
    }
}

async fn permission(
    cluster: &dyn Cluster,
    verb: &str,
    resource: &str,
    namespace: &str,
) -> Permission {
    match cluster
        .review_access(&right(verb, resource, namespace, None))
        .await
    {
        Ok(true) => Permission::Allowed,
        Ok(false) => Permission::Denied,
        Err(_) => Permission::Unknown,
    }
}

async fn configs(
    cluster: &dyn Cluster,
    namespace: &str,
) -> Result<Option<Vec<DynamicObject>>, ApiError> {
    cluster
        .list_objects(&am::CONFIGS.resource(), Some(namespace))
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("notification channels in {namespace}"),
                list_read(am::CONFIGS.group, am::CONFIGS.resource, Some(namespace)),
            )
        })
}

async fn answer(
    cluster: &dyn Cluster,
    namespace: &str,
    scope: Scope,
) -> Result<Notifications, ApiError> {
    let mut unreadable = Vec::new();
    let mut out = Notifications {
        state: ProviderState::Installed,
        namespace: namespace.to_owned(),
        events: scope.events().to_vec(),
        channels: Vec::new(),
        rules_installed: None,
        can_change: Permission::Unknown,
        can_test: Permission::Unknown,
        unreadable: Vec::new(),
    };
    let objects = match crate::cluster::modules::read_in(cluster, &am::CONFIGS, Some(namespace))
        .await
        .map_err(|e| ApiError::from(e.into_problem(&am::CONFIGS.resource().key())))?
    {
        zeabur_modules::Reading::Objects(objects) => objects,
        zeabur_modules::Reading::NotInstalled => {
            out.state = ProviderState::NotInstalled;
            return Ok(out);
        }
        zeabur_modules::Reading::Refused(refusal) => {
            unreadable.push(refusal);
            out.state = ProviderState::Refused;
            out.unreadable = unreadable;
            return Ok(out);
        }
    };
    let mut channels: Vec<AlertmanagerChannel> = objects.iter().map(am::channel).collect();
    channels.sort_by(|a, b| (!a.managed, &a.name).cmp(&(!b.managed, &b.name)));
    out.channels = channels;
    if scope == Scope::Cluster {
        out.rules_installed = cluster
            .find_object(&am::RULES.resource(), Some(namespace), am::RULES_NAME)
            .await
            .ok()
            .map(|rules| rules.is_some());
    }
    let configs = permission(cluster, "create", am::CONFIGS.resource, namespace).await;
    let secrets = permission(cluster, "create", "secrets", namespace).await;
    out.can_change = configs.and(secrets);
    out.can_test = permission(cluster, "create", "jobs", namespace).await;
    out.unreadable = unreadable;
    Ok(out)
}

/// The channels of a project, the events each receives and whether Alertmanager took it. Read
/// as the person; the operator's API not served is `state: not-installed`. Secrets are never
/// read.
#[utoipa::path(get, path = "/api/v1/projects/{project}/notifications", tag = "projects",
    operation_id = "getProjectNotifications",
    params(("project" = String, Path, description = "The project's namespace")),
    responses((status = 200, body = Notifications), (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn project_get(
    Person(cluster): Person,
    Path(project): Path<String>,
) -> Result<Json<Notifications>, ApiError> {
    answer(cluster.as_ref(), &project, Scope::Project)
        .await
        .map(Json)
}

/// The cluster's channels, in `observability`, and whether Zeabur's alert rules are installed.
#[utoipa::path(get, path = "/api/v1/cluster/notifications", tag = "cluster",
    operation_id = "getClusterNotifications",
    responses((status = 200, body = Notifications), (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn cluster_get(Person(cluster): Person) -> Result<Json<Notifications>, ApiError> {
    answer(cluster.as_ref(), am::CLUSTER_NAMESPACE, Scope::Cluster)
        .await
        .map(Json)
}

/// One write of a plan.
struct Planned {
    target: ObjectTarget,
    body: WriteBody,
    action: NotificationWriteAction,
    before: Option<Value>,
    shown: Option<Value>,
}

struct Computed {
    versions: BTreeMap<String, String>,
    writes: Vec<Planned>,
}

fn target(kind: &str, namespace: &str, name: &str) -> ObjectTarget {
    let (api_version, plural) = match kind {
        "Secret" => ("v1".to_owned(), "secrets"),
        "Job" => ("batch/v1".to_owned(), "jobs"),
        _ => (am::CONFIGS.resource().api_version(), am::CONFIGS.resource),
    };
    ObjectTarget {
        api_version,
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
            format!("Zeabur could not read a notification channel: {e}"),
        ))
    })
}

fn delete(kind: &str, namespace: &str, object: &Value, before: Option<Value>) -> Planned {
    let field = |pointer: &str| {
        object
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    Planned {
        target: target(
            kind,
            namespace,
            &field("/metadata/name").unwrap_or_default(),
        ),
        body: WriteBody::Delete(Preconditions {
            uid: field("/metadata/uid"),
            resource_version: field("/metadata/resourceVersion"),
            grace_period_seconds: None,
        }),
        action: NotificationWriteAction::Delete,
        before,
        shown: None,
    }
}

fn create(kind: &str, namespace: &str, name: &str, body: Value, shown: Value) -> Planned {
    Planned {
        target: target(kind, namespace, name),
        body: WriteBody::Create(body),
        action: NotificationWriteAction::Create,
        before: None,
        shown: Some(shown),
    }
}

/// The channel Zeabur made under `name`, or why it cannot be changed.
fn managed<'a>(configs: &'a [DynamicObject], name: &str) -> Result<&'a DynamicObject, ApiError> {
    let object = configs
        .iter()
        .find(|c| c.metadata.name.as_deref() == Some(am::object_name(name).as_str()))
        .ok_or_else(|| {
            ApiError::from(Problem::new(
                Kind::NotFound,
                format!("There is no channel named {name}."),
            ))
        })?;
    if am::channel(object).managed {
        Ok(object)
    } else {
        Err(invalid(format!(
            "Channel {name} was not made by Zeabur; change it where it was made."
        )))
    }
}

/// A channel to add.
struct Add<'a> {
    namespace: &'a str,
    scope: Scope,
    name: &'a str,
    events: &'a [AlertmanagerEvent],
    settings: &'a AlertmanagerChannelSettings,
}

impl Add<'_> {
    fn writes(
        &self,
        configs: &[DynamicObject],
        secret_exists: bool,
        versions: &mut BTreeMap<String, String>,
    ) -> Result<Vec<Planned>, ApiError> {
        let (namespace, name, settings) = (self.namespace, self.name, self.settings);
        if let Some(problem) = am::problem(name, self.events, self.scope.events(), Some(settings)) {
            return Err(invalid(problem));
        }
        let object_name = am::object_name(name);
        if configs
            .iter()
            .any(|c| c.metadata.name.as_deref() == Some(object_name.as_str()))
        {
            return Err(conflict(format!("A channel named {name} exists.")));
        }
        if secret_exists {
            return Err(conflict(format!(
                "Secret {object_name} exists; remove it or choose another name."
            )));
        }
        versions.insert(format!("VMAlertmanagerConfig/{object_name}"), String::new());
        let config = am::config(namespace, name, settings, self.events);
        Ok(vec![
            create(
                "Secret",
                namespace,
                &object_name,
                am::secret(namespace, name, settings, None),
                am::secret(namespace, name, settings, Some(HIDDEN)),
            ),
            create(
                am::CONFIGS.kind,
                namespace,
                &object_name,
                config.clone(),
                config,
            ),
        ])
    }
}

async fn compute(
    cluster: &dyn Cluster,
    namespace: &str,
    scope: Scope,
    request: &NotificationsPlanRequest,
    job: Option<&str>,
) -> Result<Computed, ApiError> {
    let configs = configs(cluster, namespace)
        .await?
        .ok_or_else(not_installed)?;
    let name = request.name();
    let object_name = am::object_name(name);
    let secret = cluster
        .find_object(&secrets(), Some(namespace), &object_name)
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("Secret {object_name} in {namespace}"),
                get_read("", "secrets", Some(namespace), &object_name),
            )
        })?
        .map(|s| value(&s))
        .transpose()?;
    let mut versions =
        BTreeMap::from([(format!("Secret/{object_name}"), version(secret.as_ref()))]);
    let writes = match request {
        NotificationsPlanRequest::Add {
            name,
            events,
            settings,
        } => {
            let add = Add {
                namespace,
                scope,
                name,
                events,
                settings,
            };
            add.writes(&configs, secret.is_some(), &mut versions)?
        }
        NotificationsPlanRequest::Events { name, events } => {
            if let Some(problem) = am::problem(name, events, scope.events(), None) {
                return Err(invalid(problem));
            }
            let before = value(managed(&configs, name)?)?;
            versions.insert(
                format!("VMAlertmanagerConfig/{object_name}"),
                version(Some(&before)),
            );
            let after = am::with_events(&before, name, events);
            if before.get("spec") == after.get("spec") {
                Vec::new()
            } else {
                vec![Planned {
                    target: target(am::CONFIGS.kind, namespace, &object_name),
                    body: WriteBody::JsonPatch(json!([
                        {"op": "test", "path": "/metadata/resourceVersion",
                         "value": version(Some(&before))},
                        {"op": "add", "path": "/spec/route/matchers",
                         "value": after.pointer("/spec/route/matchers")},
                    ])),
                    action: NotificationWriteAction::Patch,
                    before: Some(before),
                    shown: Some(after),
                }]
            }
        }
        NotificationsPlanRequest::Remove { name } => {
            let before = value(managed(&configs, name)?)?;
            versions.insert(
                format!("VMAlertmanagerConfig/{object_name}"),
                version(Some(&before)),
            );
            let mut writes = vec![delete(
                am::CONFIGS.kind,
                namespace,
                &before,
                Some(before.clone()),
            )];
            writes.extend(
                secret
                    .as_ref()
                    .map(|s| delete("Secret", namespace, s, None)),
            );
            writes
        }
        NotificationsPlanRequest::Test { name } => {
            let before = value(managed(&configs, name)?)?;
            versions.insert(
                format!("VMAlertmanagerConfig/{object_name}"),
                version(Some(&before)),
            );
            let job = job.unwrap_or_default();
            let made = am::test_job(namespace, name, job);
            vec![create("Job", namespace, job, made.clone(), made)]
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

async fn plan_in(
    state: &AppState,
    cluster: &dyn Cluster,
    everyday: &dyn Cluster,
    namespace: &str,
    scope: Scope,
    request: NotificationsPlanRequest,
) -> Result<NotificationsPlan, ApiError> {
    let sealing = plan_key(state)?;
    let identity = everyday
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let now = (state.clock)().as_second();
    let job = matches!(request, NotificationsPlanRequest::Test { .. })
        .then(|| am::test_job_name(request.name(), now));
    let computed = compute(cluster, namespace, scope, &request, job.as_deref()).await?;
    let expires_at = now.saturating_add(LIFETIME_SECONDS);
    let claims = Claims {
        person: identity.username,
        groups: identity.groups,
        namespace: namespace.to_owned(),
        request,
        versions: computed.versions,
        job,
        expires_at,
    };
    let token = token::sign_as(&sealing, PREFIX, &claims).map_err(|_random| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "The system random number generator failed, so Zeabur could not seal the plan.",
        ))
    })?;
    Ok(NotificationsPlan {
        token,
        expires_at: Timestamp::from_second(expires_at)
            .unwrap_or(Timestamp::UNIX_EPOCH)
            .to_string(),
        writes: computed
            .writes
            .iter()
            .map(|w| NotificationWrite {
                action: w.action,
                kind: w.target.kind.clone(),
                name: w.target.name.clone(),
                namespace: w.target.namespace.clone(),
                before_yaml: w.before.as_ref().and_then(yaml),
                after_yaml: w.shown.as_ref().and_then(yaml),
            })
            .collect(),
    })
}

async fn send(cluster: &dyn Cluster, write: &Planned, dry_run: bool) -> Result<(), ApiError> {
    let request = WriteRequest {
        target: write.target.clone(),
        body: write.body.clone(),
        dry_run,
    };
    let verb = match write.action {
        NotificationWriteAction::Create => "create",
        NotificationWriteAction::Patch => "patch",
        NotificationWriteAction::Delete => "delete",
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
                    (write.action != NotificationWriteAction::Create)
                        .then_some(target.name.as_str()),
                ),
            )
        })
}

async fn apply_in(
    state: &AppState,
    cluster: &dyn Cluster,
    everyday: &dyn Cluster,
    namespace: &str,
    scope: Scope,
    token: &str,
) -> Result<NotificationsApplied, ApiError> {
    let sealing = plan_key(state)?;
    let claims: Claims = token::open_as(&sealing, PREFIX, token).map_err(token_problem)?;
    if (state.clock)().as_second() >= claims.expires_at {
        return Err(token_problem(TokenError::Expired));
    }
    if claims.namespace != namespace {
        return Err(ApiError::from(Problem::new(
            Kind::BadRequest,
            "This plan was made for another project.",
        )));
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
    let computed = compute(
        cluster,
        namespace,
        scope,
        &claims.request,
        claims.job.as_deref(),
    )
    .await?;
    if computed.versions != claims.versions {
        return Err(conflict(
            "The channel or its Secret changed after the plan was made.".to_owned(),
        ));
    }
    for write in &computed.writes {
        send(cluster, write, true).await?;
    }
    for write in &computed.writes {
        send(cluster, write, false).await?;
    }
    Ok(NotificationsApplied {
        written: computed
            .writes
            .iter()
            .map(|w| format!("{}/{}", w.target.kind, w.target.name))
            .collect(),
    })
}

/// Works out what adding, changing, testing or removing one of a project's channels writes, and
/// a token to apply it; writes nothing. Secrets never appear in the answer.
#[utoipa::path(post, path = "/api/v1/projects/{project}/notifications/plan", tag = "projects",
    operation_id = "planProjectNotifications", request_body = NotificationsPlanRequest,
    params(("project" = String, Path, description = "The project's namespace")),
    responses((status = 200, body = NotificationsPlan),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn project_plan(
    State(state): State<AppState>,
    Person(cluster): Person,
    Everyday(everyday): Everyday,
    Path(project): Path<String>,
    Json(request): Json<NotificationsPlanRequest>,
) -> Result<Json<NotificationsPlan>, ApiError> {
    plan_in(
        &state,
        cluster.as_ref(),
        everyday.as_ref(),
        &project,
        Scope::Project,
        request,
    )
    .await
    .map(Json)
}

/// Carries out a project's notifications plan by its token: checks it, dry-runs every write,
/// then writes.
#[utoipa::path(post, path = "/api/v1/projects/{project}/notifications/apply", tag = "projects",
    operation_id = "applyProjectNotifications", request_body = NotificationsApply,
    params(("project" = String, Path, description = "The project's namespace")),
    responses((status = 200, body = NotificationsApplied),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn project_apply(
    State(state): State<AppState>,
    Person(cluster): Person,
    Everyday(everyday): Everyday,
    Path(project): Path<String>,
    Json(request): Json<NotificationsApply>,
) -> Result<Json<NotificationsApplied>, ApiError> {
    apply_in(
        &state,
        cluster.as_ref(),
        everyday.as_ref(),
        &project,
        Scope::Project,
        &request.token,
    )
    .await
    .map(Json)
}

/// Works out what adding, changing, testing or removing one of the cluster's channels writes,
/// and a token to apply it; writes nothing. Secrets never appear in the answer.
#[utoipa::path(post, path = "/api/v1/cluster/notifications/plan", tag = "cluster",
    operation_id = "planClusterNotifications", request_body = NotificationsPlanRequest,
    responses((status = 200, body = NotificationsPlan),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn cluster_plan(
    State(state): State<AppState>,
    Person(cluster): Person,
    Everyday(everyday): Everyday,
    Json(request): Json<NotificationsPlanRequest>,
) -> Result<Json<NotificationsPlan>, ApiError> {
    plan_in(
        &state,
        cluster.as_ref(),
        everyday.as_ref(),
        am::CLUSTER_NAMESPACE,
        Scope::Cluster,
        request,
    )
    .await
    .map(Json)
}

/// Carries out the cluster's notifications plan by its token.
#[utoipa::path(post, path = "/api/v1/cluster/notifications/apply", tag = "cluster",
    operation_id = "applyClusterNotifications", request_body = NotificationsApply,
    responses((status = 200, body = NotificationsApplied),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn cluster_apply(
    State(state): State<AppState>,
    Person(cluster): Person,
    Everyday(everyday): Everyday,
    Json(request): Json<NotificationsApply>,
) -> Result<Json<NotificationsApplied>, ApiError> {
    apply_in(
        &state,
        cluster.as_ref(),
        everyday.as_ref(),
        am::CLUSTER_NAMESPACE,
        Scope::Cluster,
        &request.token,
    )
    .await
    .map(Json)
}
