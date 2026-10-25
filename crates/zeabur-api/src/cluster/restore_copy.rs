//! What a project restore must tell the person before it runs: what the backup holds of the
//! project, and whether the cluster has room for a second copy.
//!
//! - **The backup.** The lists come from Velero's list of the backup's objects, not from the
//!   project as it runs now (`backup_download::resource_list`). A Service is reached in the cluster
//!   as `<service>.<namespace>.svc` [k8s-dns], so a copy that keeps every value still reaches the
//!   original's Services. The plan names them, the Secrets to check, the CronJobs that a copy
//!   pauses, and the routes that come back in a last step.
//! - **Room.** The copy's pods request what the original's running pods request. The room is what
//!   every ready, schedulable node has left, as a plan's fit counts it.
//! - **Deleted projects.** A backup of the whole cluster names no namespaces, so `GET
//!   /api/v1/cluster/backups/{backup}/deleted-projects` reads them from the backup's list. Only the
//!   archive holds their labels, so Zeabur downloads it only when a listed namespace is gone.
//!
//! [k8s-dns]: https://kubernetes.io/docs/concepts/services-networking/dns-pod-service/#namespaces-of-services

use std::collections::BTreeSet;

use axum::Json;
use axum::extract::{Path, State};
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_core::names::{LABEL_PROJECT, is_system_namespace};
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::api::core::v1::Node;
use zeabur_kube::{Cluster, ClusterExt as _, ClusterPod, ResourceRef};
use zeabur_modules::velero::cluster::Inventory;

use crate::cluster::backup_download;
use crate::cluster::capacity::{Amount, pod_amounts};
use crate::error::{ApiError, list_read, read_problem};
use crate::handlers::{AppState, Person};
use crate::projects::writes::preview::node_room;

/// The most pods read to count the room.
const PODS_READ: usize = 5000;

/// Whether the cluster has room for the copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = RestoreFit)]
pub enum Fit {
    /// The nodes have room for what the original's pods request.
    Fits,
    /// They do not.
    Short,
    /// Nodes or pods could not be read.
    Unknown,
}

/// The room the copy needs and the room there is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = RestoreRoom)]
pub struct Room {
    /// The verdict.
    pub fit: Fit,
    /// What the original's running pods request: millicores.
    pub need_cpu: u64,
    /// Bytes.
    pub need_memory: u64,
    /// What the nodes have left: millicores.
    pub free_cpu: u64,
    /// Bytes.
    pub free_memory: u64,
}

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
    /// Room for a second copy; absent under the same name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room: Option<Room>,
}

/// The room the running pods of `source` need against what `room` has left.
#[must_use]
pub(crate) fn room(pods: &[ClusterPod], source: &str, room: Option<&[(Amount, u64)]>) -> Room {
    let mut need = Amount::default();
    for pod in pods.iter().filter(|p| p.namespace == source) {
        if !matches!(pod.phase.as_deref(), Some("Succeeded" | "Failed")) {
            need.add(pod_amounts(pod).0);
        }
    }
    let mut free = Amount::default();
    for (left, _) in room.unwrap_or_default() {
        free.add(*left);
    }
    let fit = match room {
        None => Fit::Unknown,
        Some(_) if need.cpu > free.cpu || need.memory > free.memory => Fit::Short,
        Some(_) => Fit::Fits,
    };
    Room {
        fit,
        need_cpu: need.cpu,
        need_memory: need.memory,
        free_cpu: free.cpu,
        free_memory: free.memory,
    }
}

/// The routes of a namespace.
pub(crate) fn routes() -> [ResourceRef; 2] {
    [
        ResourceRef::new("networking.k8s.io", "v1", "ingresses", "Ingress"),
        ResourceRef::new("gateway.networking.k8s.io", "v1", "httproutes", "HTTPRoute"),
    ]
}

/// Reads, as the person, the room for a copy of `copy.source` into `copy`.
pub(crate) async fn read_room(cluster: &dyn Cluster, copy: &mut Copy) {
    let source = copy.source.as_str();
    let (nodes, pods) =
        futures_util::join!(cluster.list::<Node>(None), cluster.cluster_pods(PODS_READ));
    let fit = match (nodes, pods) {
        (Ok(nodes), Ok(pods)) if !pods.truncated => {
            let left = node_room(&nodes, &pods.pods);
            room(&pods.pods, source, Some(&left))
        }
        (_, Ok(pods)) => room(&pods.pods, source, None),
        _ => room(&[], source, None),
    };
    copy.room = Some(fit);
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

#[cfg(test)]
mod tests {
    use zeabur_kube::ContainerResources;

    use super::*;

    fn pod(namespace: &str, cpu: &str, memory: &str, phase: &str) -> ClusterPod {
        ClusterPod {
            namespace: namespace.to_owned(),
            phase: Some(phase.to_owned()),
            containers: vec![ContainerResources {
                requests: [("cpu", cpu), ("memory", memory)]
                    .into_iter()
                    .map(|(k, v)| (k.to_owned(), v.to_owned()))
                    .collect(),
                ..ContainerResources::default()
            }],
            ..ClusterPod::default()
        }
    }

    #[test]
    fn a_copy_needs_what_the_running_pods_request_and_fits_only_in_the_room_left() {
        let pods = [
            pod("blog", "500m", "1Gi", "Running"),
            pod("blog", "4", "8Gi", "Succeeded"),
            pod("shop", "2", "2Gi", "Running"),
        ];
        let roomy = [(
            Amount {
                cpu: 1000,
                memory: 4 << 30,
            },
            10,
        )];
        let tight = [(
            Amount {
                cpu: 400,
                memory: 4 << 30,
            },
            10,
        )];
        let fits = room(&pods, "blog", Some(&roomy));
        assert_eq!(
            (fits.fit, fits.need_cpu, fits.need_memory),
            (Fit::Fits, 500, 1 << 30)
        );
        assert_eq!(room(&pods, "blog", Some(&tight)).fit, Fit::Short);
        assert_eq!(room(&pods, "blog", None).fit, Fit::Unknown);
    }
}
