//! A service's log history: lines its pods wrote, also pods since replaced, from the cluster's
//! VictoriaLogs through Zeabur Proxy (`crates/zeabur-proxy`).
//!
//! # Who can see it
//!
//! VictoriaLogs keeps every namespace's lines and has no notion of people, so Zeabur never
//! reads it itself. Zeabur Proxy does, as the person: Zeabur hands it the person's token, the
//! proxy asks kube-apiserver whether they may `get` `logs.proxy.zeabur.com` in the project's
//! namespace, and reads that namespace's lines only. Developers and anyone with Kubernetes'
//! `view` have that right (the chart's `zeabur-proxy-logs-reader`), the same people who may read
//! the pods' logs (`pods/log`); Viewers do not.

use axum::Json;
use axum::extract::{Path, RawQuery, State};
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_modules::prometheus_api::{PrometheusRange, pod_pattern};

use crate::error::ApiError;
use crate::handlers::{AppState, Credential};
use crate::services::proxy::detail;

/// The most lines one history holds.
const MAX_LINES: u32 = 1000;

/// Whether the history could be read, as the person. An open set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum LogHistoryState {
    /// It was read.
    Available,
    /// Zeabur runs without Zeabur Proxy (the chart's `proxy.enabled`), or without a sign-in to
    /// pass on (local mode).
    NotInstalled,
    /// The person may not read the project's log history (`get` on `logs.proxy.zeabur.com`).
    Unreadable,
    /// Zeabur Proxy or VictoriaLogs did not answer, or refused the query.
    Unavailable,
}

/// One line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogHistoryLine {
    /// When it was written, RFC 3339.
    pub time: String,
    /// The pod that wrote it, which may be gone.
    pub pod: String,
    /// Its container.
    pub container: String,
    /// The line.
    pub message: String,
}

/// A service's log lines over a range, every pod's, also those replaced since.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogHistory {
    /// `projects/{project}/services/{kind}/{name}/logs/history`.
    pub id: String,
    /// Whether it could be read.
    pub state: LogHistoryState,
    /// The range.
    pub range: PrometheusRange,
    /// The lines, oldest first: the newest ones when there were more than `limit`.
    pub lines: Vec<LogHistoryLine>,
    /// The most lines returned.
    pub limit: u32,
    /// Whether there were more lines in the range than returned.
    pub truncated: bool,
    /// Zeabur Proxy's or VictoriaLogs' words when it was not read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// The parameters of a log history (for the description; parsed by hand).
#[expect(
    dead_code,
    reason = "describes the parameters in the OpenAPI document; parsed by hand"
)]
#[derive(Debug, Default, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct LogHistoryParams {
    /// `1h` (the default), `6h`, `24h` or `7d`.
    range: Option<String>,
    /// Only this container's lines.
    container: Option<String>,
    /// The most lines, 1,000 at most (the default).
    limit: Option<u32>,
}

#[derive(Debug, PartialEq, Eq)]
struct Params {
    range: PrometheusRange,
    container: Option<String>,
    limit: u32,
}

fn bad(detail: String) -> ApiError {
    ApiError::from(Problem::new(Kind::BadRequest, detail))
}

fn params(raw: Option<&str>) -> Result<Params, ApiError> {
    let mut params = Params {
        range: PrometheusRange::Hour,
        container: None,
        limit: MAX_LINES,
    };
    for (key, value) in form_urlencoded::parse(raw.unwrap_or_default().as_bytes()) {
        match key.as_ref() {
            "range" => {
                params.range = PrometheusRange::parse(&value).ok_or_else(|| {
                    bad(format!(
                        "`range` must be `1h`, `6h`, `24h` or `7d`, not {value:?}."
                    ))
                })?;
            }
            "container" => {
                if !zeabur_plan::checks::is_dns_label(&value) {
                    return Err(bad(format!("{value:?} is not a container's name.")));
                }
                params.container = Some(value.into_owned());
            }
            "limit" => {
                params.limit = value
                    .parse::<u32>()
                    .ok()
                    .filter(|limit| (1..=MAX_LINES).contains(limit))
                    .ok_or_else(|| {
                        bad(format!(
                            "`limit` must be a whole number from 1 to {MAX_LINES}, not {value:?}."
                        ))
                    })?;
            }
            _ => {}
        }
    }
    Ok(params)
}

/// The LogsQL for the lines of the pods `pods` matches (from [`pod_pattern`]), in `container`
/// when one is named. Both are checked names, so they need no escaping beyond the pattern's own.
fn logsql(pods: &str, container: Option<&str>) -> String {
    let pods = format!("kubernetes.pod_name:~\"^{pods}$\"");
    match container {
        Some(container) => format!("{pods} kubernetes.container_name:=\"{container}\""),
        None => pods,
    }
}

/// The path and query of the proxy request.
fn request(project: &str, query: &str, start: i64, limit: u32) -> String {
    let encoded = form_urlencoded::Serializer::new(String::new())
        .append_pair("namespace", project)
        .append_pair("query", query)
        .append_pair("start", &start.to_string())
        .append_pair("limit", &limit.to_string())
        .finish();
    format!("/logs/select/logsql/query?{encoded}")
}

