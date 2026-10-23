//! cert-manager's `ClusterIssuers`, and the plans that set up a Let's Encrypt issuer with a
//! DNS-01 solver (see [`zeabur_modules::cert_manager`]).
//!
//! | Route | What it does |
//! |---|---|
//! | `GET /api/v1/cluster/issuers` | the `ClusterIssuers` with whether each is ready, or that cert-manager is not installed |
//! | `POST /api/v1/cluster/issuers/plan` | what creating or changing a DNS-01 issuer writes, and a token; writes nothing |
//! | `POST /api/v1/cluster/issuers/apply` | carries out that plan by its token |
//!
//! Everything is read and written as the person; RBAC on `clusterissuers` and on Secrets in
//! cert-manager's cluster resource namespace decides. The token is sealed, so the provider's
//! token travels in it unreadable; a plan's YAML never holds it.

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
use zeabur_modules::cert_manager::{self as cm, IssuerRequest, IssuerView};

use crate::error::{ApiError, get_read, read_problem};
use crate::handlers::{AppState, Everyday, Person};
use crate::plan_token::{self as token, LIFETIME_SECONDS, TokenError};
use crate::projects::backups::ProviderState;
use crate::projects::namespace::Permission;
use crate::projects::writes::token_problem;
use crate::views::UnreadableView;

/// Sealed plans of this module start with this, so no other plan's token opens as one.
const PREFIX: &str = "c1.";
/// What a plan's YAML shows for the provider's token or key.
const HIDDEN: &str = "(hidden)";

/// The cluster's `ClusterIssuers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterIssuers {
    /// Whether cert-manager is installed and its issuers could be read.
    pub state: ProviderState,
    /// The issuers, by name.
    pub issuers: Vec<IssuerView>,
    /// The issuer a new domain gets when none is chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// Whether the person may create `ClusterIssuers`.
    pub can_change: Permission,
    /// What could not be read.
    pub unreadable: Vec<UnreadableView>,
}

/// What an issuer plan writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IssuerWrite {
    /// `create` or `patch`.
    pub action: String,
    /// The object's kind.
    pub kind: String,
    /// Its name.
    pub name: String,
    /// Its namespace; empty for the `ClusterIssuer`.
    pub namespace: String,
    /// The object as it is, YAML; absent for a creation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_yaml: Option<String>,
    /// The object as it will be, YAML; a Secret's value is not in it.
    pub after_yaml: String,
}

/// An issuer plan, and the token that applies it.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IssuersPlan {
    /// Send this to `apply` to carry out exactly this plan.
    pub token: String,
    /// When the token stops working, RFC 3339.
    pub expires_at: String,
    /// The writes, in order; empty when nothing changes.
    pub writes: Vec<IssuerWrite>,
}

/// The token of a plan to apply.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct IssuersApply {
    /// The token `plan` returned.
    pub token: String,
}

/// What an apply wrote.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct IssuersApplied {
    /// The objects written, `Kind/name`.
    pub written: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Claims {
    person: String,
    groups: Vec<String>,
    namespace: String,
    request: IssuerRequest,
    versions: BTreeMap<String, String>,
    expires_at: i64,
}

