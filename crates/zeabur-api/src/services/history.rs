//! What a service used over time: CPU, memory and restarts of its pods, from the metrics store
//! (see `zeabur_modules::prometheus_api` for the queries, `crate::settings` for the store).
//!
//! # Who can see it
//!
//! The store is read through kube-apiserver's service proxy as the person, so the right that
//! decides is `get` on `services/proxy` for the store's Service. kube-apiserver does not tell the
//! store who is asking (it removes `Authorization` and the impersonation headers before
//! proxying) [k8s-authn-filter], and the store has no notion of projects: whoever may reach it
//! may ask it anything, about every namespace. Zeabur's queries name only this project's pods,
//! but that limits what Zeabur shows, not what the person could read. So the rule is: being
//! allowed history is being allowed every project's metrics, and only people who may see the
//! whole cluster should be. Kubernetes' `edit` and `admin` roles include `services/proxy` only in
//! their own namespace, and `view` not at all [k8s-bootstrap-policy], so a tenant does not get it
//! unless someone grants it.
//!
//! [k8s-authn-filter]: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/apiserver/pkg/endpoints/filters/authentication.go
//! [k8s-bootstrap-policy]: https://github.com/kubernetes/kubernetes/blob/master/plugin/pkg/auth/authorizer/rbac/bootstrappolicy/policy.go

use axum::Json;
use axum::extract::{Path, RawQuery, State};
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::Cluster;
use zeabur_modules::prometheus_api::{
    self, PrometheusRange, PrometheusRestart, PrometheusSeries, QUERY_RANGE, Query,
};

use crate::error::ApiError;
use crate::handlers::{AppState, Person};
use crate::settings::{Proxied, StoreState, metrics_store, proxy_get, proxy_refusal};
use crate::views::UnreadableView;

/// The store a history came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HistorySource {
    /// Its Service port, `namespace/name:port`.
    pub service: String,
}

/// A service's CPU, memory and restarts over a range, one value per pod and time.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceHistory {
    /// `projects/{project}/services/{kind}/{name}/history`.
    pub id: String,
    /// Whether the store answered.
    pub state: StoreState,
    /// The store.
    pub source: HistorySource,
    /// The range.
    pub range: PrometheusRange,
    /// Seconds between two times.
    pub step: i64,
    /// The times of the values (Unix seconds), oldest first.
    pub times: Vec<i64>,
    /// CPU per pod, in millicores; every pod the store knows in the range, also those replaced
    /// since.
    pub cpu: Vec<PrometheusSeries>,
    /// Memory per pod, the working set's highest in each step, in bytes.
    pub memory: Vec<PrometheusSeries>,
    /// Restarts, oldest first; absent when the store has no restart counts (it does not scrape
    /// kube-state-metrics), which is not the same as none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restarts: Option<Vec<PrometheusRestart>>,
    /// kube-apiserver's or the store's words when it did not answer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// The settings ConfigMap or the store, when the person may not read it.
    pub unreadable: Vec<UnreadableView>,
}

/// The parameters of a history (for the description; parsed by hand).
#[expect(
    dead_code,
    reason = "describes the parameter in the OpenAPI document; parsed by hand"
)]
#[derive(Debug, Default, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct HistoryParams {
    /// `1h` (the default, a point every 15 s), `6h` (1 min), `24h` (5 min) or `7d` (30 min).
    range: Option<String>,
}

/// The `range` of a query string; `1h` when it names none.
fn range(raw: Option<&str>) -> Result<PrometheusRange, ApiError> {
    match form_urlencoded::parse(raw.unwrap_or_default().as_bytes()).find(|(key, _)| key == "range")
    {
        None => Ok(PrometheusRange::default()),
        Some((_, text)) => PrometheusRange::parse(&text).ok_or_else(|| {
            ApiError::from(Problem::new(
                Kind::BadRequest,
                format!("`range` must be `1h`, `6h`, `24h` or `7d`, not {text:?}."),
            ))
        }),
    }
}

/// `history` as the store's failure leaves it: its state, its words, and a refusal named.
fn failed(history: &mut ServiceHistory, service: &zeabur_kube::ServicePort, failed: Proxied) {
    if let Proxied::Failed {
        state,
        status,
        message,
    } = failed
    {
        history.state = state;
        history.message = Some(message);
        if state == StoreState::Unreadable {
            history
                .unreadable
                .extend(status.map(|status| proxy_refusal(service, status)));
        }
    }
}

