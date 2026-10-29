//! What a service used over time: CPU, memory and restarts of its pods, from the metrics store
//! (see `zeabur_modules::prometheus_api` for the queries, `crate::settings` for the store).
//!
//! # Who can see it
//!
//! The store has no notion of projects: whoever may reach it may ask it anything, about every
//! namespace. So it is read one of two ways.
//!
//! - Through Zeabur Proxy (`crates/zeabur-proxy`), when Zeabur has one and the store in use is the
//!   default one, the store the proxy reads. Zeabur hands it the person's token; the proxy asks
//!   kube-apiserver whether they may `get` `metrics.proxy.zeabur.com` in the project's namespace
//!   and holds every query to that namespace (VictoriaMetrics' `extra_label`). Viewers, developers
//!   and anyone with Kubernetes' `view` have that right (the chart's
//!   `zeabur-proxy-metrics-reader`), the same people who may read the pods' current usage.
//! - Otherwise, through kube-apiserver's service proxy as the person, so the right that decides is
//!   `get` on `services/proxy` for the store's Service. kube-apiserver does not tell the store who
//!   is asking (it removes `Authorization` and the impersonation headers before proxying)
//!   [k8s-authn-filter], so being allowed this is being allowed every project's metrics, and only
//!   people who may see the whole cluster should be. Kubernetes' `edit` and `admin` roles include
//!   `services/proxy` only in their own namespace, and `view` not at all [k8s-bootstrap-policy], so
//!   a tenant does not get it unless someone grants it.
//!
//! [k8s-authn-filter]: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/apiserver/pkg/endpoints/filters/authentication.go
//! [k8s-bootstrap-policy]: https://github.com/kubernetes/kubernetes/blob/master/plugin/pkg/auth/authorizer/rbac/bootstrappolicy/policy.go

use axum::Json;
use axum::extract::{Path, RawQuery, State};
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, KubernetesStatus, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::{Cluster, ServicePort, UserToken};
use zeabur_modules::prometheus_api::{
    self, PrometheusRange, PrometheusRestart, PrometheusSeries, QUERY_RANGE, Query,
};

use crate::error::ApiError;
use crate::handlers::{AppState, Credential, Person};
use crate::services::proxy::{ZeaburProxy, detail};
use crate::settings::{
    Proxied, SettingSource, StoreState, metrics_store, proxy_get, proxy_refusal,
};
use crate::views::UnreadableView;

/// The store a history came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HistorySource {
    /// Its Service port, `namespace/name:port`.
    pub service: String,
    /// How it was read, which decides who may read it.
    pub via: HistoryVia,
}

/// How the store was read. An open set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum HistoryVia {
    /// Through Zeabur Proxy, held to the project's namespace: project members may read it.
    ZeaburProxy,
    /// Through kube-apiserver's service proxy: whoever may read it may read every project's
    /// metrics.
    ServiceProxy,
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

/// Where the history is read from.
#[derive(Clone, Copy)]
enum Source<'a> {
    /// The store's Service through kube-apiserver's service proxy.
    Store(&'a dyn Cluster, &'a ServicePort),
    /// Zeabur Proxy, holding every query to the project's namespace.
    Proxy(&'a dyn ZeaburProxy, &'a UserToken, &'a str),
}

/// Why a read gave no values.
enum Missed {
    /// Zeabur Proxy serves no metrics store: read the store directly.
    NoProxyBackend,
    /// Zeabur Proxy could not reach the store, which may not be installed yet; its words.
    ProxyBackendDown(String),
    /// The state it means, the right that is missing, and the words.
    Failed(StoreState, Option<UnreadableView>, String),
}

/// The right Zeabur Proxy checks, named for the interface.
fn proxy_refused(project: &str, message: String) -> UnreadableView {
    UnreadableView {
        resource: format!("metrics.proxy.zeabur.com ({project})"),
        kubernetes: KubernetesStatus::new(403, "Forbidden", message),
    }
}

/// `GET path_and_query` of the store's Prometheus API, as the person.
async fn fetch(source: Source<'_>, path: &str, encoded: &str) -> Result<Value, Missed> {
    match source {
        Source::Store(cluster, service) => {
            match proxy_get(cluster, service, &format!("{path}?{encoded}")).await {
                Proxied::Answer(answer) => Ok(answer),
                Proxied::Failed {
                    state,
                    status,
                    message,
                } => {
                    let refusal = (state == StoreState::Unreadable)
                        .then_some(status)
                        .flatten()
                        .map(|status| proxy_refusal(service, status));
                    Err(Missed::Failed(state, refusal, message))
                }
            }
        }
        Source::Proxy(proxy, token, project) => {
            let namespace = form_urlencoded::Serializer::new(String::new())
                .append_pair("namespace", project)
                .finish();
            let answer = proxy
                .get(token, format!("/metrics{path}?{namespace}&{encoded}"))
                .await
                .map_err(|error| {
                    Missed::Failed(
                        StoreState::Unavailable,
                        None,
                        format!("Zeabur Proxy did not answer: {error}"),
                    )
                })?;
            match answer.status {
                200 => serde_json::from_slice(&answer.body).map_err(|error| {
                    Missed::Failed(StoreState::Unavailable, None, error.to_string())
                }),
                404 => Err(Missed::NoProxyBackend),
                // Every point would read samples from before the project's namespace.
                400 if code(&answer.body).as_deref() == Some("before_namespace") => Ok(
                    serde_json::json!({"status": "success", "data": {"resultType": "matrix", "result": []}}),
                ),
                502 => Err(Missed::ProxyBackendDown(detail(&answer.body))),
                403 => {
                    let message = detail(&answer.body);
                    Err(Missed::Failed(
                        StoreState::Unreadable,
                        Some(proxy_refused(project, message.clone())),
                        message,
                    ))
                }
                _ => Err(Missed::Failed(
                    StoreState::Unavailable,
                    None,
                    detail(&answer.body),
                )),
            }
        }
    }
}

