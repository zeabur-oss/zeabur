//! The write endpoints of layer L1: plan, then apply.
//!
//! # Plan, then apply
//!
//! `plan` reads the project as the person, computes the writes ([`zeabur_plan::plan`]) and
//! returns them with a token. Nothing is written. `apply` takes only the token: it opens it (the
//! token is encrypted and authenticated), checks the expiry, that the person is the one who
//! planned, and that every object it writes still has the `resourceVersion` it was planned
//! against; then it computes the same writes again,
//! sends each as a dry run with strict field validation, and only when all pass, sends them for
//! real. Each write repeats the `resourceVersion` precondition, so an object changed between the
//! check and the write fails with `409` instead of being overwritten, and of many applies of one
//! plan exactly one succeeds.
//!
//! A deletion is planned and applied the same way: its write is a `DELETE` with preconditions on
//! the object's `uid` and `resourceVersion` ([`zeabur_kube::Preconditions`]), dry-run first like
//! every write. Its plan also says what goes with the object, and, for a NetworkPolicy, which
//! services and connections it no longer holds back
//! (`zeabur_modules::network_policy::opened_without`).
//!
//! A plan that writes several objects is not one transaction: kube-apiserver has none across
//! objects. The dry run catches what validation and admission would refuse; a write that fails
//! after earlier ones succeeded answers with a problem naming the objects already written.
//!
//! # Elevation
//!
//! Plan and apply read and write with the connection every request uses, the administrator's
//! while the person holds one ([`crate::access::ClusterAccess::for_request`]). Only the token's
//! binding uses the everyday identity ([`crate::access::ClusterAccess::everyday`]), so elevating
//! or the rights ending between plan and apply does not void a plan. A write the cluster refuses
//! where elevation is offered answers `elevation-required`, as every refusal from the cluster
//! does (the `error` module).

pub(crate) mod admission;
pub(crate) mod certificates;
mod creations;
mod deletions;
mod edits;
mod namespace;
pub(crate) mod preview;
mod refusals;
pub(crate) mod releases;
mod secrets;
pub(crate) mod terminal;
mod volumes;

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use creations::{refuse_existing, served_kinds};
use deletions::{
    deleted, network_policies_opened, public_ports_blocked, reclaim_policies,
    replica_sets_of_deleted,
};
use edits::edited_objects;
use namespace::compute_namespace;
use refusals::refused;
use releases::releases_to_roll_back;
use secrets::secret_values;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use volumes::storage_class;
use zeabur_core::problem::{AppliedObject, Kind, MEDIA_TYPE as PROBLEM, Problem, ProblemAt};
use zeabur_kube::k8s_openapi::api::storage::v1::StorageClass;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{Cluster, ClusterExt as _, ObjectTarget, Preconditions, WriteBody, WriteRequest};
use zeabur_plan::{Change, ErrorKind, GatewayRef, Input, Plan, PlanError, PlanRequest, Write};

use crate::cluster::s3_destinations;
use crate::error::{ApiError, project_read, read_problem};
use crate::handlers::{AppState, Credential, Everyday, Person};
use crate::plan_token::{self as token, Claims, LIFETIME_SECONDS, PlanKey, TokenError};
use crate::projects::{deploy_keys, snapshot};

/// A plan and the token that applies it.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlanResponse {
    /// Send this to `apply` to carry out exactly this plan.
    pub token: String,
    /// When the token stops working, RFC 3339.
    pub expires_at: String,
    /// The plan.
    #[serde(flatten)]
    pub plan: Plan,
    /// What the plan does once applied: the pods it replaces, whether new ones fit, the data
    /// and fields it removes.
    pub preview: preview::Preview,
    /// What the workloads it creates miss: resource requests and limits, health checks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gaps: Vec<zeabur_model::Gap>,
}

/// The token of a plan to apply.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ApplyRequest {
    /// The token `plan` returned.
    pub token: String,
}

/// One object an apply wrote.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    /// The object.
    pub target: zeabur_plan::Target,
    /// Its `resourceVersion` after the write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_version: Option<String>,
}

/// What an apply wrote.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ApplyResponse {
    /// The objects, in the order written.
    pub applied: Vec<Applied>,
}

