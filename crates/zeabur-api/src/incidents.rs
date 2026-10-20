//! Cluster incidents ([`zeabur_gaps::incidents`]): one cause on the cluster's side (a component
//! with no ready replica, a node that is not ready) with the services, domains and projects it
//! reaches.
//!
//! Incidents are their own read, not part of the polled project document: they need cluster-wide
//! lists (Deployments, `DaemonSets`, Nodes, `IngressClasses`) a document should not pay for every
//! few seconds. Each list the person may not read is named in `unreadable`, and the incidents it
//! would have shown are missing; an empty list with nothing unreadable means none was found.
//!
//! A project's incidents are those that reach it. Their services carry the kinds of diagnosis
//! (`symptoms`) the incident explains, so a client shows those diagnoses as the incident's and
//! not as the service's own.

use axum::Json;
use axum::extract::Path;
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_gaps::{Reach, Wide};
use zeabur_kube::k8s_openapi::api::apps::v1::{DaemonSet, Deployment};
use zeabur_kube::k8s_openapi::api::core::v1::Node;
use zeabur_kube::k8s_openapi::api::networking::v1::Ingress;
use zeabur_kube::{Cluster, ClusterError, ClusterExt as _};
use zeabur_model::Incident;

use crate::cluster::WIDE_LIMIT;
use crate::error::ApiError;
use crate::handlers::Person;
use crate::projects::ProjectRead;
use crate::views::UnreadableView;

/// The incidents found, and the reads that were refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IncidentList {
    /// The incidents, sorted by id.
    pub items: Vec<Incident>,
    /// Reads the person was refused; incidents they would have shown are missing.
    pub unreadable: Vec<UnreadableView>,
}

fn allowed<T>(
    result: Result<T, ClusterError>,
    resource: &str,
    unreadable: &mut Vec<UnreadableView>,
) -> Result<T, ApiError>
where
    T: Default,
{
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            let refusal = error
                .into_unreadable(resource)
                .map_err(|other| ApiError::from(other.into_problem(resource)))?;
            unreadable.push(refusal.into());
            Ok(T::default())
        }
    }
}

/// The components and nodes of the cluster, as far as the person may list them.
async fn causes(
    cluster: &dyn Cluster,
    unreadable: &mut Vec<UnreadableView>,
) -> Result<
    (
        Vec<zeabur_gaps::Component>,
        Vec<zeabur_gaps::NodeState>,
        Vec<zeabur_gaps::IngressClassFact>,
    ),
    ApiError,
> {
    let (deployments, daemon_sets, nodes) = futures_util::join!(
        cluster.list::<Deployment>(None),
        cluster.list::<DaemonSet>(None),
        cluster.list::<Node>(None),
    );
    let deployments = allowed(deployments, "deployments.apps", unreadable)?;
    let daemon_sets = allowed(daemon_sets, "daemonsets.apps", unreadable)?;
    let nodes = allowed(nodes, "nodes", unreadable)?;
    let classes = crate::gaps::ingress_class_facts(cluster, unreadable).await?;
    Ok((
        zeabur_gaps::components(&deployments, &daemon_sets),
        zeabur_gaps::node_states(&nodes),
        classes,
    ))
}

/// The incidents that reach the project of `read`, with their refusals added to `unreadable`.
pub(crate) async fn of_project(
    cluster: &dyn Cluster,
    read: &ProjectRead,
    unreadable: &mut Vec<UnreadableView>,
) -> Result<Vec<Incident>, ApiError> {
    let (components, nodes, classes) = causes(cluster, unreadable).await?;
    let project = &read.document.project;
    let reach = [Reach {
        project,
        snapshot: &read.snapshot,
    }];
    let mut items = zeabur_gaps::incidents(&components, &nodes, &classes, &reach, &Wide::default());
    items.retain(|incident| incident.projects.contains(&project.namespace));
    Ok(items)
}

/// What on the cluster's side is breaking this project's services: a component down or a node
/// not ready, with the services, domains and projects it reaches.
#[utoipa::path(get, path = "/api/v1/projects/{project}/incidents", tag = "projects",
    operation_id = "listProjectIncidents",
    params(("project" = String, Path, description = "The project's namespace")),
    responses((status = 200, body = IncidentList), (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn project(
    Person(cluster): Person,
    Path(project): Path<String>,
) -> Result<Json<IncidentList>, ApiError> {
    let cluster = cluster.as_ref();
    let read = crate::projects::read(cluster, &project).await?;
    let mut unreadable = read.document.unreadable.clone();
    let items = of_project(cluster, &read, &mut unreadable).await?;
    Ok(Json(IncidentList { items, unreadable }))
}

/// Every incident on the cluster: a component down or a node not ready, with the domains and
/// projects it reaches, as far as the person can list Ingresses and pods. Services are not named:
/// they are a project's, see `listProjectIncidents`.
#[utoipa::path(get, path = "/api/v1/cluster/incidents", tag = "cluster",
    operation_id = "listClusterIncidents",
    responses((status = 200, body = IncidentList), (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn cluster(Person(cluster): Person) -> Result<Json<IncidentList>, ApiError> {
    let cluster = cluster.as_ref();
    let mut unreadable = Vec::new();
    let (ingresses, pods) = futures_util::join!(
        cluster.list::<Ingress>(None),
        cluster.cluster_pods(WIDE_LIMIT),
    );
    let ingresses = allowed(ingresses, "ingresses.networking.k8s.io", &mut unreadable)?;
    let pods: Vec<(String, String)> = allowed(pods, "pods", &mut unreadable)?
        .pods
        .into_iter()
        .filter_map(|pod| Some((pod.namespace, pod.node_name?)))
        .collect();
    let (components, nodes, classes) = causes(cluster, &mut unreadable).await?;
    let wide = Wide {
        ingresses: &ingresses,
        pods: &pods,
    };
    let items = zeabur_gaps::incidents(&components, &nodes, &classes, &[], &wide);
    Ok(Json(IncidentList { items, unreadable }))
}
