//! The kinds a person may create in a project, for writing objects as YAML.
//!
//! `GET /projects/{p}/creatable` asks discovery which resources the cluster serves and keeps the
//! namespaced kinds that can be created, read and listed, leaving out what controllers generate
//! and events. Each carries an `access` hint: denied when the project is read-only for the person
//! ([`zeabur_kube::AccessRules::read_only`]), unknown otherwise; the plan's dry run is what
//! decides. The cluster-scoped kinds are named too, so a client can say why one is refused before
//! planning.

use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::ServedResource;

use super::objects::{GENERATED, ObjectAccess};
use crate::error::ApiError;
use crate::extract::{Json, Path};
use crate::handlers::Person;

/// A kind that can be created in a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatableKind {
    /// The API group; empty for the core group.
    pub group: String,
    /// The preferred version.
    pub version: String,
    /// The plural resource name.
    pub resource: String,
    /// The kind.
    pub kind: String,
    /// Not one of Kubernetes' own groups: a custom resource, or an extension API.
    pub custom: bool,
    /// Whether the person may create it: denied in a read-only project, else unknown.
    pub access: ObjectAccess,
}

/// A kind that belongs to the whole cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterKind {
    /// The API group; empty for the core group.
    pub group: String,
    /// The kind.
    pub kind: String,
}

/// What can be created in a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatableList {
    /// The namespaced kinds, by group then kind.
    pub items: Vec<CreatableKind>,
    /// The cluster-scoped kinds, which a project cannot hold.
    pub cluster_scoped: Vec<ClusterKind>,
}

fn creatable(served: &ServedResource) -> bool {
    let can = |verb: &str| served.verbs.iter().any(|v| v == verb);
    served.namespaced
        && can("create")
        && can("get")
        && can("list")
        && !GENERATED.contains(&(served.group.as_str(), served.resource.as_str()))
        && served.group != "metrics.k8s.io"
        && !(served.group.is_empty() && served.resource == "events")
}

fn custom(group: &str) -> bool {
    !matches!(group, "" | "apps" | "batch" | "autoscaling" | "policy")
        && !group.ends_with(".k8s.io")
}

/// The kinds the person may create in a project, each with an access hint, and the kinds that
/// belong to the whole cluster.
#[utoipa::path(get, path = "/api/v1/projects/{project}/creatable", tag = "resources", operation_id = "listCreatable",
    params(("project" = crate::rules::ProjectName, Path)),
    responses((status = 200, body = CreatableList), (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn list(
    Person(cluster): Person,
    Path(project): Path<String>,
) -> Result<Json<CreatableList>, ApiError> {
    let served = cluster
        .served_resources()
        .await
        .map_err(|e| ApiError::from(e.into_problem("the cluster's APIs")))?;
    let access = match cluster.access_rules(&project).await.map(|r| r.read_only()) {
        Ok(Some(true)) => ObjectAccess::Denied,
        _ => ObjectAccess::Unknown,
    };
    let mut items: Vec<CreatableKind> = served
        .iter()
        .filter(|s| creatable(s))
        .map(|s| CreatableKind {
            group: s.group.clone(),
            version: s.version.clone(),
            resource: s.resource.clone(),
            kind: s.kind.clone(),
            custom: custom(&s.group),
            access,
        })
        .collect();
    items.sort_by(|a, b| (&a.group, &a.kind).cmp(&(&b.group, &b.kind)));
    let mut cluster_scoped: Vec<ClusterKind> = served
        .iter()
        .filter(|s| !s.namespaced)
        .map(|s| ClusterKind {
            group: s.group.clone(),
            kind: s.kind.clone(),
        })
        .collect();
    cluster_scoped.sort_by(|a, b| (&a.group, &a.kind).cmp(&(&b.group, &b.kind)));
    cluster_scoped.dedup();
    Ok(Json(CreatableList {
        items,
        cluster_scoped,
    }))
}
