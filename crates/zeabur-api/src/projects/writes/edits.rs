//! What a plan reads before it edits an object as YAML, or deletes it: the object itself.

use zeabur_kube::{Cluster, ClusterError, ClusterExt as _};
use zeabur_plan::{Change, Fetched, PlanRequest};

use crate::error::{ApiError, get_read, read_problem};

/// The objects `request` edits as YAML or deletes.
///
/// A Secret is read with its values hidden ([`zeabur_kube::ResourceRef::is_secret`]): the plan
/// takes the values from what the person revealed, and never needs them for its other fields.
/// A ConfigMap or Secret the person may list but not read is left to the plan, which takes it
/// from the project's objects.
pub(super) async fn edited_objects(
    cluster: &dyn Cluster,
    project: &str,
    request: &PlanRequest,
) -> Result<Vec<Fetched>, ApiError> {
    let mut out = Vec::new();
    for change in &request.changes {
        let (resource, name) = if let Change::Edit { resource, name, .. } = change
            && zeabur_plan::checks::resource_allowed(resource).allowed
        {
            (resource, name)
        } else if let Change::Delete { resource, name, .. } = change
            && resource.resource != "namespaces"
        {
            (resource, name)
        } else {
            continue;
        };
        let kind = if resource.group.is_empty() && resource.resource == "secrets" {
            "Secret"
        } else {
            ""
        };
        let reference = zeabur_kube::ResourceRef::new(
            &resource.group,
            &resource.version,
            &resource.resource,
            kind,
        );
        let read = cluster
            .find_object(&reference, Some(project), name)
            .await
            .and_then(|found| {
                found
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(|e| ClusterError::Decode(e.to_string()))
            });
        let listed = resource.group.is_empty()
            && matches!(resource.resource.as_str(), "configmaps" | "secrets");
        let read = match read {
            Err(error) if listed && (error.is_refused() || error.is_not_found()) => Ok(None),
            other => other,
        };
        if let Some(object) = read.map_err(|e| {
            read_problem(
                e,
                &format!("{} {name}", resource.resource),
                get_read(&resource.group, &resource.resource, Some(project), name),
            )
        })? {
            out.push(Fetched {
                resource: resource.clone(),
                name: name.clone(),
                object,
            });
        }
    }
    Ok(out)
}
