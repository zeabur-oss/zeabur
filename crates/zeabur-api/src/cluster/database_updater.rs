//! Whether the chart's database updater is ready, read as the signed-in person.

use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
use zeabur_kube::k8s_openapi::api::core::v1::ConfigMap;
use zeabur_kube::{ClusterError, ClusterExt as _, INFO_CONFIG_MAP, INFO_NAMESPACE, ResourceRef};
use zeabur_model::readiness;

use crate::extract::Json;
use crate::handlers::Person;

/// Whether automatic database updates can be enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum DatabaseUpdaterStatus {
    /// The chart has not configured an updater, or its Deployment is absent.
    Missing,
    /// The updater exists but is paused, terminating or still starting.
    NotReady,
    /// The updater's Deployment has reached its requested state and has running replicas.
    Ready,
    /// The signed-in person could not check the updater.
    Unknown,
}

/// The native state of the chart-owned database updater.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseUpdater {
    /// Whether it is installed and ready.
    pub status: DatabaseUpdaterStatus,
    /// A safe explanation without the cluster's connection details or Deployment contents.
    pub message: String,
    /// Why the state could not be checked, including permission refusals.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<Problem>,
}

impl DatabaseUpdater {
    fn state(status: DatabaseUpdaterStatus, message: &str) -> Self {
        Self {
            status,
            message: message.to_owned(),
            problem: None,
        }
    }

    fn missing() -> Self {
        Self::state(
            DatabaseUpdaterStatus::Missing,
            "Automatic database updates are not installed. Ask a cluster administrator to enable the database updater.",
        )
    }

    fn unknown(error: ClusterError) -> Self {
        let (kind, message) = match error {
            ClusterError::Status(status) if status.is_refusal() => (
                Kind::Forbidden,
                "Your account cannot check the database updater. Ask a cluster administrator for access.",
            ),
            ClusterError::Decode(_) | ClusterError::InvalidName(_) => (
                Kind::Internal,
                "The database updater's configuration could not be read. Ask a cluster administrator to check its installation.",
            ),
            ClusterError::Transport(_) => (
                Kind::Unavailable,
                "The cluster could not be reached to check the database updater. Try again.",
            ),
            ClusterError::Status(_) => (
                Kind::Unavailable,
                "The database updater's status could not be checked. Try again.",
            ),
        };
        Self {
            problem: Some(Problem::new(kind, message)),
            ..Self::state(DatabaseUpdaterStatus::Unknown, message)
        }
    }
}

/// The updater's live Deployment state. Reads only the public locator and its named Deployment,
/// with the person's credential; an unreadable object is unknown, not missing.
#[utoipa::path(get, path = "/api/v1/cluster/database-updater", tag = "cluster", operation_id = "getDatabaseUpdater",
    responses((status = 200, body = DatabaseUpdater), (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(Person(cluster): Person) -> Json<DatabaseUpdater> {
    let map = cluster
        .find_object(
            &ResourceRef::new("", "v1", "configmaps", "ConfigMap"),
            Some(INFO_NAMESPACE),
            INFO_CONFIG_MAP,
        )
        .await;
    let map: ConfigMap = match map {
        Ok(Some(map)) => match map.try_parse() {
            Ok(map) => map,
            Err(error) => {
                return Json(DatabaseUpdater::unknown(ClusterError::Decode(
                    error.to_string(),
                )));
            }
        },
        Ok(None) => return Json(DatabaseUpdater::missing()),
        Err(error) => return Json(DatabaseUpdater::unknown(error)),
    };
    let data = map.data.unwrap_or_default();
    let namespace = data.get("databaseUpdaterNamespace").map(|s| s.trim());
    let name = data.get("databaseUpdaterDeployment").map(|s| s.trim());
    let (namespace, name) = match (
        namespace.filter(|s| !s.is_empty()),
        name.filter(|s| !s.is_empty()),
    ) {
        (None, None) => return Json(DatabaseUpdater::missing()),
        (Some(namespace), Some(name)) => (namespace, name),
        _ => {
            return Json(DatabaseUpdater::unknown(ClusterError::Decode(
                "incomplete updater location".to_owned(),
            )));
        }
    };
    let deployment = cluster
        .find_object(
            &ResourceRef::new("apps", "v1", "deployments", "Deployment"),
            Some(namespace),
            name,
        )
        .await;
    Json(match deployment {
        Ok(Some(deployment)) => match deployment.try_parse() {
            Ok(deployment) => deployment_state(&deployment),
            Err(error) => DatabaseUpdater::unknown(ClusterError::Decode(error.to_string())),
        },
        Ok(None) => DatabaseUpdater::missing(),
        Err(error) => DatabaseUpdater::unknown(error),
    })
}

fn deployment_state(deployment: &Deployment) -> DatabaseUpdater {
    let running = deployment
        .spec
        .as_ref()
        .and_then(|spec| spec.replicas)
        .unwrap_or(1)
        > 0;
    if deployment.metadata.deletion_timestamp.is_none()
        && running
        && readiness::deployment(deployment).is_current()
    {
        DatabaseUpdater::state(
            DatabaseUpdaterStatus::Ready,
            "Automatic database updates are ready.",
        )
    } else {
        DatabaseUpdater::state(
            DatabaseUpdaterStatus::NotReady,
            "The database updater is not ready. Existing updates can still be turned off.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_and_decode_failures_do_not_expose_their_contents() {
        for error in [
            ClusterError::Transport("https://private.example/?token=secret".to_owned()),
            ClusterError::Decode("private deployment contents".to_owned()),
        ] {
            let view = DatabaseUpdater::unknown(error);
            assert_eq!(view.status, DatabaseUpdaterStatus::Unknown);
            let body = serde_json::to_string(&view).unwrap();
            assert!(!body.contains("private"));
            assert!(!body.contains("secret"));
        }
    }
}
