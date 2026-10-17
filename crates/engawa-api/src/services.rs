//! A project's services (workloads): each cut from the project document, its releases and the
//! history of its usage.

pub mod history;
pub mod releases;

use axum::Json;
use axum::extract::Path;
use engawa_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use engawa_model::WorkloadKind;

use crate::error::ApiError;
use crate::handlers::Person;
use crate::projects::{document_of, not_found};
use crate::views::ServiceDocument;

/// One service (workload) of a project, cut from the project document: for deep links, scripts
/// and the MCP server. The web interface reads the project document instead.
#[utoipa::path(get, path = "/api/v1/projects/{project}/services/{kind}/{name}", tag = "services",
    params(("project" = String, Path),
           ("kind" = String, Path, description = "deployment, statefulset, daemonset, job or cronjob; any case"),
           ("name" = String, Path)),
    responses((status = 200, body = ServiceDocument), (status = 404, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn service(
    Person(cluster): Person,
    Path((project, kind, name)): Path<(String, String, String)>,
) -> Result<Json<ServiceDocument>, ApiError> {
    let Some(workload_kind) = WorkloadKind::parse(&kind) else {
        return Err(not_found(&project, &kind, &name));
    };
    let document = document_of(cluster.as_ref(), &project).await?;
    let service = document
        .project
        .services
        .into_iter()
        .find(|s| s.workload.kind == workload_kind && s.workload.name == name)
        .ok_or_else(|| not_found(&project, workload_kind.as_str(), &name))?;
    Ok(Json(ServiceDocument {
        service,
        unreadable: document.unreadable,
    }))
}
