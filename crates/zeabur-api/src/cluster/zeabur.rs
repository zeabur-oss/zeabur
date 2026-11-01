//! Zeabur's own version and how it was installed, for the interface's update check.
//!
//! Zeabur makes no request to GitHub: the interface reads the releases from the person's browser
//! (docs/reference/telemetry.mdx, "Update check").

use axum::Json;
use axum::extract::State;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::{Cluster, ClusterExt as _, ResourceRef};
use zeabur_model::Tool;
use zeabur_model::adapters::provenance::{
    HELM_RELEASE_NAME_ANNOTATION, MANAGED_BY_LABEL, detect_tool,
};

use crate::error::ApiError;
use crate::handlers::{AppState, Person};
use crate::settings::Settings;
use crate::views::{InstallMethod, ZeaburVersion};

/// The `app.kubernetes.io/managed-by` value on zeabur-installer's `HelmChart` objects.
const INSTALLER: &str = "zeabur-installer";

/// Zeabur's version, whether the interface checks for a newer one, and how Zeabur was installed:
/// from its Deployment's marks (Helm, Argo CD, Flux) and, for a Helm release, zeabur-installer's
/// `HelmChart` of that name in `kube-system`. Both are read as the person; a refused read leaves
/// `installedBy` out.
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
    match detect_tool(&deployment.metadata)? {
        Tool::ArgoCd => Some(InstallMethod::ArgoCd),
        Tool::Flux => Some(InstallMethod::Flux),
        Tool::Helm => {
            let release = deployment
                .metadata
                .annotations
                .as_ref()
                .and_then(|a| a.get(HELM_RELEASE_NAME_ANNOTATION));
            let Some(release) = release else {
                return Some(InstallMethod::Helm);
            };
            let chart = cluster
                .find_object(
                    &ResourceRef::new("helm.cattle.io", "v1", "helmcharts", "HelmChart"),
                    Some("kube-system"),
                    release,
                )
                .await
                .ok()?;
            let by_installer = chart.is_some_and(|chart| {
                chart
                    .metadata
                    .labels
                    .as_ref()
                    .and_then(|l| l.get(MANAGED_BY_LABEL))
                    .is_some_and(|v| v == INSTALLER)
            });
            Some(if by_installer {
                InstallMethod::Installer
            } else {
                InstallMethod::Helm
            })
        }
    }
}
