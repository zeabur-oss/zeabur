//! The Gateway API `Gateway`s a new `HTTPRoute` of a project can attach to: the listeners whose
//! `allowedRoutes` accept an `HTTPRoute` from the project's namespace
//! ([`zeabur_plan::Attachable`]).
//!
//! Gateways are the cluster's, so the person needs `list` on `gateways` in every namespace. A
//! project member usually has not, and then "Add domain" offers only an Ingress.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::Path;
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::{Cluster, ResourceRef};
use zeabur_plan::Attachable;

use crate::error::ApiError;
use crate::handlers::Person;

/// A Gateway that accepts an `HTTPRoute` from the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GatewayView {
    /// The Gateway's namespace.
    pub namespace: String,
    /// The Gateway's name.
    pub name: String,
    /// The listeners that accept the route.
    pub listeners: Vec<ListenerView>,
}

/// A listener of a Gateway.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListenerView {
    /// The listener's name, a parent's `sectionName`.
    pub name: String,
    /// The hostname it serves; absent when it serves every hostname.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
}

/// The Gateways a project's new `HTTPRoute` can attach to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGateways {
    /// Whether you may list the cluster's Gateways. When not, `gateways` is empty.
    pub readable: bool,
    /// The Gateways, by namespace and name.
    pub gateways: Vec<GatewayView>,
}

/// The Gateways that accept an `HTTPRoute` from `project`: empty when Gateway API is not installed,
/// `None` when the person may not list them.
pub(crate) async fn attachable<C: Cluster + ?Sized>(
    cluster: &C,
    project: &str,
) -> Result<Option<Vec<Attachable>>, ApiError> {
    let resource = ResourceRef::new("gateway.networking.k8s.io", "v1", "gateways", "Gateway");
    let listed = match cluster.list_objects(&resource, None).await {
        Ok(listed) => listed.unwrap_or_default(),
        Err(error) if error.is_refused() => return Ok(None),
        Err(error) => return Err(ApiError::from(error.into_problem("the Gateways"))),
    };
    if listed.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let gateways: Vec<serde_json::Value> = listed
        .iter()
        .filter_map(|g| serde_json::to_value(g).ok())
        .collect();
    // A listener that selects namespaces by label needs the namespace's labels; without them it
    // does not match.
    let namespace = ResourceRef::new("", "v1", "namespaces", "Namespace");
    let labels: BTreeMap<String, String> = cluster
        .get_object(&namespace, None, project)
        .await
        .ok()
        .flatten()
        .and_then(|n| n.metadata.labels)
        .unwrap_or_default();
    Ok(Some(zeabur_plan::attachable(&gateways, project, &labels)))
}

/// The Gateways a new `HTTPRoute` of the project can attach to.
#[utoipa::path(get, path = "/api/v1/projects/{project}/gateways", tag = "projects",
    operation_id = "listProjectGateways",
    params(("project" = String, Path, description = "The project's namespace")),
    responses((status = 200, body = ProjectGateways),
              (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn list(
    Person(cluster): Person,
    Path(project): Path<String>,
) -> Result<Json<ProjectGateways>, ApiError> {
    let found = attachable(&*cluster, &project).await?;
    Ok(Json(ProjectGateways {
        readable: found.is_some(),
        gateways: found
            .unwrap_or_default()
            .into_iter()
            .map(|g| GatewayView {
                namespace: g.namespace,
                name: g.name,
                listeners: g
                    .listeners
                    .into_iter()
                    .map(|l| ListenerView {
                        name: l.name,
                        hostname: l.hostname,
                    })
                    .collect(),
            })
            .collect(),
    }))
}