pub(crate) fn plan_problem(error: PlanError) -> ApiError {
    let kind = match error.kind {
        _ if error.managed_by.is_some() => Kind::Managed,
        ErrorKind::BadRequest => Kind::BadRequest,
        ErrorKind::NotFound => Kind::NotFound,
        ErrorKind::Conflict => Kind::Conflict,
        ErrorKind::Invalid => Kind::Invalid,
        ErrorKind::SystemNamespace => Kind::SystemNamespace,
    };
    let at = error.change.map(|change| ProblemAt {
        change,
        line: error.line,
        field: error.field,
    });
    ApiError::from(Problem {
        managed_by: error.managed_by,
        at,
        ..Problem::new(kind, error.detail)
    })
}

fn key(state: &AppState) -> Result<Arc<PlanKey>, ApiError> {
    state.plan_key.clone().ok_or_else(|| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "Zeabur could not make a token key when it started, so it cannot plan changes.",
        ))
    })
}

/// Reads what a plan needs and computes it.
async fn compute(
    cluster: &dyn Cluster,
    project: &str,
    request: &PlanRequest,
    now: &str,
    cluster_issuer: Option<&str>,
) -> Result<Plan, ApiError> {
    if request.changes.iter().any(Change::is_namespace_change) {
        return compute_namespace(cluster, project, request, now).await;
    }
    let objects = cluster
        .namespace_objects(project)
        .await
        .map_err(|e| read_problem(e, &format!("project {project}"), project_read(project)))?;
    let routes = objects.http_routes.clone();
    let (snapshot, _) = snapshot(objects);
    let wants_gateways = request
        .changes
        .iter()
        .any(|c| matches!(c, Change::AddDomain(_)));
    let gateways: Option<Vec<GatewayRef>> = if wants_gateways {
        cluster
            .gateways()
            .await
            .map_err(|e| ApiError::from(e.into_problem("the Gateways")))?
    } else {
        Some(Vec::new())
    };
    let kinds = served_kinds(cluster, request).await?;
    refuse_existing(cluster, project, request, &kinds).await?;
    let secret_values = secret_values(cluster, project, request).await?;
    let releases = releases_to_roll_back(cluster, project, request, &snapshot).await?;
    let storage_classes = if request
        .changes
        .iter()
        .any(|c| matches!(c, Change::ExpandVolume { .. } | Change::AddVolume { .. }))
    {
        match cluster.list::<StorageClass>(None).await {
            Ok(list) => Some(list.iter().map(storage_class).collect::<Vec<_>>()),
            Err(error) if error.is_refused() => None,
            Err(error) => return Err(ApiError::from(error.into_problem("the StorageClasses"))),
        }
    } else {
        Some(Vec::new())
    };
    let certificates = certificates::checked(request, now)?;
    let issuers = certificates::issuers(cluster, request, &snapshot, cluster_issuer).await;
    let mut objects = edited_objects(cluster, project, request).await?;
    objects.extend(certificates::issued(cluster, project, request, &snapshot).await);
    let namespace = namespace::pod_security_namespace(cluster, project, request).await?;
    let replica_sets = replica_sets_of_deleted(cluster, project, request).await?;
    let reclaim_policies = reclaim_policies(cluster, request, &snapshot, &routes, &objects).await?;
    let changes_data = request.changes.iter().any(|c| {
        matches!(c, Change::Data { .. })
            || matches!(c, Change::Edit { resource, .. } if resource.group.is_empty()
                && matches!(resource.resource.as_str(), "configmaps" | "secrets"))
    });
    let reloader = if changes_data {
        crate::cluster::config_reload::read(cluster).await.0.runs()
    } else {
        None
    };
    let mut plan = zeabur_plan::plan(
        Input {
            snapshot: &snapshot,
            http_routes: &routes,
            gateways: gateways.as_deref(),
            secret_values: &secret_values,
            now,
            releases: &releases,
            storage_classes: storage_classes.as_deref(),
            objects: &objects,
            replica_sets: &replica_sets,
            reclaim_policies: &reclaim_policies,
            namespace: namespace.as_ref(),
            reloader,
            kinds: &kinds,
            cluster_issuer,
            issuers: &issuers,
            certificates: &certificates,
        },
        request,
    )
    .map_err(plan_problem)?;
    let policies: Vec<&str> = deleted(request)
        .filter(|(resource, _)| {
            resource.group == "networking.k8s.io" && resource.resource == "networkpolicies"
        })
        .map(|(_, name)| name)
        .collect();
    plan.warnings
        .extend(network_policies_opened(cluster, &snapshot, &policies).await?);
    plan.warnings
        .extend(public_ports_blocked(cluster, &snapshot, request).await?);
    Ok(plan)
}

