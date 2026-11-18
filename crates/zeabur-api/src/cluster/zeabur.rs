//! Zeabur's own version and how it was installed, for the interface's update check.
//!
//! [`super::update`] compares it with the releases on GitHub.

use axum::extract::State;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::{Cluster, ClusterExt as _, ResourceRef};
use zeabur_model::Tool;
use zeabur_model::adapters::provenance::detect_tool;

use crate::error::ApiError;
use crate::extract::Json;
use crate::handlers::{AppState, Person};
use crate::settings::Settings;
use crate::views::{InstallMethod, ZeaburVersion};

/// Zeabur's version, whether the interface checks for a newer one, and how Zeabur was installed,
/// from its Deployment's marks (Helm, Argo CD, Flux). The Deployment is read as the person; a
/// refused read leaves `installedBy` out.
#[utoipa::path(get, path = "/api/v1/cluster/zeabur", tag = "cluster", operation_id = "getZeabur",
    responses((status = 200, body = ZeaburVersion), (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    State(state): State<AppState>,
    Person(cluster): Person,
) -> Result<Json<ZeaburVersion>, ApiError> {
    Ok(Json(ZeaburVersion {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        update_check: state.settings.update_check,
        installed_by: installed_by(cluster.as_ref(), &state.settings).await,
    }))
}

async fn installed_by(cluster: &dyn Cluster, settings: &Settings) -> Option<InstallMethod> {
    let Some(namespace) = settings.namespace.as_deref() else {
        return Some(InstallMethod::Zeaburctl);
    };
    let deployment = cluster
        .find_object(
            &ResourceRef::new("apps", "v1", "deployments", "Deployment"),
            Some(namespace),
            settings.deployment.as_deref()?,
        )
        .await
        .ok()??;
    Some(match detect_tool(&deployment.metadata)? {
        Tool::ArgoCd => InstallMethod::ArgoCd,
        Tool::Flux => InstallMethod::Flux,
        Tool::Helm => InstallMethod::Helm,
    })
}
