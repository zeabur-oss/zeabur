//! What a plan reads before it edits an object as YAML, or deletes it: the object itself. The
//! same for the Zeabur Builder `Build` it builds again, changes the options of, or tells of Keel.
//! A plan that sets or removes a service's `Build` reads every `Build` of the project: whether the
//! service has one, and which others read its credentials or make its images.

use zeabur_kube::{Cluster, ClusterError, ClusterExt as _};
use zeabur_plan::{AutoUpdateTarget, Change, Fetched, PlanRequest};

use crate::error::{ApiError, get_read, list_read, read_problem};

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
    let mut out: Vec<Fetched> = Vec::new();
    let builds = zeabur_plan::builds::builds();
    let clusters = zeabur_plan::builds::cloudnative_pg_clusters();
    for change in &request.changes {
        let (resource, name) =
            if let Change::Rebuild { build }
            | Change::UploadToken { build, .. }
            | Change::ArchiveDigest { build, .. }
            | Change::BuildOptions(zeabur_plan::BuildOptionsChange { build, .. })
            | Change::AutoUpdate {
                workload: AutoUpdateTarget::Workload(_),
                update: zeabur_plan::Update::NewBuilds { build, .. },
                ..
            } = change
            {
                (&builds, build)
            } else if let Change::AutoUpdate {
                workload: AutoUpdateTarget::Cluster { name, .. },
                ..
            } = change
            {
                (&clusters, name)
            } else if let Change::Edit { resource, name, .. } = change
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
        if out
            .iter()
            .any(|f| f.resource == *resource && f.name == *name)
        {
            continue;
        }
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
    if request
        .changes
        .iter()
        .any(|c| matches!(c, Change::SetBuild(_) | Change::RemoveBuild { .. }))
    {
        for build in project_builds(cluster, project).await? {
            if !out
                .iter()
                .any(|f| f.resource == build.resource && f.name == build.name)
            {
                out.push(build);
            }
        }
    }
    out.extend(middlewares(cluster, project, request).await?);
    Ok(out)
}

/// The project's `Build`s, with one list; none where Zeabur Builder is not installed.
async fn project_builds(cluster: &dyn Cluster, project: &str) -> Result<Vec<Fetched>, ApiError> {
    let resource = zeabur_plan::builds::builds();
    let reference = zeabur_kube::ResourceRef::new(
        &resource.group,
        &resource.version,
        &resource.resource,
        "Build",
    );
    let listed = cluster
        .list_objects(&reference, Some(project))
        .await
        .map_err(|e| {
            read_problem(
                e,
                "the project's Builds",
                list_read(&resource.group, &resource.resource, Some(project)),
            )
        })?
        .unwrap_or_default();
    listed
        .into_iter()
        .map(|object| {
            let name = object.metadata.name.clone().unwrap_or_default();
            let object = serde_json::to_value(object).map_err(|e| {
                read_problem(
                    ClusterError::Decode(e.to_string()),
                    &format!("builds {name}"),
                    get_read(&resource.group, &resource.resource, Some(project), &name),
                )
            })?;
            Ok(Fetched {
                resource: resource.clone(),
                name,
                object,
            })
        })
        .collect()
}

/// The Traefik middlewares the request's redirects, basic auth and IP limits update or delete,
/// and the NetworkPolicy of its egress limits, where they exist.
async fn middlewares(
    cluster: &dyn Cluster,
    project: &str,
    request: &PlanRequest,
) -> Result<Vec<Fetched>, ApiError> {
    let mut out = Vec::new();
    let mut reads = Vec::new();
    for change in &request.changes {
        reads.extend(zeabur_plan::access::policy_read(change).map(|read| ("NetworkPolicy", read)));
        reads.extend(zeabur_plan::access::middleware_read(change).map(|read| ("Middleware", read)));
    }
    for (kind, (resource, name)) in reads {
        let reference = zeabur_kube::ResourceRef::new(
            &resource.group,
            &resource.version,
            &resource.resource,
            kind,
        );
        // Absent, or not served without Traefik: the plan then makes it, or says Traefik is needed.
        let found = cluster
            .find_object(&reference, Some(project), &name)
            .await
            .and_then(|found| {
                found
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(|e| ClusterError::Decode(e.to_string()))
            })
            .map_err(|e| {
                read_problem(
                    e,
                    &format!("{} {name}", resource.resource),
                    get_read(&resource.group, &resource.resource, Some(project), &name),
                )
            })?;
        if let Some(object) = found {
            out.push(Fetched {
                resource,
                name,
                object,
            });
        }
    }
    Ok(out)
}
