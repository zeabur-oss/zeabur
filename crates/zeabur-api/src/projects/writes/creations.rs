//! What a plan reads before it creates an object: whether the name is taken.

use zeabur_core::problem::{Kind, Problem};
use zeabur_kube::Cluster;
use zeabur_plan::{Change, PlanRequest};

use crate::error::{ApiError, get_read, read_problem};

/// Refuses a `create` whose object already exists: server-side apply would otherwise update it.
pub(super) async fn refuse_existing(
    cluster: &dyn Cluster,
    project: &str,
    request: &PlanRequest,
) -> Result<(), ApiError> {
    for change in &request.changes {
        let Change::Create { object } = change else {
            continue;
        };
        let text = |path: &str| object.pointer(path).and_then(serde_json::Value::as_str);
        let (Some(api_version), Some(kind), Some(name)) =
            (text("/apiVersion"), text("/kind"), text("/metadata/name"))
        else {
            continue;
        };
        // The planner refuses kinds it does not create; nothing to look up for them.
        let Some(plural) = zeabur_plan::creatable(api_version, kind) else {
            continue;
        };
        let (group, version) = api_version.split_once('/').unwrap_or(("", api_version));
        let resource = zeabur_kube::ResourceRef::new(group, version, plural, kind);
        let found = match cluster.get_object(&resource, Some(project), name).await {
            Ok(found) => found.is_some(),
            Err(error) if error.is_not_found() => false,
            Err(other) => {
                return Err(read_problem(
                    other,
                    &format!("{kind} {name}"),
                    get_read(group, plural, Some(project), name),
                ));
            }
        };
        if found {
            return Err(ApiError::from(Problem::new(
                Kind::Conflict,
                format!("{kind} {name} already exists in project {project}."),
            )));
        }
    }
    Ok(())
}