/// [`compute`], or [`terminal::compute`] for a terminal's change.
async fn compute_any(
    planner: &terminal::Planner<'_>,
    cluster: &dyn Cluster,
    project: &str,
    request: &PlanRequest,
    now: &str,
    cluster_issuer: Option<&str>,
) -> Result<Plan, ApiError> {
    if request.changes.iter().any(Change::is_terminal_change) {
        terminal::compute(planner, cluster, project, request, now).await
    } else {
        compute(cluster, project, request, now, cluster_issuer).await
    }
}

/// Computes the writes a change needs and returns them with a token; writes nothing.
#[utoipa::path(post, path = "/api/v1/projects/{project}/plan", tag = "projects",
    params(("project" = String, Path, description = "The project's namespace")),
    request_body = PlanRequest,
    responses((status = 200, body = PlanResponse),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn plan(
    State(state): State<AppState>,
    Person(cluster): Person,
    Everyday(everyday): Everyday,
    Credential(handoff): Credential,
    Path(project): Path<String>,
    Json(request): Json<PlanRequest>,
) -> Result<Json<PlanResponse>, ApiError> {
    let key = key(&state)?;
    let identity = everyday
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let now = Timestamp::from_second((state.clock)().as_second()).unwrap_or(Timestamp::UNIX_EPOCH);
    let made_at = now.to_string();
    let planner = terminal::Planner {
        settings: &state.settings.terminal,
        person: &identity.username,
        handoff: handoff.as_ref(),
    };
    let opened = deploy_keys::open(&state, &identity.username, &project, &request)?;
    let opened = s3_destinations::open(cluster.as_ref(), opened).await?;
    let plan = compute_any(
        &planner,
        cluster.as_ref(),
        &project,
        &opened,
        &made_at,
        state.settings.domains.cluster_issuer.as_deref(),
    )
    .await?;
    let from_yaml = creations::from_yaml(&opened);
    if from_yaml {
        creations::dry_run(cluster.as_ref(), &project, &plan).await?;
    }
    let preview = preview::read(cluster.as_ref(), &project, &plan, &opened).await?;
    let gaps = if from_yaml {
        creations::gaps(&project, &plan)
    } else {
        Vec::new()
    };
    let expires_at = now.as_second().saturating_add(LIFETIME_SECONDS);
    let claims = Claims {
        person: identity.username,
        groups: identity.groups,
        project,
        request,
        versions: zeabur_plan::versions(&plan),
        made_at,
        expires_at,
    };
    let token = token::sign(&key, &claims).map_err(|_random| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "The system random number generator failed, so Zeabur could not seal the plan.",
        ))
    })?;
    Ok(Json(PlanResponse {
        token,
        expires_at: Timestamp::from_second(expires_at)
            .unwrap_or(Timestamp::UNIX_EPOCH)
            .to_string(),
        plan: plan.without_secret_values(),
        preview,
        gaps,
    }))
}

/// The problem for a plan token that does not open.
pub(crate) fn token_problem(error: TokenError) -> ApiError {
    let kind = match error {
        TokenError::Expired => Kind::PlanExpired,
        TokenError::Altered | TokenError::Malformed => Kind::BadRequest,
    };
    let detail = match error {
        TokenError::Expired => "The plan expired.",
        TokenError::Altered => "The plan token was altered, or it is not from this server.",
        TokenError::Malformed => "The plan token is malformed.",
    };
    let mut problem = Problem::new(kind, detail);
    let (fix, key) = match error {
        TokenError::Expired => (
            "Plans last ten minutes. Make the plan again.",
            zeabur_core::message::Message::new("problem.planExpired.fix.again")
                .with("minutes", (LIFETIME_SECONDS / 60).to_string()),
        ),
        TokenError::Altered | TokenError::Malformed => (
            "Make the plan again; a plan applies only with the token it came with, \
             unchanged, on the server that made it.",
            zeabur_core::message::Message::new("problem.badRequest.fix.planToken"),
        ),
    };
    problem.fix_for_occasion(fix, key);
    ApiError::from(problem)
}

