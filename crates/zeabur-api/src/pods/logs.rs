//! A container's log, as a snapshot or as it is written.

#![expect(
    unused_qualifications,
    reason = "utoipa's `path` macro expands `params(LogQuery)` to a qualified path"
)]

use std::convert::Infallible;

use axum::Json;
use axum::extract::{Path, RawQuery};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use futures_util::{Stream, StreamExt as _};
use utoipa::IntoParams;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::{ClusterError, LogOptions, unavailable_log};
use zeabur_model::Origin;

use crate::error::{ApiError, get_read, read_problem};
use crate::handlers::Person;
use crate::views::LogSnapshot;

/// Which part of a container's log to read.
///
/// Parsed by hand from the query string: axum's `Query` extractor would bring
/// `serde_urlencoded` and its float formatter into the binary for five parameters.
#[derive(Debug, Default, IntoParams)]
#[into_params(parameter_in = Query, rename_all = "camelCase")]
pub(crate) struct LogQuery {
    /// The container; required when the pod has several.
    container: Option<String>,
    /// Only the last lines (default 500).
    tail_lines: Option<i64>,
    /// Only lines newer than this many seconds.
    since_seconds: Option<i64>,
    /// The previous instance of the container, after a restart.
    #[param(required = false)]
    previous: bool,
    /// Prefix each line with its timestamp.
    #[param(required = false)]
    timestamps: bool,
}

impl LogQuery {
    /// Reads the parameters; unknown ones are ignored, malformed numbers are a bad request.
    fn parse(query: Option<&str>) -> Result<Self, ApiError> {
        let mut parsed = Self::default();
        let number = |name: &str, value: &str| {
            value.parse::<i64>().map_err(|_parse_error| {
                ApiError::from(Problem::new(
                    Kind::BadRequest,
                    format!("`{name}` must be a whole number, not {value:?}."),
                ))
            })
        };
        for (name, value) in form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
            match name.as_ref() {
                "container" => parsed.container = Some(value.into_owned()),
                "tailLines" => parsed.tail_lines = Some(number("tailLines", &value)?),
                "sinceSeconds" => parsed.since_seconds = Some(number("sinceSeconds", &value)?),
                "previous" => parsed.previous = value == "true",
                "timestamps" => parsed.timestamps = value == "true",
                _ => {}
            }
        }
        Ok(parsed)
    }

    fn options(&self) -> LogOptions {
        LogOptions {
            container: self.container.clone(),
            tail_lines: Some(self.tail_lines.unwrap_or(500)),
            since_seconds: self.since_seconds,
            previous: self.previous,
            timestamps: self.timestamps,
        }
    }

    fn origin(&self, project: &str, pod: &str) -> Origin {
        Origin {
            field_path: self
                .container
                .as_ref()
                .map(|c| format!("spec.containers{{{c}}}")),
            ..Origin::complete("v1", "Pod", Some(project), pod)
        }
    }
}

/// The problem for a log that could not be read: the kubelet's refusal ([`unavailable_log`]) is
/// a `log-unavailable` problem, anything else is told as for any read.
fn log_problem(error: ClusterError, what: &str, project: &str, pod: &str) -> ApiError {
    let reason = match &error {
        ClusterError::Status(status) if status.code == 400 => unavailable_log(&status.message),
        ClusterError::Status(_)
        | ClusterError::Transport(_)
        | ClusterError::Decode(_)
        | ClusterError::InvalidName(_) => None,
    };
    match (reason, error) {
        (Some(reason), ClusterError::Status(status)) => {
            Problem::log_unavailable(reason, status, what).into()
        }
        (_, error) => read_problem(error, what, get_read("", "pods/log", Some(project), pod)),
    }
}

/// A snapshot of a container's log. A log the kubelet cannot give (the container has not
/// started, has not restarted, or is gone) is a `log-unavailable` problem, never a log line.
#[utoipa::path(get, path = "/api/v1/projects/{project}/pods/{pod}/logs", tag = "pods",
    params(("project" = String, Path), ("pod" = String, Path), LogQuery),
    responses((status = 200, body = LogSnapshot), (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, description = "The pod does not exist, or the log asked for cannot be read (`log-unavailable`)", body = Problem, content_type = PROBLEM)))]
pub(crate) async fn logs(
    Person(cluster): Person,
    Path((project, pod)): Path<(String, String)>,
    RawQuery(query): RawQuery,
) -> Result<Json<LogSnapshot>, ApiError> {
    let query = LogQuery::parse(query.as_deref())?;
    let what = format!("the log of pod {pod}");
    let text = cluster
        .pod_logs(&project, &pod, &query.options())
        .await
        .map_err(|e| log_problem(e, &what, &project, &pod))?;
    Ok(Json(LogSnapshot {
        origin: query.origin(&project, &pod),
        lines: text.lines().map(str::to_owned).collect(),
    }))
}

/// A container's log as it is written, as Server-Sent Events: one `data` line per log line; an
/// `error` event with a problem (JSON) when the stream fails, after which it ends.
#[utoipa::path(get, path = "/api/v1/projects/{project}/pods/{pod}/logs/follow", tag = "pods",
    params(("project" = String, Path), ("pod" = String, Path), LogQuery),
    responses((status = 200, content_type = "text/event-stream", body = String),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, description = "The pod does not exist, or the log asked for cannot be read (`log-unavailable`)", body = Problem, content_type = PROBLEM)))]
pub(crate) async fn follow_logs(
    Person(cluster): Person,
    Path((project, pod)): Path<(String, String)>,
    RawQuery(query): RawQuery,
) -> Result<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>, ApiError> {
    let query = LogQuery::parse(query.as_deref())?;
    let what = format!("the log of pod {pod}");
    let lines = cluster
        .follow_pod_logs(&project, &pod, &query.options())
        .await
        .map_err(|e| log_problem(e, &what, &project, &pod))?;
    let events = lines.scan(false, move |failed, line| {
        if *failed {
            return std::future::ready(None);
        }
        let event = match line {
            Ok(line) => SseEvent::default().data(line),
            Err(error) => {
                *failed = true;
                let problem = error.into_problem(&what);
                SseEvent::default()
                    .event("error")
                    .data(serde_json::to_string(&problem).unwrap_or_default())
            }
        };
        std::future::ready(Some(Ok(event)))
    });
    Ok(Sse::new(events).keep_alive(KeepAlive::default()))
}