/// The `code` of Zeabur Proxy's problem.
fn code(body: &[u8]) -> Option<String> {
    serde_json::from_slice::<Value>(body)
        .ok()?
        .get("code")?
        .as_str()
        .map(str::to_owned)
}

/// One range query; the values, or why there are none.
async fn query(
    source: Source<'_>,
    query: &Query,
    range: PrometheusRange,
    times: &[i64],
) -> Result<Vec<PrometheusSeries>, Missed> {
    let (Some(&start), Some(&end)) = (times.first(), times.last()) else {
        return Ok(Vec::new());
    };
    let encoded = form_urlencoded::Serializer::new(String::new())
        .append_pair("query", &query.promql)
        .append_pair("start", &start.to_string())
        .append_pair("end", &end.to_string())
        .append_pair("step", &range.step().to_string())
        .finish();
    let answer = fetch(source, QUERY_RANGE, &encoded).await?;
    prometheus_api::series(&answer, times, query.scale)
        .map_err(|error| Missed::Failed(StoreState::Unavailable, None, error.message))
}

/// CPU, memory and restarts (`None` when the store keeps no restart counts).
type Readings = (
    Vec<PrometheusSeries>,
    Vec<PrometheusSeries>,
    Option<Vec<PrometheusRestart>>,
);

async fn read(
    source: Source<'_>,
    queries: &prometheus_api::Queries,
    range: PrometheusRange,
    times: &[i64],
) -> Result<Readings, Missed> {
    let (cpu, memory, restarts) = futures_util::join!(
        query(source, &queries.cpu, range, times),
        query(source, &queries.memory, range, times),
        query(source, &queries.restarts, range, times),
    );
    let (cpu, memory) = (cpu?, memory?);
    // No restart counts at all: the store has no kube-state-metrics, so restarts are unknown.
    let restarts = match restarts {
        Ok(counts) if !counts.is_empty() => {
            let oom_killed = if counts
                .iter()
                .flat_map(|s| &s.values)
                .flatten()
                .any(|&v| v >= 0.5)
            {
                query(source, &queries.oom_killed, range, times)
                    .await
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            Some(prometheus_api::restarts(&counts, &oom_killed, times))
        }
        Ok(_) | Err(_) => None,
    };
    Ok((cpu, memory, restarts))
}

/// What a service's pods used over the last hour, 6 hours, day or week, from the metrics store,
/// as you: CPU (millicores) and memory (the working set, bytes) per pod, and restarts when the
/// store has kube-state-metrics. Pods are the service's by name, so those a rollout replaced
/// are included. Always `200` once the parameters are valid: `state` says when there is no
/// store, you may not read it (`unreadable` names the right), or it did not answer. With Zeabur
/// Proxy and the default store, the right is `get` on `metrics.proxy.zeabur.com` in the project,
/// which project members have; otherwise it is `get` on the store's `services/proxy`, which is
/// reading every project's metrics: see the description of `GET /api/v1/settings/metrics-store`.
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
    Credential(handoff): Credential,
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
            via: HistoryVia::ServiceProxy,
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
    let direct = Source::Store(cluster.as_ref(), &store.service);
    let through_proxy = match (&state.zeabur_proxy, &handoff, store.source) {
        (Some(proxy), Some(handoff), SettingSource::Default) => Some(Source::Proxy(
            proxy.as_ref(),
            &handoff.token,
            project.as_str(),
        )),
        _ => None,
    };
    let readings = match through_proxy {
        Some(source) => match read(source, &queries, range, &times).await {
            Err(Missed::NoProxyBackend) => read(direct, &queries, range, &times).await,
            // Read directly whoever may: an administrator then sees that there is no store.
            Err(Missed::ProxyBackendDown(message)) => {
                match read(direct, &queries, range, &times).await {
                    Err(Missed::Failed(StoreState::Unreadable, ..)) => {
                        history.source.via = HistoryVia::ZeaburProxy;
                        Err(Missed::Failed(StoreState::Unavailable, None, message))
                    }
                    other => other,
                }
            }
            other => {
                history.source.via = HistoryVia::ZeaburProxy;
                other
            }
        },
        None => read(direct, &queries, range, &times).await,
    };
    let (cpu, memory, restarts) = match readings {
        Ok(readings) => readings,
        Err(Missed::Failed(state, refusal, message)) => {
            history.state = state;
            history.message = Some(message);
            history.unreadable.extend(refusal);
            return Ok(Json(history));
        }
        Err(Missed::NoProxyBackend | Missed::ProxyBackendDown(_)) => {
            history.state = StoreState::Unavailable;
            history.message = Some("Zeabur Proxy did not reach the metrics store.".to_owned());
            return Ok(Json(history));
        }
    };
    history.restarts = restarts;
    history.times = times;
    history.cpu = cpu;
    history.memory = memory;
    Ok(Json(history))
}
