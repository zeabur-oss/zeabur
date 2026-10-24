//! Runtime alerts: what Falco flagged, from the policy reports falcosidekick writes (see
//! `zeabur_modules::falco`).
//!
//! `GET /projects/{p}/alerts` reads the project namespace's reports, `GET /cluster/alerts` the
//! cluster-wide reports, which hold the alerts without a namespace (a node's own processes).
//! Both read as the person. Alerts carry command lines and file paths, so the Falco component of
//! the catalogue lets a namespace's admins read its report (a ClusterRole aggregated to `admin`)
//! and nobody else unless an admin binds that role; `list policyreports.wgpolicyk8s.io` in the
//! namespace is the permission. A refusal is `state: refused` with the rule in `unreadable`, an
//! API that is not served is `state: not-installed`; neither is an error.

use axum::Json;
use axum::extract::Path;
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::Cluster;
use zeabur_modules::Reading;
use zeabur_modules::falco::{self, Alert};

use crate::error::ApiError;
use crate::handlers::Person;
use crate::projects::backups::ProviderState;
use crate::views::UnreadableView;

/// Falco's recent alerts in one place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeAlerts {
    /// Whether the reports could be read.
    pub state: ProviderState,
    /// Newest first. A report keeps its newest alerts only (100 when Zeabur Catalog installs
    /// Falco), so older ones may be gone.
    pub items: Vec<Alert>,
    /// What could not be read.
    pub unreadable: Vec<UnreadableView>,
}

async fn read(
    cluster: &dyn Cluster,
    what: &zeabur_modules::Read,
    namespace: Option<&str>,
) -> Result<RuntimeAlerts, ApiError> {
    let reading = crate::cluster::modules::read_in(cluster, what, namespace)
        .await
        .map_err(|e| ApiError::from(e.into_problem("runtime alerts")))?;
    Ok(match reading {
        Reading::Objects(objects) => RuntimeAlerts {
            state: ProviderState::Installed,
            items: falco::alerts(&objects),
            unreadable: Vec::new(),
        },
        Reading::NotInstalled => RuntimeAlerts {
            state: ProviderState::NotInstalled,
            items: Vec::new(),
            unreadable: Vec::new(),
        },
        Reading::Refused(refusal) => RuntimeAlerts {
            state: ProviderState::Refused,
            items: Vec::new(),
            unreadable: vec![refusal],
        },
    })
}

/// Falco's recent alerts about a project's pods, newest first, read as the person.
#[utoipa::path(get, path = "/api/v1/projects/{project}/alerts", tag = "projects",
    operation_id = "getProjectAlerts",
    params(("project" = String, Path, description = "The project's namespace")),
    responses((status = 200, body = RuntimeAlerts), (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn project(
    Person(cluster): Person,
    Path(project): Path<String>,
) -> Result<Json<RuntimeAlerts>, ApiError> {
    read(cluster.as_ref(), &falco::POLICY_REPORTS, Some(&project))
        .await
        .map(Json)
}

/// Falco's recent alerts that concern no namespace (a node's own processes), newest first, read
/// as the person.
#[utoipa::path(get, path = "/api/v1/cluster/alerts", tag = "cluster",
    operation_id = "getClusterAlerts",
    responses((status = 200, body = RuntimeAlerts), (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn cluster(Person(cluster): Person) -> Result<Json<RuntimeAlerts>, ApiError> {
    read(cluster.as_ref(), &falco::CLUSTER_POLICY_REPORTS, None)
        .await
        .map(Json)
}
