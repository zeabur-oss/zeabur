//! A project's events.

#![expect(
    unused_qualifications,
    reason = "utoipa's `path` macro expands `params(EventParams)` to a qualified path"
)]

use std::cmp::Ordering;

use axum::Json;
use axum::extract::{Path, RawQuery};
use utoipa::IntoParams;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::ClusterExt as _;
use zeabur_kube::k8s_openapi::api::events::v1::Event;
use zeabur_model::{Origin, id};

use crate::error::{ApiError, list_read, read_problem};
use crate::handlers::Person;
use crate::query::{EventQuery, LimitParams};
use crate::views::{EventList, EventView};

/// The parameters of the events list (for the description; parsed by [`EventQuery`]).
#[expect(
    dead_code,
    reason = "describes the parameters in the OpenAPI document; parsed by hand"
)]
#[derive(Debug, Default, IntoParams)]
#[into_params(parameter_in = Query, rename_all = "camelCase")]
pub(crate) struct EventParams {
    /// Only events about this object, as `<kind>/<name>` (`pod/web-1`, `Deployment/web`; the
    /// kind in any case).
    regarding: Option<String>,
}

/// Events in a project, the newest `limit` of them (100 by default; see [`crate::query`]).
///
/// Ordered by when they last happened, newest first (events that say nothing about when last),
/// then by name. `totalSize` counts every event that matched, also those past the limit.
#[utoipa::path(get, path = "/api/v1/projects/{project}/events", tag = "projects",
    params(("project" = String, Path, description = "The project's namespace"), EventParams, LimitParams),
    responses((status = 200, body = EventList), (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn events(
    Person(cluster): Person,
    Path(project): Path<String>,
    RawQuery(query): RawQuery,
) -> Result<Json<EventList>, ApiError> {
    let query = EventQuery::parse(query.as_deref(), &["regarding"])?;
    let regarding = match query.filters.get("regarding") {
        None => None,
        Some(text) => {
            let (kind, name) = text
                .split_once('/')
                .filter(|(kind, name)| !kind.is_empty() && !name.is_empty())
                .ok_or_else(|| {
                    ApiError::from(Problem::new(
                        Kind::BadRequest,
                        format!("`regarding` must be `<kind>/<name>`, such as `pod/web-1`, not {text:?}."),
                    ))
                })?;
            Some((kind.to_ascii_lowercase(), name.to_owned()))
        }
    };
    let events = cluster.list::<Event>(Some(&project)).await.map_err(|e| {
        read_problem(
            e,
            &format!("events in project {project}"),
            list_read("events.k8s.io", "events", Some(&project)),
        )
    })?;
    let mut keyed: Vec<(EventKey, EventView)> = events
        .iter()
        .map(|e| (event_key(e), event_view(&project, e)))
        .filter(|(_, view)| {
            regarding.as_ref().is_none_or(|(kind, name)| {
                view.regarding
                    .as_ref()
                    .is_some_and(|r| r.kind.eq_ignore_ascii_case(kind) && r.name == *name)
            })
        })
        .collect();
    keyed.sort_by(|a, b| newest_first(&a.0, &b.0));
    let total_size = keyed.len();
    Ok(Json(EventList {
        items: keyed
            .into_iter()
            .take(query.limit)
            .map(|(_, view)| view)
            .collect(),
        total_size,
    }))
}

/// An event's place in the list: when it last happened (seconds and nanoseconds since the Unix
/// epoch) and its name.
pub(crate) type EventKey = (Option<(i64, i32)>, String);

/// Newest first, events without a time last, then by name.
pub(crate) fn newest_first(a: &EventKey, b: &EventKey) -> Ordering {
    match (&a.0, &b.0) {
        (Some(x), Some(y)) => y.cmp(x),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
    .then_with(|| a.1.cmp(&b.1))
}

pub(crate) fn event_key(event: &Event) -> EventKey {
    let time = last_seen(event).map(|t| (t.as_second(), t.subsec_nanosecond()));
    (time, event.metadata.name.clone().unwrap_or_default())
}

/// When an event last happened. Events written through the core/v1 API have no `series` or
/// `eventTime`; their times are in the deprecated fields.
pub(crate) fn last_seen(event: &Event) -> Option<zeabur_kube::k8s_openapi::jiff::Timestamp> {
    event
        .series
        .as_ref()
        .map(|s| s.last_observed_time.0)
        .or_else(|| event.deprecated_last_timestamp.as_ref().map(|t| t.0))
        .or_else(|| event.event_time.as_ref().map(|t| t.0))
}

pub(crate) fn event_view(project: &str, event: &Event) -> EventView {
    let regarding = event.regarding.as_ref().and_then(|r| {
        Some(Origin {
            field_path: r.field_path.clone(),
            ..Origin::complete(
                r.api_version.as_deref().unwrap_or("v1"),
                r.kind.as_deref()?,
                r.namespace.as_deref(),
                r.name.as_deref()?,
            )
        })
    });
    // Events written through the core/v1 API have no `series`; their count is in the
    // deprecated field.
    let count = event
        .series
        .as_ref()
        .map(|s| s.count)
        .or(event.deprecated_count)
        .unwrap_or(1);
    let name = event.metadata.name.as_deref().unwrap_or_default();
    EventView {
        id: id::event(project, name),
        origin: Origin::complete("events.k8s.io/v1", "Event", Some(project), name),
        regarding,
        r#type: event.type_.clone().unwrap_or_default(),
        reason: event.reason.clone().unwrap_or_default(),
        note: event.note.clone().unwrap_or_default(),
        count,
        last_seen: last_seen(event).map(|t| t.to_string()),
        reporter: event.reporting_controller.clone().or_else(|| {
            event
                .deprecated_source
                .as_ref()
                .and_then(|s| s.component.clone())
        }),
    }
}