/// The `ClusterIssuers`, each with its `Ready` condition and, where the form can change it, its
/// DNS provider's settings without the secret. Read as the person.
#[utoipa::path(get, path = "/api/v1/cluster/issuers", tag = "cluster",
    operation_id = "getClusterIssuers",
    responses((status = 200, body = ClusterIssuers), (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    State(state): State<AppState>,
    Person(cluster): Person,
) -> Result<Json<ClusterIssuers>, ApiError> {
    let cluster = cluster.as_ref();
    let resource = cm::CLUSTER_ISSUERS.resource();
    let read = crate::cluster::modules::read(cluster, &cm::CLUSTER_ISSUERS)
        .await
        .map_err(|e| ApiError::from(e.into_problem(&resource.key())))?;
    let (state_now, objects, unreadable) = match read {
        Reading::Objects(objects) => (ProviderState::Installed, objects, Vec::new()),
        Reading::NotInstalled => (ProviderState::NotInstalled, Vec::new(), Vec::new()),
        Reading::Refused(refusal) => (ProviderState::Refused, Vec::new(), vec![refusal]),
    };
    let mut issuers: Vec<IssuerView> = objects.iter().map(cm::issuer).collect();
    issuers.sort_by(|a, b| a.name.cmp(&b.name));
    let can_change = match cluster
        .review_access(&Denied {
            verb: "create".to_owned(),
            group: cm::CLUSTER_ISSUERS.group.to_owned(),
            resource: cm::CLUSTER_ISSUERS.resource.to_owned(),
            name: None,
            namespace: None,
        })
        .await
    {
        Ok(true) => Permission::Allowed,
        Ok(false) => Permission::Denied,
        Err(_) => Permission::Unknown,
    };
    Ok(Json(ClusterIssuers {
        state: state_now,
        issuers,
        default: state.settings.domains.cluster_issuer.clone(),
        can_change,
        unreadable,
    }))
}

fn invalid(detail: impl Into<String>) -> ApiError {
    ApiError::from(Problem::new(Kind::Invalid, detail.into()))
}

fn value(object: &DynamicObject) -> Result<Value, ApiError> {
    serde_json::to_value(object).map_err(|e| {
        ApiError::from(Problem::new(
            Kind::Internal,
            format!("Zeabur could not read a cert-manager object: {e}"),
        ))
    })
}

fn version(object: Option<&Value>) -> String {
    object
        .and_then(|o| o.pointer("/metadata/resourceVersion"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

async fn find(
    cluster: &dyn Cluster,
    resource: &ResourceRef,
    namespace: Option<&str>,
    name: &str,
) -> Result<Option<Value>, ApiError> {
    cluster
        .find_object(resource, namespace, name)
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("{} {name}", resource.kind),
                get_read(&resource.group, &resource.resource, namespace, name),
            )
        })?
        .map(|o| value(&o))
        .transpose()
}

/// cert-manager's cluster resource namespace, from its controller's arguments.
async fn resource_namespace(cluster: &dyn Cluster) -> Result<String, ApiError> {
    let deployments = ResourceRef::new("apps", "v1", "deployments", "Deployment");
    let controller = find(cluster, &deployments, Some(cm::NAMESPACE), cm::CONTROLLER).await?;
    controller
        .as_ref()
        .and_then(cm::resource_namespace)
        .ok_or_else(|| {
            invalid(format!(
                "Zeabur found no cert-manager controller at Deployment {}/{}, so it does not \
                 know the namespace where a ClusterIssuer reads its Secrets.",
                cm::NAMESPACE,
                cm::CONTROLLER
            ))
        })
}

fn check(request: &IssuerRequest) -> Result<(), ApiError> {
    if !zeabur_plan::checks::is_dns_subdomain(&request.name) {
        return Err(invalid(format!(
            "{:?} is not a valid name: use lowercase letters, digits, '-' and '.'.",
            request.name
        )));
    }
    if let Some(field) = request.dns.missing().first() {
        return Err(invalid(format!("Enter the {field} of the DNS provider.")));
    }
    let email = request.email.trim();
    if !email.is_empty() && !email.contains('@') {
        return Err(invalid(format!("{email:?} is not an email address.")));
    }
    Ok(())
}

/// One write of a plan.
struct Planned {
    target: ObjectTarget,
    body: WriteBody,
    before: Option<Value>,
    shown: Value,
}

struct Computed {
    versions: BTreeMap<String, String>,
    writes: Vec<Planned>,
}

fn target(kind: &str, namespace: &str, name: &str) -> ObjectTarget {
    let (api_version, plural) = if kind == "Secret" {
        ("v1", "secrets")
    } else {
        ("cert-manager.io/v1", "clusterissuers")
    };
    ObjectTarget {
        api_version: api_version.to_owned(),
        kind: kind.to_owned(),
        plural: plural.to_owned(),
        namespace: namespace.to_owned(),
        name: name.to_owned(),
    }
}

fn secret_write(
    namespace: &str,
    request: &IssuerRequest,
    existing: Option<&Value>,
) -> Result<Option<Planned>, ApiError> {
    let name = cm::credentials_name(&request.name);
    let secret = request.dns.secret();
    let target = target("Secret", namespace, &name);
    let shown = cm::credentials_secret(namespace, request, HIDDEN);
    match (secret.is_empty(), existing) {
        (true, None) => Err(invalid("Enter the DNS provider's token or key.")),
        (true, Some(_)) => Ok(None),
        (false, None) => Ok(Some(Planned {
            target,
            body: WriteBody::Create(cm::credentials_secret(namespace, request, secret)),
            before: None,
            shown,
        })),
        (false, Some(existing)) => Ok(Some(Planned {
            target,
            body: WriteBody::JsonPatch(json!([
                {"op": "test", "path": "/metadata/resourceVersion", "value": version(Some(existing))},
                {"op": "add", "path": "/stringData", "value": {request.dns.secret_key(): secret}},
            ])),
            before: None,
            shown,
        })),
    }
}

fn issuer_write(request: &IssuerRequest, existing: Option<Value>) -> Option<Planned> {
    let after = cm::issuer_object(request);
    let target = target("ClusterIssuer", "", &request.name);
    match existing {
        None => Some(Planned {
            target,
            body: WriteBody::Create(after.clone()),
            before: None,
            shown: after,
        }),
        Some(before) if before.get("spec") == after.get("spec") => None,
        Some(before) => Some(Planned {
            target,
            body: WriteBody::JsonPatch(json!([
                {"op": "test", "path": "/metadata/resourceVersion", "value": version(Some(&before))},
                {"op": "replace", "path": "/spec", "value": after.get("spec")},
            ])),
            before: Some(before),
            shown: after,
        }),
    }
}

async fn compute(
    cluster: &dyn Cluster,
    namespace: &str,
    request: &IssuerRequest,
) -> Result<Computed, ApiError> {
    check(request)?;
    let issuer = find(
        cluster,
        &cm::CLUSTER_ISSUERS.resource(),
        None,
        &request.name,
    )
    .await?;
    if let Some(existing) = &issuer
        && existing.pointer("/spec/acme").is_none()
    {
        return Err(invalid(format!(
            "ClusterIssuer {} is not an ACME issuer; change it with kubectl, or choose another name.",
            request.name
        )));
    }
    let secrets = ResourceRef::new("", "v1", "secrets", "Secret");
    let credentials = cm::credentials_name(&request.name);
    let secret = find(cluster, &secrets, Some(namespace), &credentials).await?;
    let versions = BTreeMap::from([
        (format!("Secret/{credentials}"), version(secret.as_ref())),
        (
            format!("ClusterIssuer/{}", request.name),
            version(issuer.as_ref()),
        ),
    ]);
    let mut writes = Vec::new();
    writes.extend(secret_write(namespace, request, secret.as_ref())?);
    writes.extend(issuer_write(request, issuer));
    Ok(Computed { versions, writes })
}

fn yaml(object: &Value) -> String {
    zeabur_graph::manifest::to_yaml(&zeabur_graph::manifest::clean(object)).unwrap_or_default()
}

fn plan_key(state: &AppState) -> Result<std::sync::Arc<token::PlanKey>, ApiError> {
    state.plan_key.clone().ok_or_else(|| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "Zeabur could not make a token key when it started, so it cannot plan changes.",
        ))
    })
}

