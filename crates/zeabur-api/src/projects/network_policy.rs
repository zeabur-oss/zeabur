//! The network-policy module in the API: each project document's reachability extension, and
//! the ready-made policies for one service.
//!
//! The project's `NetworkPolicy` objects are read as the person, in the project's namespace,
//! with the rest of the document; the module evaluates them (see
//! `zeabur_modules::network_policy`). A refused read joins the document's `unreadable`; it never
//! makes the document fail. Whether the cluster's network plugin enforces the policies is not
//! known; clients say it depends on the plugin.

use axum::Json;
use axum::extract::{Path, RawQuery};
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_graph::{Snapshot, workloads};
use zeabur_kube::{Cluster, DynamicObject};
use zeabur_model::{HttpRoute, Project, WorkloadKind, id};
use zeabur_modules::Reading;
use zeabur_modules::network_policy::{self, Preset, Reachability, Workload};

use crate::error::{ApiError, project_read, read_problem};
use crate::handlers::Person;
use crate::views::UnreadableView;

/// The project's services as reachability sees them: their ids and pod template labels.
pub(crate) fn services(snapshot: &Snapshot) -> Vec<Workload> {
    workloads(snapshot)
        .into_iter()
        .map(|w| Workload {
            id: id::service(&snapshot.namespace, w.kind(), w.name()),
            labels: w
                .pod_template()
                .and_then(|t| t.metadata.as_ref())
                .and_then(|m| m.labels.clone())
                .unwrap_or_default(),
        })
        .collect()
}

/// What reachability is computed from, as read for one project.
pub(crate) struct Facts {
    pub(crate) namespace: String,
    pub(crate) policies: Vec<DynamicObject>,
    pub(crate) workloads: Vec<Workload>,
    pub(crate) connections: Vec<(String, String)>,
}

impl Facts {
    pub(crate) fn input(&self) -> network_policy::Input<'_> {
        network_policy::Input {
            namespace: &self.namespace,
            workloads: &self.workloads,
            policies: &self.policies,
            connections: &self.connections,
        }
    }
}

/// The reachability extension of a project document; `None` when the API is not served. A
/// refused read adds to `unreadable`.
///
/// # Errors
///
/// When kube-apiserver cannot be reached.
pub(crate) async fn extension(
    cluster: &dyn Cluster,
    snapshot: &Snapshot,
    project: &Project,
    unreadable: &mut Vec<UnreadableView>,
) -> Result<Option<Reachability>, ApiError> {
    Ok(read(cluster, snapshot, project, unreadable)
        .await?
        .map(|facts| network_policy::reachability(facts.input())))
}

/// The project's NetworkPolicy objects, services and connections; `None` when the API is not
/// served. A refused read adds to `unreadable`.
///
/// # Errors
///
/// When kube-apiserver cannot be reached.
pub(crate) async fn read(
    cluster: &dyn Cluster,
    snapshot: &Snapshot,
    project: &Project,
    unreadable: &mut Vec<UnreadableView>,
) -> Result<Option<Facts>, ApiError> {
    let reading = crate::cluster::modules::read_in(
        cluster,
        &network_policy::NETWORK_POLICIES,
        Some(&snapshot.namespace),
    )
    .await
    .map_err(|e| ApiError::from(e.into_problem("network policies")))?;
    let objects = match reading {
        Reading::Objects(objects) => objects,
        Reading::NotInstalled => return Ok(None),
        Reading::Refused(refusal) => {
            unreadable.push(refusal);
            return Ok(None);
        }
    };
    let workloads = services(snapshot);
    let connections: Vec<(String, String)> = project
        .connections
        .iter()
        .map(|c| {
            (
                id::service(&snapshot.namespace, c.from.kind, &c.from.name),
                id::service(&snapshot.namespace, c.to.kind, &c.to.name),
            )
        })
        .collect();
    Ok(Some(Facts {
        namespace: snapshot.namespace.clone(),
        policies: objects,
        workloads,
        connections,
    }))
}

/// Services traffic reaches from outside the project: those an Ingress routes to (from the
/// projection) or an HTTPRoute names a Kubernetes Service of in `backendRefs` [gateway-backend],
/// and those behind a `NodePort` or `LoadBalancer` Service. A backend of weight 0 counts: it
/// receives nothing now, but a change to the weight alone opens it to outside traffic, so it is
/// protected as if it did.
///
/// [gateway-backend]: https://gateway-api.sigs.k8s.io/reference/spec/#httpbackendref
pub(crate) fn exposed(
    project: &Project,
    http_routes: &[serde_json::Value],
) -> (Vec<String>, Vec<String>) {
    let http_routes: Vec<HttpRoute> = http_routes
        .iter()
        .filter_map(|route| serde_json::from_value(route.clone()).ok())
        .collect();
    let backends: Vec<&str> = http_routes
        .iter()
        .flat_map(HttpRoute::named_local_services)
        .collect();
    let routed = project
        .services
        .iter()
        .filter(|s| {
            !s.route_names.is_empty()
                || s.endpoint_names
                    .iter()
                    .any(|e| backends.contains(&e.as_str()))
        })
        .map(|s| s.id.clone())
        .collect();
    let published = project
        .services
        .iter()
        .filter(|s| {
            project
                .endpoints_of(s)
                .any(|e| matches!(e.type_.as_deref(), Some("NodePort" | "LoadBalancer")))
        })
        .map(|s| s.id.clone())
        .collect();
    (routed, published)
}

/// The query of [`presets`] (for the description; parsed by hand like the other queries).
#[expect(
    dead_code,
    reason = "describes the parameter in the OpenAPI document; parsed by hand"
)]
#[derive(Debug, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct PresetQuery {
    /// The service, `<kind>/<name>` (`deployment/api`; any case).
    service: String,
}

/// The ready-made policies for one service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PresetList {
    /// Isolate first, then allow the ingress controller, then one per other service.
    pub items: Vec<Preset>,
}

/// Ready-made NetworkPolicy objects for one service, to plan as new objects (`create`): isolate
/// it (its project and cluster DNS stay reachable), let the ingress controller (Traefik)
/// reach it, or let one other service reach it. Nothing is written; whether the cluster enforces
/// NetworkPolicy depends on its network plugin.
#[utoipa::path(get, path = "/api/v1/projects/{project}/network-policy/presets", tag = "projects",
    operation_id = "listNetworkPolicyPresets",
    params(("project" = String, Path), PresetQuery),
    responses((status = 200, body = PresetList), (status = 404, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn presets(
    Person(cluster): Person,
    Path(project): Path<String>,
    RawQuery(query): RawQuery,
) -> Result<Json<PresetList>, ApiError> {
    let wanted = form_urlencoded::parse(query.unwrap_or_default().as_bytes())
        .find(|(name, _)| name == "service")
        .map_or_else(String::new, |(_, value)| value.into_owned());
    let objects = cluster
        .namespace_objects(&project)
        .await
        .map_err(|e| read_problem(e, &format!("project {project}"), project_read(&project)))?;
    let (snapshot, _) = crate::projects::snapshot(objects);
    let (kind, name) = wanted.split_once('/').unwrap_or_default();
    let target = WorkloadKind::parse(kind).map(|k| id::service(&project, k, name));
    let all = services(&snapshot);
    let Some(service) = target.and_then(|t| all.iter().find(|w| w.id == t)) else {
        return Err(ApiError::from(Problem::new(
            Kind::NotFound,
            format!("Project {project} has no service {wanted:?}, or you cannot see it."),
        )));
    };
    Ok(Json(PresetList {
        items: network_policy::presets(&project, service, &all),
    }))
}
