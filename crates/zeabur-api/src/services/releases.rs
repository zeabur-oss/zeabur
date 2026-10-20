//! A service's releases: the pod templates its workload has run, for the Releases tab and
//! rollbacks ([`zeabur_plan::releases`]).

use axum::Json;
use axum::extract::Path;
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_model::WorkloadKind;
use zeabur_plan::Release;

use crate::error::{ApiError, project_read, read_problem};
use crate::handlers::Person;
use crate::projects::snapshot;
use crate::projects::writes::releases::History;

/// A workload's releases.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[schema(as = WorkloadReleaseList)]
pub struct ReleaseList {
    /// Newest first. Jobs and CronJobs have none.
    pub items: Vec<Release>,
}

/// The releases of a service's workload, newest first, each with its pod template; one is
/// `current`. A rollback is a plan (`rollback` change).
#[utoipa::path(get, path = "/api/v1/projects/{project}/services/{kind}/{name}/releases", tag = "services",
    params(("project" = String, Path),
           ("kind" = String, Path, description = "deployment, statefulset, daemonset, job or cronjob; any case"),
           ("name" = String, Path)),
    responses((status = 200, body = ReleaseList), (status = 404, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn releases(
    Person(cluster): Person,
    Path((project, kind, name)): Path<(String, String, String)>,
) -> Result<Json<ReleaseList>, ApiError> {
    let missing = || {
        ApiError::from(Problem::new(
            Kind::NotFound,
            format!("Project {project} has no {kind} named {name}."),
        ))
    };
    let workload_kind = WorkloadKind::parse(&kind).ok_or_else(missing)?;
    let objects = cluster
        .namespace_workloads(&project)
        .await
        .map_err(|e| read_problem(e, &format!("project {project}"), project_read(&project)))?;
    let (snapshot, _) = snapshot(objects);
    let workload = zeabur_graph::workloads(&snapshot)
        .into_iter()
        .find(|w| w.kind() == workload_kind && w.name() == name)
        .ok_or_else(missing)?;
    if matches!(workload_kind, WorkloadKind::Job | WorkloadKind::CronJob) {
        return Ok(Json(ReleaseList { items: Vec::new() }));
    }
    let history = History::read(cluster.as_ref(), &project).await?;
    Ok(Json(ReleaseList {
        items: history.of(workload, &snapshot.pods),
    }))
}
