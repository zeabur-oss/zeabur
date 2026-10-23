//! Plans that write the project's Namespace only: creating it, adopting it, letting it go, its
//! details.

use zeabur_kube::{Cluster, ClusterError};
use zeabur_plan::{Change, Input, Plan, PlanRequest};

use super::plan_problem;
use crate::error::{ApiError, get_read, read_problem};

/// Plans adopting the namespace, letting it go or the project's details: these read and write
/// the Namespace only, so the objects in it are not read (a person may be allowed to label a
/// namespace without reading what is in it). The planner refuses them mixed with other changes.
pub(super) async fn compute_namespace(
    cluster: &dyn Cluster,
    project: &str,
    request: &PlanRequest,
    now: &str,
) -> Result<Plan, ApiError> {
    let resource = zeabur_kube::ResourceRef::new("", "v1", "namespaces", "Namespace");
    let what = format!("namespace {project}");
    let creating = request
        .changes
        .iter()
        .any(|c| matches!(c, Change::CreateProject { .. }));
    let namespace = match cluster.get_object(&resource, None, project).await {
        Err(e) if creating && e.is_not_found() => Ok(None),
        read => read,
    }
    .map_err(|e| read_problem(e, &what, get_read("", "namespaces", None, project)))?
    .map(serde_json::to_value)
    .transpose()
    .map_err(|e| ApiError::from(ClusterError::Decode(e.to_string()).into_problem(&what)))?;
    let snapshot = zeabur_graph::Snapshot {
        namespace: project.to_owned(),
        ..zeabur_graph::Snapshot::default()
    };
    zeabur_plan::plan(
        Input {
            namespace: namespace.as_ref(),
            ..Input::new(&snapshot, now)
        },
        request,
    )
    .map_err(plan_problem)
}

/// The Namespace, for a plan that sets the project's Pod Security level or a service's profile
/// (whose plan says when the project could be raised). Only the level needs it: for a profile
/// alone, a Namespace the person may not read is left out.
pub(super) async fn pod_security_namespace(
    cluster: &dyn Cluster,
    project: &str,
    request: &PlanRequest,
) -> Result<Option<serde_json::Value>, ApiError> {
    let level = request
        .changes
        .iter()
        .any(|c| matches!(c, Change::PodSecurity { .. }));
    let profile = request
        .changes
        .iter()
        .any(|c| matches!(c, Change::SecurityProfile { .. }));
    if !level && !profile {
        return Ok(None);
    }
    let resource = zeabur_kube::ResourceRef::new("", "v1", "namespaces", "Namespace");
    let what = format!("namespace {project}");
    match cluster.get_object(&resource, None, project).await {
        Ok(namespace) => namespace
            .map(serde_json::to_value)
            .transpose()
            .map_err(|e| ApiError::from(ClusterError::Decode(e.to_string()).into_problem(&what))),
        Err(_) if !level => Ok(None),
        Err(e) => Err(read_problem(
            e,
            &what,
            get_read("", "namespaces", None, project),
        )),
    }
}