/// The lines of VictoriaLogs' answer (one JSON object per line), oldest first. Lines that are
/// not such objects are skipped.
fn lines(body: &[u8]) -> Vec<LogHistoryLine> {
    let field = |entry: &serde_json::Value, name: &str| {
        entry
            .get(name)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let mut lines: Vec<(Timestamp, LogHistoryLine)> = body
        .split(|&b| b == b'\n')
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .filter_map(|entry| {
            let time = field(&entry, "_time");
            let at = time.parse::<Timestamp>().ok()?;
            Some((
                at,
                LogHistoryLine {
                    time,
                    pod: field(&entry, "kubernetes.pod_name"),
                    container: field(&entry, "kubernetes.container_name"),
                    message: field(&entry, "_msg"),
                },
            ))
        })
        .collect();
    lines.sort_by_key(|(at, _)| *at);
    lines.into_iter().map(|(_, line)| line).collect()
}

/// What a service's pods logged over the last hour, 6 hours, day or week, also pods replaced
/// since, as you, from the cluster's VictoriaLogs through Zeabur Proxy. Always `200` once the
/// parameters are valid: `state` says when Zeabur Proxy is not installed, you may not read the
/// project's log history (`get` on `logs.proxy.zeabur.com` in its namespace), or it did not
/// answer.
#[utoipa::path(get, path = "/api/v1/projects/{project}/services/{kind}/{name}/logs/history",
    tag = "services", operation_id = "getServiceLogHistory",
    params(("project" = String, Path, description = "The project's namespace"),
           ("kind" = String, Path, description = "`deployment`, `statefulset`, `daemonset`, `job` or `cronjob` (any case)"),
           ("name" = String, Path),
           LogHistoryParams),
    responses((status = 200, body = LogHistory),
              (status = 400, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn history(
    State(state): State<AppState>,
    Credential(handoff): Credential,
    Path((project, kind, name)): Path<(String, String, String)>,
    RawQuery(raw): RawQuery,
) -> Result<Json<LogHistory>, ApiError> {
    let params = params(raw.as_deref())?;
    if !zeabur_plan::checks::is_dns_label(&project) {
        return Err(bad(format!("{project:?} is not a project's name.")));
    }
    let pods = pod_pattern(&kind, &name).ok_or_else(|| {
        bad(format!(
            "Log history is kept for Deployments, StatefulSets, DaemonSets, Jobs and CronJobs \
             with a Kubernetes name, not {kind} {name:?}."
        ))
    })?;
    let mut history = LogHistory {
        id: format!(
            "projects/{project}/services/{}/{name}/logs/history",
            kind.to_ascii_lowercase()
        ),
        state: LogHistoryState::NotInstalled,
        range: params.range,
        lines: Vec::new(),
        limit: params.limit,
        truncated: false,
        message: None,
    };
    let (Some(proxy), Some(handoff)) = (state.zeabur_proxy.as_ref(), handoff) else {
        return Ok(Json(history));
    };
    let start = (state.clock)()
        .as_second()
        .saturating_sub(params.range.seconds());
    let path = request(
        &project,
        &logsql(&pods, params.container.as_deref()),
        start,
        params.limit,
    );
    match proxy.get(&handoff.token, path).await {
        Ok(answer) if answer.status == 200 => {
            history.state = LogHistoryState::Available;
            history.lines = lines(&answer.body);
            history.truncated =
                u32::try_from(history.lines.len()).unwrap_or(u32::MAX) >= params.limit;
        }
        Ok(answer) => {
            history.state = if answer.status == 403 {
                LogHistoryState::Unreadable
            } else {
                LogHistoryState::Unavailable
            };
            history.message = Some(detail(&answer.body));
        }
        Err(error) => {
            history.state = LogHistoryState::Unavailable;
            history.message = Some(format!("Zeabur Proxy did not answer: {error}"));
        }
    }
    Ok(Json(history))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_query_names_the_services_pods_and_the_container() {
        let pods = pod_pattern("Deployment", "web.v2").unwrap();
        assert_eq!(
            logsql(&pods, None),
            r#"kubernetes.pod_name:~"^web[.]v2-[a-z0-9]{1,10}-[a-z0-9]{5}$""#
        );
        assert_eq!(
            logsql("db-[0-9]+", Some("postgres")),
            r#"kubernetes.pod_name:~"^db-[0-9]+$" kubernetes.container_name:="postgres""#
        );
        let path = request("shop", "*", 1_700_000_000, 50);
        assert_eq!(
            path,
            "/logs/select/logsql/query?namespace=shop&query=*&start=1700000000&limit=50"
        );
    }

    #[test]
    fn parameters_are_checked() {
        assert_eq!(
            params(Some("range=24h&container=app&limit=10")).unwrap(),
            Params {
                range: PrometheusRange::Day,
                container: Some("app".to_owned()),
                limit: 10
            }
        );
        assert_eq!(params(None).unwrap().limit, MAX_LINES);
        for bad in [
            "range=2h",
            "container=a%22b",
            "container=App",
            "limit=0",
            "limit=1001",
            "limit=x",
        ] {
            assert!(params(Some(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn lines_come_oldest_first_whatever_their_precision() {
        let body = concat!(
            r#"{"_time":"2026-10-22T14:44:10.9Z","_msg":"second","kubernetes.pod_name":"web-1","kubernetes.container_name":"app"}"#,
            "\n",
            "not json\n",
            r#"{"_time":"2026-10-22T14:44:10.123456789Z","_msg":"first","kubernetes.pod_name":"web-0","kubernetes.container_name":"app"}"#,
            "\n",
        );
        let lines = lines(body.as_bytes());
        assert_eq!(
            lines.iter().map(|l| l.message.as_str()).collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert_eq!(lines[0].pod, "web-0");
        assert_eq!(
            detail(br#"{"code":"forbidden","detail":"no"}"#),
            "no".to_owned()
        );
        assert_eq!(detail(b" plain \n"), "plain".to_owned());
    }
}
