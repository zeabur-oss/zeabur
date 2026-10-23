//! The object-storage capability in the API: `GET /projects/{p}/object-storage`.
//!
//! It lists the `SeaweedFS` Operator's stores in the project, with their buckets, where to
//! connect and the Secret holding the access key (see `zeabur_modules::seaweedfs`). The objects
//! are read as the person. An operator whose API is not served is "not installed"; a refused read
//! is named in `unreadable`. No Secret is read: the interface reveals the keys, as the person,
//! only when asked (`POST /projects/{p}/secrets/{name}/reveal`).
//!
//! Stores and buckets are created and deleted through plans: the kinds are in
//! `zeabur_plan::CREATABLE`.

use axum::Json;
use axum::extract::Path;
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::api::core::v1::Pod;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{Cluster, ClusterExt};
use zeabur_modules::seaweedfs::{self, ObjectStorage};

use crate::error::ApiError;
use crate::handlers::Person;
use crate::projects::backups::{ProviderState, read_all};
use crate::views::UnreadableView;

/// A project's object storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectObjectStorage {
    /// The operator's module (`seaweedfs-operator`).
    pub module: String,
    /// Whether its objects could be read.
    pub state: ProviderState,
    /// The stores, by name.
    pub items: Vec<ObjectStorage>,
    /// What could not be read.
    pub unreadable: Vec<UnreadableView>,
}

/// The object storage of a project, read as the person.
#[utoipa::path(get, path = "/api/v1/projects/{project}/object-storage", tag = "projects",
    operation_id = "getProjectObjectStorage",
    params(("project" = String, Path, description = "The project's namespace")),
    responses((status = 200, body = ProjectObjectStorage), (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    Person(cluster): Person,
    Path(project): Path<String>,
) -> Result<Json<ProjectObjectStorage>, ApiError> {
    read(cluster.as_ref(), &project).await.map(Json)
}

async fn read(cluster: &dyn Cluster, project: &str) -> Result<ProjectObjectStorage, ApiError> {
    let failed = |e: zeabur_kube::ClusterError| {
        ApiError::from(e.into_problem(&format!("object storage of {project}")))
    };
    let mut unreadable = Vec::new();
    let read = read_all(
        cluster,
        [
            &seaweedfs::SEAWEEDS,
            &seaweedfs::BUCKETS,
            &seaweedfs::CREDENTIALS,
        ],
        Some(project),
        &mut unreadable,
    )
    .await
    .map_err(failed)?;
    let (state, mut items) = match read {
        Ok([stores, buckets, credentials]) => (
            ProviderState::Installed,
            stores
                .iter()
                .map(|s| seaweedfs::object_storage(project, s, &buckets, &credentials))
                .collect::<Vec<_>>(),
        ),
        Err(state) => (state, Vec::new()),
    };
    if !items.is_empty() {
        let pods = match cluster.list::<Pod>(Some(project)).await {
            Ok(pods) => pods,
            Err(error) if error.code() == Some(403) => Vec::new(),
            Err(error) => return Err(failed(error)),
        };
        let now = Some(Timestamp::now());
        let empty = std::collections::BTreeMap::new();
        for store in &mut items {
            let mut pods: Vec<_> = pods
                .iter()
                .filter(|p| store.is_member(p.metadata.labels.as_ref().unwrap_or(&empty)))
                .map(|p| zeabur_graph::pod_summary(project, p, now))
                .collect();
            pods.sort_by(|a, b| a.name.cmp(&b.name));
            store.pods = pods;
        }
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(ProjectObjectStorage {
        module: seaweedfs::MODULE.id.to_owned(),
        state,
        items,
        unreadable,
    })
}