/// One range query, as the person; the values, or how it failed.
async fn query(
    cluster: &dyn Cluster,
    service: &zeabur_kube::ServicePort,
    query: &Query,
    range: PrometheusRange,
    times: &[i64],
) -> Result<Vec<PrometheusSeries>, Proxied> {
    let (Some(&start), Some(&end)) = (times.first(), times.last()) else {
        return Ok(Vec::new());
    };
    let encoded = form_urlencoded::Serializer::new(String::new())
        .append_pair("query", &query.promql)
        .append_pair("start", &start.to_string())
        .append_pair("end", &end.to_string())
        .append_pair("step", &range.step().to_string())
        .finish();
    match proxy_get(cluster, service, &format!("{QUERY_RANGE}?{encoded}")).await {
        Proxied::Answer(answer) => {
            prometheus_api::series(&answer, times, query.scale).map_err(|error| Proxied::Failed {
                state: StoreState::Unavailable,
                status: None,
                message: error.message,
            })
        }
        failed @ Proxied::Failed { .. } => Err(failed),
    }
}

/// What a service's pods used over the last hour, 6 hours, day or week, from the metrics store,
/// as you: CPU (millicores) and memory (the working set, bytes) per pod, and restarts when the
/// store has kube-state-metrics. Pods are the service's by name, so those a rollout replaced
/// are included. Always `200` once the parameters are valid: `state` says when there is no
/// store, you may not read it (`get` on its `services/proxy`; `unreadable` names it), or it did
/// not answer. Reading the store is reading every project's metrics: see the description of
/// `GET /api/v1/settings/metrics-store`.
#[utoipa::path(get, path = "/api/v1/projects/{project}/services/{kind}/{name}/history",
    tag = "services", operation_id = "getServiceHistory",
    params(("project" = String, Path, description = "The project's namespace"),
           ("kind" = String, Path, description = "`deployment`, `statefulset`, `daemonset`, `job` or `cronjob` (any case)"),
           ("name" = String, Path),
           HistoryParams),
    responses((status = 200, body = ServiceHistory),
              (status = 400, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path((project, kind, name)): Path<(String, String, String)>,
    RawQuery(raw): RawQuery,
) -> Result<Json<ServiceHistory>, ApiError> {
    let range = range(raw.as_deref())?;
    if !zeabur_plan::checks::is_dns_label(&project) {
        return Err(ApiError::from(Problem::new(
            Kind::BadRequest,
            format!("{project:?} is not a project's name."),
        )));
    }
    let pods = prometheus_api::pod_pattern(&kind, &name).ok_or_else(|| {
        ApiError::from(Problem::new(
            Kind::BadRequest,
            format!(
                "History is kept for Deployments, StatefulSets, DaemonSets, Jobs and CronJobs \
                 with a Kubernetes name, not {kind} {name:?}."
            ),
        ))
    })?;
    let store = metrics_store(cluster.as_ref(), &state.settings).await;
    let times = range.times((state.clock)().as_second());
    let queries = prometheus_api::queries(&project, &pods, range);
    let mut history = ServiceHistory {
        id: format!(
            "projects/{project}/services/{}/{name}/history",
            kind.to_ascii_lowercase()
        ),
        state: StoreState::Available,
        source: HistorySource {
            service: store.service.to_string(),
        },
        range,
        step: range.step(),
        times: Vec::new(),
        cpu: Vec::new(),
        memory: Vec::new(),
        restarts: None,
        message: None,
        unreadable: store.unreadable.clone().into_iter().collect(),
    };
    let service = &store.service;
    let (cpu, memory, restarts) = futures_util::join!(
        query(cluster.as_ref(), service, &queries.cpu, range, &times),
        query(cluster.as_ref(), service, &queries.memory, range, &times),
        query(cluster.as_ref(), service, &queries.restarts, range, &times),
    );
    let (cpu, memory) = match (cpu, memory) {
        (Ok(cpu), Ok(memory)) => (cpu, memory),
        (Err(failure), _) | (_, Err(failure)) => {
            failed(&mut history, service, failure);
            return Ok(Json(history));
        }
    };
    // No restart counts at all: the store has no kube-state-metrics, so restarts are unknown.
    history.restarts = match restarts {
        Ok(counts) if !counts.is_empty() => {
            let oom_killed = if counts
                .iter()
                .flat_map(|s| &s.values)
                .flatten()
                .any(|&v| v >= 0.5)
            {
                query(
                    cluster.as_ref(),
                    service,
                    &queries.oom_killed,
                    range,
                    &times,
                )
                .await
                .unwrap_or_default()
            } else {
                Vec::new()
            };
            Some(prometheus_api::restarts(&counts, &oom_killed, &times))
        }
        Ok(_) | Err(_) => None,
    };
    history.times = times;
    history.cpu = cpu;
    history.memory = memory;
    Ok(Json(history))
}