/// Carries out a plan by its token: checks it, dry-runs every write, then writes.
///
/// Reads and writes use the elevated credential while the person holds one, as every request
/// does. The plan's binding alone is checked against the everyday identity: elevation adds groups
/// but keeps the user name, so a plan made before elevating still applies after, and one made
/// while elevated still applies once the rights have ended (and is refused then if the everyday
/// credential may not write it). Where elevation is offered and not in use, a `403` from the
/// cluster is answered as `elevation-required` (same status, same `denied`; see the `error`
/// module), so the interface can offer to retry as an administrator.
#[utoipa::path(post, path = "/api/v1/projects/{project}/apply", tag = "projects",
    params(("project" = String, Path, description = "The project's namespace")),
    request_body = ApplyRequest,
    responses((status = 200, body = ApplyResponse),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn apply(
    State(state): State<AppState>,
    Person(cluster): Person,
    Everyday(everyday): Everyday,
    Credential(handoff): Credential,
    Path(project): Path<String>,
    Json(request): Json<ApplyRequest>,
) -> Result<Json<ApplyResponse>, ApiError> {
    let key = key(&state)?;
    let claims =
        token::verify(&key, &request.token, (state.clock)().as_second()).map_err(token_problem)?;
    if claims.project != project {
        return Err(ApiError::from(Problem::new(
            Kind::BadRequest,
            format!(
                "This plan is for project {}, not {project}.",
                claims.project
            ),
        )));
    }
    let identity = everyday
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let planned_by = (claims.person.as_str(), claims.groups.as_slice());
    if (identity.username.as_str(), identity.groups.as_slice()) != planned_by {
        return Err(ApiError::from(Problem::new(
            Kind::Forbidden,
            "Someone else made this plan, or you made it with other group memberships.",
        )));
    }
    let planner = terminal::Planner {
        settings: &state.settings.terminal,
        person: &identity.username,
        handoff: handoff.as_ref(),
    };
    let opened = deploy_keys::open(&state, &identity.username, &project, &claims.request)?;
    let opened = s3_destinations::open(cluster.as_ref(), opened).await?;
    let plan = compute_any(
        &planner,
        cluster.as_ref(),
        &project,
        &opened,
        &claims.made_at,
        state.settings.domains.cluster_issuer.as_deref(),
    )
    .await?;
    let namespace = terminal::namespace(&state.settings.terminal, &claims.request, &project);
    if zeabur_plan::versions(&plan) != claims.versions {
        return Err(ApiError::from(Problem::new(
            Kind::Conflict,
            "Someone changed an object in this plan after the plan was made.",
        )));
    }
    let writes: Vec<(zeabur_plan::Target, WriteRequest)> = plan
        .objects
        .iter()
        .map(|object| {
            (
                object.target.clone(),
                write_request(namespace, &object.target, &object.write),
            )
        })
        .collect();
    for (target, write) in &writes {
        let dry = WriteRequest {
            dry_run: true,
            ..write.clone()
        };
        if let Err(error) = cluster.write(&dry).await {
            let problem = refused(cluster.as_ref(), namespace, target, write, error).await;
            return Err(ApiError::from(refusal(problem, &plan, target, &[])));
        }
    }
    let mut applied = Vec::new();
    for (target, write) in writes {
        match cluster.write(&write).await {
            Ok(written) => applied.push(Applied {
                target,
                resource_version: written.resource_version,
            }),
            Err(error) => {
                let problem = refused(cluster.as_ref(), namespace, &target, &write, error).await;
                return Err(ApiError::from(refusal(problem, &plan, &target, &applied)));
            }
        }
    }
    Ok(Json(ApplyResponse { applied }))
}

/// The problem for a refused write of `target`: the document it came from, when a create made
/// it; what the apply wrote before it; no Secret's value.
fn refusal(
    mut problem: Problem,
    plan: &Plan,
    target: &zeabur_plan::Target,
    applied: &[Applied],
) -> Problem {
    problem.at = change_of(plan, target).map(|change| ProblemAt {
        change,
        line: None,
        field: problem.fields.first().map(|f| f.field.clone()),
    });
    problem.applied = applied
        .iter()
        .map(|a| AppliedObject {
            api_version: a.target.api_version.clone(),
            kind: a.target.kind.clone(),
            plural: a.target.plural.clone(),
            name: a.target.name.clone(),
            resource_version: a.resource_version.clone(),
        })
        .collect();
    let mut problem = creations::without_secret_values(problem, plan);
    if !applied.is_empty() {
        let done: Vec<String> = applied
            .iter()
            .map(|a| format!("{} {}", a.target.kind, a.target.name))
            .collect();
        problem.detail = format!("{} Already written: {}.", problem.detail, done.join(", "));
    }
    problem
}

/// The index of the `create` that makes `target`, if one does.
fn change_of(plan: &Plan, target: &zeabur_plan::Target) -> Option<usize> {
    plan.objects
        .iter()
        .find(|o| &o.target == target)
        .and_then(|o| o.change)
}

pub(crate) fn write_request(
    project: &str,
    target: &zeabur_plan::Target,
    write: &Write,
) -> WriteRequest {
    WriteRequest {
        target: ObjectTarget {
            api_version: target.api_version.clone(),
            kind: target.kind.clone(),
            plural: target.plural.clone(),
            namespace: if target.namespaced() {
                project.to_owned()
            } else {
                String::new()
            },
            name: target.name.clone(),
        },
        body: match write {
            Write::Create { body } => WriteBody::Create(body.clone()),
            Write::Patch { operations } => {
                WriteBody::JsonPatch(serde_json::Value::Array(operations.clone()))
            }
            Write::EphemeralContainers { operations } => {
                WriteBody::EphemeralContainers(serde_json::Value::Array(operations.clone()))
            }
            Write::Delete {
                uid,
                resource_version,
            } => WriteBody::Delete(Preconditions {
                uid: uid.clone(),
                resource_version: Some(resource_version.clone()),
                grace_period_seconds: None,
            }),
        },
        dry_run: false,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zeabur_plan::{Plan, PlannedObject, Target, Write};

    fn object(kind: &str, write: Write) -> PlannedObject {
        PlannedObject {
            target: Target {
                api_version: "v1".to_owned(),
                kind: kind.to_owned(),
                plural: format!("{}s", kind.to_lowercase()),
                name: "s3".to_owned(),
            },
            before: None,
            after: None,
            before_yaml: None,
            after_yaml: None,
            changes: Vec::new(),
            owners: Vec::new(),
            resource_version: None,
            write,
            change: None,
            raw_after: serde_json::Value::Null,
        }
    }

    #[test]
    fn a_plan_shows_no_secret_value_in_its_writes_and_everything_else_as_it_is() {
        let body = |kind: &str| {
            json!({"apiVersion": "v1", "kind": kind, "metadata": {"name": "s3"},
                   "stringData": {"key": "hunter2"}, "data": {"id": "aWQ="}})
        };
        let patch = Write::Patch {
            operations: vec![
                json!({"op": "test", "path": "/metadata/resourceVersion", "value": "7"}),
                json!({"op": "replace", "path": "/data/id", "value": "bmV3"}),
                json!({"op": "add", "path": "/stringData", "value": {"key": "hunter3"}}),
                json!({"op": "add", "path": "/dataSource", "value": "kept"}),
            ],
        };
        let plan = Plan {
            objects: vec![
                object(
                    "Secret",
                    Write::Create {
                        body: body("Secret"),
                    },
                ),
                object("Secret", patch.clone()),
                object(
                    "ConfigMap",
                    Write::Create {
                        body: body("ConfigMap"),
                    },
                ),
                object("ConfigMap", patch),
            ],
            impact: Vec::new(),
            warnings: Vec::new(),
        }
        .without_secret_values();
        let secret_values = ["hunter2", "hunter3", "aWQ=", "bmV3"];
        let shown = serde_json::to_string(&plan.objects[..2]).unwrap();
        for value in secret_values {
            assert!(!shown.contains(value), "{value} in {shown}");
        }
        assert!(shown.contains("\"7\"") && shown.contains("kept"), "{shown}");
        let kept = serde_json::to_string(&plan.objects[2..]).unwrap();
        for value in secret_values {
            assert!(kept.contains(value), "{value} in {kept}");
        }
    }
}
