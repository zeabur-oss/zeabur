//! What a project restore must tell the person before it runs: what the backup holds of the
//! project.
//!
//! - **The backup.** The lists come from Velero's list of the backup's objects, not from the
//!   project as it runs now (`backup_download::resource_list`). A Service is reached in the cluster
//!   as `<service>.<namespace>.svc` [k8s-dns], so a copy that keeps every value still reaches the
//!   original's Services. The plan names them, the Secrets to check, the CronJobs that a copy
//!   pauses, and the routes that come back in a last step.
//! - **Deleted projects.** A backup of the whole cluster names no namespaces, so `GET
//!   /api/v1/cluster/backups/{backup}/deleted-projects` reads them from the backup's list. Only the
//!   archive holds their labels, so Zeabur downloads it only when a listed namespace is gone.
//!
//! [k8s-dns]: https://kubernetes.io/docs/concepts/services-networking/dns-pod-service/#namespaces-of-services

use std::collections::BTreeSet;

use axum::extract::State;
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_core::names::{LABEL_PROJECT, is_system_namespace};
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::ResourceRef;
use zeabur_modules::velero::cluster::Inventory;

use crate::cluster::backup_download;
use crate::error::{ApiError, list_read, read_problem};
use crate::extract::{Json, Path};
use crate::handlers::{AppState, Person};

/// What a project restore reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = RestoreCopy)]
pub struct Copy {
    /// The namespace restored.
    pub source: String,
    /// The namespace it is restored into.
    pub into: String,
    /// The original no longer exists, so the backup goes back under its own name.
    pub same_name: bool,
    /// What the backup holds of `source`; absent when Velero's list could not be read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inventory: Option<Inventory>,
    /// Why Velero's list could not be read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unread: Option<String>,
}

/// The routes of a namespace.
pub(crate) fn routes() -> [ResourceRef; 2] {
    [
        ResourceRef::new("networking.k8s.io", "v1", "ingresses", "Ingress"),
        ResourceRef::new("gateway.networking.k8s.io", "v1", "httproutes", "HTTPRoute"),
    ]
}

/// The projects a backup holds that no longer exist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct DeletedProjects {
    /// Their names, sorted.
    pub items: Vec<String>,
}

/// The projects in `backup` that no longer exist in the cluster: each comes back under its own
/// name. Read as the person, who must be able to create a `DownloadRequest` in Velero's
/// namespace.
#[utoipa::path(get, path = "/api/v1/cluster/backups/{backup}/deleted-projects", tag = "cluster",
    operation_id = "listDeletedBackupProjects",
    params(("backup" = String, Path, description = "The backup")),
    responses((status = 200, body = DeletedProjects),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM),
              (status = 503, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn deleted_projects(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path(backup): Path<String>,
) -> Result<Json<DeletedProjects>, ApiError> {
    let cluster = cluster.as_ref();
    let list = backup_download::resource_list(&state, cluster, &backup).await?;
    let live: BTreeSet<String> = cluster
        .namespaces()
        .await
        .map_err(|e| {
            read_problem(
                e,
                "the list of namespaces",
                list_read("", "namespaces", None),
            )
        })?
        .into_iter()
        .filter_map(|n| n.metadata.name)
        .collect();
    let gone: Vec<String> = list
        .get("v1/Namespace")
        .into_iter()
        .flatten()
        .filter(|n| !is_system_namespace(n) && !live.contains(*n))
        .cloned()
        .collect();
    if gone.is_empty() {
        return Ok(Json(DeletedProjects { items: Vec::new() }));
    }
    let mut items: Vec<String> = backup_download::namespaces(&state, cluster, &backup, gone)
        .await?
        .iter()
        .filter(|n| {
            n.pointer("/metadata/labels")
                .and_then(Value::as_object)
                .is_some_and(|l| l.contains_key(LABEL_PROJECT))
        })
        .filter_map(|n| n.pointer("/metadata/name").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    items.sort();
    Ok(Json(DeletedProjects { items }))
}