const fn action(write: &Planned) -> &'static str {
    if matches!(write.body, WriteBody::Create(_)) {
        "create"
    } else {
        "patch"
    }
}

/// Works out what creating or changing a Let's Encrypt `ClusterIssuer` with a DNS-01 solver
/// writes, and a token to apply it; writes nothing. The provider's token never appears in the
/// answer.
#[utoipa::path(post, path = "/api/v1/cluster/issuers/plan", tag = "cluster",
    operation_id = "planClusterIssuer", request_body = IssuerRequest,
    responses((status = 200, body = IssuersPlan),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn plan(
    State(state): State<AppState>,
    Person(cluster): Person,
    Everyday(everyday): Everyday,
    Json(request): Json<IssuerRequest>,
) -> Result<Json<IssuersPlan>, ApiError> {
    let sealing = plan_key(&state)?;
    let identity = everyday
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let cluster = cluster.as_ref();
    let namespace = resource_namespace(cluster).await?;
    let computed = compute(cluster, &namespace, &request).await?;
    let expires_at = (state.clock)().as_second().saturating_add(LIFETIME_SECONDS);
    let writes = computed
        .writes
        .iter()
        .map(|w| IssuerWrite {
            action: action(w).to_owned(),
            kind: w.target.kind.clone(),
            name: w.target.name.clone(),
            namespace: w.target.namespace.clone(),
            before_yaml: w.before.as_ref().map(yaml),
            after_yaml: yaml(&w.shown),
        })
        .collect();
    let claims = Claims {
        person: identity.username,
        groups: identity.groups,
        namespace,
        request,
        versions: computed.versions,
        expires_at,
    };
    let token = token::sign_as(&sealing, PREFIX, &claims).map_err(|_random| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "The system random number generator failed, so Zeabur could not seal the plan.",
        ))
    })?;
    Ok(Json(IssuersPlan {
        token,
        expires_at: Timestamp::from_second(expires_at)
            .unwrap_or(Timestamp::UNIX_EPOCH)
            .to_string(),
        writes,
    }))
}

