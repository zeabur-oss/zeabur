//! What a project's pods use now: one read of metrics-server's `PodMetrics` for the namespace,
//! which the project's Overview and each service page cut per pod.
//!
//! The person needs `list` on `pods.metrics.k8s.io` in the namespace. Kubernetes' aggregated
//! role `system:aggregated-metrics-reader`, which metrics-server installs, adds it to `view`,
//! `edit` and `admin` [ms-rbac], so a person who can see a project's pods can usually see their
//! usage. A pod metrics-server has no sample for is not in the list (see
//! `zeabur_modules::metrics_server`).
//!
//! [ms-rbac]: https://github.com/kubernetes-sigs/metrics-server/blob/master/manifests/base/rbac.yaml

use axum::Json;
use axum::extract::Path;
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_model::Origin;
use zeabur_modules::Reading;

use crate::cluster::modules;
use crate::error::ApiError;
use crate::handlers::Person;
use crate::views::{MetricsState, UnreadableView};

/// What one container uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContainerUsage {
    /// The container.
    pub name: String,
    /// CPU, in millicores, rounded up.
    pub cpu_millicores: u64,
    /// Memory, the working set, in bytes.
    pub memory_bytes: u64,
}

/// What one pod uses, the sum of its containers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PodUsage {
    /// Its `PodMetrics`.
    pub origin: Origin,
    /// The pod.
    pub pod: String,
    /// CPU, in millicores.
    pub cpu_millicores: u64,
    /// Memory, the working set, in bytes.
    pub memory_bytes: u64,
    /// Its containers.
    pub containers: Vec<ContainerUsage>,
}

/// What a project's pods use now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUsage {
    /// `projects/{project}/usage`.
    pub id: String,
    /// Whether usage could be read.
    pub state: MetricsState,
    /// When the newest sample was taken (RFC 3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// The window CPU is averaged over, as metrics-server writes it (`15s`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<String>,
    /// Every pod with a sample, by name. A running pod missing here has none yet.
    pub pods: Vec<PodUsage>,
    /// What could not be read.
    pub unreadable: Vec<UnreadableView>,
}

/// What each pod of a project uses now, from metrics-server: CPU in millicores and memory (the
/// working set) in bytes, per pod and container. Approximate ("meant only for autoscaling
/// purposes"). Always `200`: `state` says when metrics-server is not installed, the person may
/// not read it (`unreadable` names the refusal), or it did not answer.
#[utoipa::path(get, path = "/api/v1/projects/{project}/usage", tag = "projects",
    operation_id = "getProjectUsage",
    params(("project" = String, Path, description = "The project's namespace")),
    responses((status = 200, body = ProjectUsage),
              (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    Person(cluster): Person,
    Path(project): Path<String>,
) -> Result<Json<ProjectUsage>, ApiError> {
    let read = modules::read_in(
        cluster.as_ref(),
        &zeabur_modules::metrics_server::POD_METRICS,
        Some(&project),
    )
    .await;
    let mut usage = ProjectUsage {
        id: format!("projects/{project}/usage"),
        state: MetricsState::Available,
        timestamp: None,
        window: None,
        pods: Vec::new(),
        unreadable: Vec::new(),
    };
    match read {
        Ok(Reading::Objects(objects)) => {
            let pods = zeabur_modules::metrics_server::pod_usage(&objects);
            if let Some(newest) = pods.iter().max_by(|a, b| a.timestamp.cmp(&b.timestamp)) {
                usage.timestamp.clone_from(&newest.timestamp);
                usage.window.clone_from(&newest.window);
            }
            usage.pods = pods.into_iter().map(|pod| view(&project, pod)).collect();
        }
        Ok(Reading::NotInstalled) => usage.state = MetricsState::NotInstalled,
        Ok(Reading::Refused(refusal)) => {
            usage.state = MetricsState::Unreadable;
            usage.unreadable.push(refusal);
        }
        Err(_) => usage.state = MetricsState::Unavailable,
    }
    Ok(Json(usage))
}

fn view(project: &str, pod: zeabur_modules::metrics_server::PodUsage) -> PodUsage {
    let containers: Vec<ContainerUsage> = pod
        .containers
        .into_iter()
        .map(|c| ContainerUsage {
            cpu_millicores: zeabur_core::quantity::millicores(&c.cpu).unwrap_or(0),
            memory_bytes: zeabur_core::quantity::bytes(&c.memory).unwrap_or(0),
            name: c.name,
        })
        .collect();
    PodUsage {
        origin: Origin::complete(
            "metrics.k8s.io/v1beta1",
            "PodMetrics",
            Some(project),
            &pod.pod,
        ),
        cpu_millicores: containers
            .iter()
            .fold(0, |sum, c| sum.saturating_add(c.cpu_millicores)),
        memory_bytes: containers
            .iter()
            .fold(0, |sum, c| sum.saturating_add(c.memory_bytes)),
        pod: pod.pod,
        containers,
    }
}
