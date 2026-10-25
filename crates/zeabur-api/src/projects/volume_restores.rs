//! Restoring a volume from its K8up backups:
//!
//! `GET /projects/{p}/volumes/{claim}/restores`: the snapshots of the claim, its restores, and
//! the parts of a `Restore` the interface puts in a plan (the backup's backend and Pod security).

use axum::Json;
use axum::extract::Path;
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::{Cluster, DynamicObject};
use zeabur_modules::k8up::{self, RestoreView, SnapshotView};

use super::backups::{ProviderState, read_all};
use crate::error::ApiError;
use crate::handlers::Person;
use crate::views::UnreadableView;

/// What a claim can be restored from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VolumeRestores {
    /// Whether K8up's objects could be read.
    pub state: ProviderState,
    /// The schedule of the claim's backups.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    /// The snapshots of the claim, newest first. Only these times can be restored.
    pub snapshots: Vec<SnapshotView>,
    /// The restores of the claim's backups, newest first.
    pub restores: Vec<RestoreView>,
    /// A `Restore` with the backup's backend and Pod security, for a snapshot and a method.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<Value>,
    /// What could not be read.
    pub unreadable: Vec<UnreadableView>,
}

async fn k8up_objects(
    cluster: &dyn Cluster,
    project: &str,
    unreadable: &mut Vec<UnreadableView>,
) -> Result<Result<[Vec<DynamicObject>; 3], ProviderState>, ApiError> {
    read_all(
        cluster,
        [&k8up::SCHEDULES, &k8up::SNAPSHOTS, &k8up::RESTORES],
        Some(project),
        unreadable,
    )
    .await
    .map_err(|e| ApiError::from(e.into_problem(&format!("volume restores of {project}"))))
}

/// What `claim` can be restored from: the snapshots K8up lists for it, its restores, and the
/// parts of a `Restore`, read as the person.
#[utoipa::path(get, path = "/api/v1/projects/{project}/volumes/{claim}/restores", tag = "projects",
    operation_id = "getVolumeRestores",
    params(("project" = String, Path, description = "The project's namespace"),
           ("claim" = String, Path, description = "The claim")),
    responses((status = 200, body = VolumeRestores), (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    Person(cluster): Person,
    Path((project, claim)): Path<(String, String)>,
) -> Result<Json<VolumeRestores>, ApiError> {
    let mut unreadable = Vec::new();
    let found = k8up_objects(cluster.as_ref(), &project, &mut unreadable).await?;
    let mut out = VolumeRestores {
        state: ProviderState::Installed,
        schedule: None,
        snapshots: Vec::new(),
        restores: Vec::new(),
        template: None,
        unreadable,
    };
    match found {
        Ok([schedules, snapshots, restores]) => {
            out.restores = k8up::restores(&restores, &claim);
            if let Some(schedule) = k8up::schedule_of(&schedules, &claim) {
                out.schedule.clone_from(&schedule.metadata.name);
                out.snapshots = k8up::snapshots(&snapshots, schedule, &claim);
                out.template = Some(k8up::restore_template(schedule, &claim));
            }
        }
        Err(state) => out.state = state,
    }
    Ok(Json(out))
}