async fn send(cluster: &dyn Cluster, write: &Planned, dry_run: bool) -> Result<(), ApiError> {
    let request = WriteRequest {
        target: write.target.clone(),
        body: write.body.clone(),
        dry_run,
    };
    let target = &write.target;
    let namespace = (!target.namespace.is_empty()).then_some(target.namespace.clone());
    let group = if target.kind == "Secret" {
        ""
    } else {
        cm::CLUSTER_ISSUERS.group
    };
    cluster
        .write(&request)
        .await
        .map(|_| ())
        .map_err(|error: ClusterError| {
            read_problem(
                error,
                &format!("{} {}", target.kind, target.name),
                Denied {
                    verb: action(write).to_owned(),
                    group: group.to_owned(),
                    resource: target.plural.clone(),
                    name: (action(write) == "patch").then(|| target.name.clone()),
                    namespace,
                },
            )
        })
}

/// Carries out an issuer plan by its token: checks it, dry-runs every write, then writes.
#[utoipa::path(post, path = "/api/v1/cluster/issuers/apply", tag = "cluster",
    operation_id = "applyClusterIssuer", request_body = IssuersApply,
    responses((status = 200, body = IssuersApplied),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn apply(
    State(state): State<AppState>,
    Person(cluster): Person,
    Everyday(everyday): Everyday,
    Json(request): Json<IssuersApply>,
) -> Result<Json<IssuersApplied>, ApiError> {
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
    let computed = compute(cluster, &claims.namespace, &claims.request).await?;
    if computed.versions != claims.versions {
        return Err(ApiError::from(Problem::new(
            Kind::Conflict,
            "The issuer or its credentials changed after the plan was made.",
        )));
    }
    for write in &computed.writes {
        send(cluster, write, true).await?;
    }
    for write in &computed.writes {
        send(cluster, write, false).await?;
    }
    Ok(Json(IssuersApplied {
        written: computed
            .writes
            .iter()
            .map(|w| format!("{}/{}", w.target.kind, w.target.name))
            .collect(),
    }))
}
