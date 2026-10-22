//! Whether Stakater Reloader runs in the cluster (`zeabur_modules::reloader`), for a service's
//! "Restart when its configuration changes" setting and for plans that change a ConfigMap or
//! Secret.
//!
//! Reloader's Deployment is found by listing Deployments' metadata in every namespace, as the
//! person; a person who may not do that gets `unknown`, never a `403`: the setting is theirs to
//! use on their own workloads whoever may see Reloader.

use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::Cluster;
use zeabur_model::Origin;
use zeabur_modules::reloader;

use crate::error::ApiError;
use crate::handlers::Person;

/// Whether Reloader runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ReloaderState {
    /// Its Deployment has an available replica.
    Running,
    /// Its Deployment is there with no available replica.
    Stopped,
    /// No Deployment of it was found.
    NotInstalled,
    /// Deployments of every namespace could not be listed with the person's rights.
    Unknown,
}

impl ReloaderState {
    /// What a plan is told: `Some(true)` when it runs, `None` when that is not known.
    pub(crate) const fn runs(self) -> Option<bool> {
        match self {
            Self::Running => Some(true),
            Self::Stopped | Self::NotInstalled => Some(false),
            Self::Unknown => None,
        }
    }
}

/// Whether the cluster restarts workloads when their configuration changes.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigReloadView {
    /// `cluster/config-reload`.
    pub id: &'static str,
    /// Whether Reloader runs.
    pub reloader: ReloaderState,
    /// Its Deployments, when found.
    pub deployments: Vec<Origin>,
}

/// Reads whether Reloader runs, and its Deployments.
pub(crate) async fn read(cluster: &dyn Cluster) -> (ReloaderState, Vec<Origin>) {
    let resource = reloader::DEPLOYMENTS.resource();
    let metadata = match cluster.list_metadata(&resource, None).await {
        Ok(Some(list)) => list,
        Ok(None) => return (ReloaderState::NotInstalled, Vec::new()),
        Err(_) => return (ReloaderState::Unknown, Vec::new()),
    };
    let mut deployments = Vec::new();
    let mut origins = Vec::new();
    for meta in metadata.iter().filter(|m| reloader::is_reloader(m)) {
        let (Some(namespace), Some(name)) = (meta.namespace.as_deref(), meta.name.as_deref())
        else {
            continue;
        };
        origins.push(Origin::complete(
            "apps/v1",
            "Deployment",
            Some(namespace),
            name,
        ));
        match cluster.get_object(&resource, Some(namespace), name).await {
            Ok(Some(object)) => deployments.push(object),
            Ok(None) => {}
            Err(_) => return (ReloaderState::Unknown, origins),
        }
    }
    let state = if origins.is_empty() {
        ReloaderState::NotInstalled
    } else if reloader::running(&deployments) {
        ReloaderState::Running
    } else {
        ReloaderState::Stopped
    };
    (state, origins)
}

/// Whether Stakater Reloader runs in the cluster, so a workload's Reloader annotation takes
/// effect. Always answers: `unknown` when the person may not list Deployments in every
/// namespace.
#[utoipa::path(get, path = "/api/v1/cluster/config-reload", tag = "cluster",
    operation_id = "getConfigReload",
    responses((status = 200, body = ConfigReloadView),
        (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(Person(cluster): Person) -> Result<Json<ConfigReloadView>, ApiError> {
    let (reloader, deployments) = read(cluster.as_ref()).await;
    Ok(Json(ConfigReloadView {
        id: "cluster/config-reload",
        reloader,
        deployments,
    }))
}
