//! cert-manager's `ClusterIssuers` as the cluster page lists them (see
//! [`zeabur_modules::cert_manager`]).
//!
//! | Route | What it does |
//! |---|---|
//! | `GET /api/v1/cluster/issuers` | the `ClusterIssuers` with whether each is ready, or that cert-manager is not installed |
//!
//! Everything is read as the person; RBAC on `clusterissuers` decides.

use axum::Json;
use axum::extract::State;
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_modules::Reading;
use zeabur_modules::cert_manager::{self as cm, IssuerView};

use crate::error::ApiError;
use crate::handlers::{AppState, Person};
use crate::projects::backups::ProviderState;
use crate::views::UnreadableView;

/// The cluster's `ClusterIssuers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterIssuers {
    /// Whether cert-manager is installed and its issuers could be read.
    pub state: ProviderState,
    /// The issuers, by name.
    pub issuers: Vec<IssuerView>,
    /// The issuer a new domain gets when none is chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// What could not be read.
    pub unreadable: Vec<UnreadableView>,
}

/// The `ClusterIssuers`, each with its `Ready` condition. Read as the person.
#[utoipa::path(get, path = "/api/v1/cluster/issuers", tag = "cluster",
    operation_id = "getClusterIssuers",
    responses((status = 200, body = ClusterIssuers), (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    State(state): State<AppState>,
    Person(cluster): Person,
) -> Result<Json<ClusterIssuers>, ApiError> {
    let cluster = cluster.as_ref();
    let resource = cm::CLUSTER_ISSUERS.resource();
    let read = crate::cluster::modules::read(cluster, &cm::CLUSTER_ISSUERS)
        .await
        .map_err(|e| ApiError::from(e.into_problem(&resource.key())))?;
    let (state_now, objects, unreadable) = match read {
        Reading::Objects(objects) => (ProviderState::Installed, objects, Vec::new()),
        Reading::NotInstalled => (ProviderState::NotInstalled, Vec::new(), Vec::new()),
        Reading::Refused(refusal) => (ProviderState::Refused, Vec::new(), vec![refusal]),
    };
    let mut issuers: Vec<IssuerView> = objects.iter().map(cm::issuer).collect();
    issuers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(ClusterIssuers {
        state: state_now,
        issuers,
        default: state.settings.domains.cluster_issuer.clone(),
        unreadable,
    }))
}
