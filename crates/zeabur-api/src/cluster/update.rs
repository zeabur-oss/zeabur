//! Whether a newer Zeabur is out, for the interface's update check (docs/reference/telemetry.mdx,
//! "Update check").
//!
//! The server asks, not the browser. So a cluster makes at most one request an hour however many
//! people open it, and people who share an address do not share GitHub's limit of 60 requests an
//! hour without signing in [gh-limit]. Each request names the last answer's `ETag`, and a `304`
//! keeps that answer. When GitHub refuses, the last answer stands until GitHub's reset, or a
//! minute.
//!
//! [gh-limit]: https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api#primary-rate-limit-for-unauthenticated-users

use std::future::Future;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::State;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};

use crate::error::ApiError;
use crate::handlers::{AppState, Person};
use crate::releases::{self, Answer, RELEASES_URL};
use crate::views::{ZeaburRelease, ZeaburUpdate};

/// How long an answer is used before GitHub is asked again.
const EVERY: Duration = Duration::from_hours(1);
/// How long to wait after a failure GitHub gives no wait for.
const RETRY: Duration = Duration::from_mins(1);

/// Whether a newer release than the running Zeabur is out. While the update check is off, the
/// answer is that none is, and GitHub is not asked.
#[utoipa::path(get, path = "/api/v1/cluster/zeabur/update", tag = "cluster",
    operation_id = "getZeaburUpdate",
    responses((status = 200, body = ZeaburUpdate),
        (status = 401, body = Problem, content_type = PROBLEM),
        (status = 503, description = "GitHub has not answered since the server started", body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    State(state): State<AppState>,
    Person(_): Person,
) -> Result<Json<ZeaburUpdate>, ApiError> {
    if !state.settings.update_check {
        return Ok(Json(ZeaburUpdate { newer: None }));
    }
    let Some(http) = state.github.as_ref() else {
        return Err(unavailable("TLS could not be set up"));
    };
    let list = state
        .releases
        .get(Instant::now(), |etag| async move {
            releases::ask(http, RELEASES_URL, "zeabur", etag.as_deref(), None).await
        })
        .await
        .map_err(|e| unavailable(&e))?;
    Ok(Json(ZeaburUpdate {
        newer: releases::newer(&list, env!("CARGO_PKG_VERSION")),
    }))
}

fn unavailable(error: &str) -> ApiError {
    ApiError::from(Problem::new(Kind::Unavailable, format!("GitHub: {error}")))
}

/// The last answer, kept for every request.
#[derive(Debug, Default)]
pub struct Releases {
    kept: tokio::sync::Mutex<Kept>,
}

#[derive(Debug, Default)]
struct Kept {
    releases: Option<Vec<ZeaburRelease>>,
    etag: Option<String>,
    /// No request before this.
    next: Option<Instant>,
    /// Why the last request failed.
    error: Option<String>,
}

impl Releases {
    /// The releases. GitHub is asked with `ask` when the last answer is an hour old. One request
    /// goes at a time, and the others wait for its answer.
    pub(crate) async fn get<F, A>(&self, now: Instant, ask: F) -> Result<Vec<ZeaburRelease>, String>
    where
        F: FnOnce(Option<String>) -> A,
        A: Future<Output = Result<Answer, String>>,
    {
        let mut kept = self.kept.lock().await;
        if kept.next.is_none_or(|next| now >= next) {
            let (wait, error) = match ask(kept.etag.clone()).await {
                Ok(Answer::NotModified) if kept.releases.is_some() => (EVERY, None),
                Ok(Answer::Releases { releases, etag }) => {
                    kept.releases = Some(releases);
                    kept.etag = etag;
                    (EVERY, None)
                }
                Ok(Answer::NotModified) => (RETRY, Some("GitHub answered 304".to_owned())),
                Ok(Answer::Refused { status, wait, .. }) => (
                    wait.unwrap_or(RETRY).max(RETRY),
                    Some(format!("GitHub answered {status}")),
                ),
                Err(e) => (RETRY, Some(e)),
            };
            kept.next = Some(now + wait);
            kept.error = error;
        }
        kept.releases.clone().ok_or_else(|| {
            kept.error
                .clone()
                .unwrap_or_else(|| "no answer yet".to_owned())
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn ok(etag: &str) -> Answer {
        let release = |version: &str| ZeaburRelease {
            version: version.to_owned(),
            url: format!("https://example.test/{version}"),
            prerelease: false,
            security: false,
        };
        Answer::Releases {
            releases: vec![release("0.1.2"), release("0.1.1")],
            etag: Some(etag.to_owned()),
        }
    }

    fn refused(status: u16, wait: Option<Duration>) -> Answer {
        Answer::Refused {
            status,
            wait,
            used_up: false,
        }
    }

    /// Asks with `answer`, and says which `ETag` the request named, or `None` when none was made.
    async fn ask(
        releases: &Releases,
        now: Instant,
        answer: Result<Answer, String>,
    ) -> (Option<Option<String>>, Result<Vec<ZeaburRelease>, String>) {
        let sent = Mutex::new(None);
        let got = releases
            .get(now, |etag| {
                *sent.lock().unwrap() = Some(etag);
                async move { answer }
            })
            .await;
        (sent.into_inner().unwrap(), got)
    }

    #[tokio::test]
    async fn one_request_an_hour_then_again_with_the_etag() {
        let releases = Releases::default();
        let start = Instant::now();
        let (sent, got) = ask(&releases, start, Ok(ok("\"a\""))).await;
        assert_eq!(sent, Some(None));
        assert_eq!(got.unwrap().len(), 2);

        let (sent, got) = ask(&releases, start + Duration::from_mins(59), Ok(ok("\"b\""))).await;
        assert_eq!(sent, None, "within the hour, no request");
        assert_eq!(got.unwrap().len(), 2);

        let (sent, got) = ask(&releases, start + EVERY, Ok(Answer::NotModified)).await;
        assert_eq!(sent, Some(Some("\"a\"".to_owned())));
        assert_eq!(got.unwrap().len(), 2, "a 304 keeps the last answer");
    }

    #[tokio::test]
    async fn a_failure_keeps_the_last_answer_and_tries_again_in_a_minute() {
        let releases = Releases::default();
        let start = Instant::now();
        assert!(ask(&releases, start, Ok(ok("\"a\""))).await.1.is_ok());

        let later = start + EVERY;
        let (sent, got) = ask(&releases, later, Err("timed out".to_owned())).await;
        assert!(sent.is_some());
        assert_eq!(got.unwrap().len(), 2);

        let (sent, _) = ask(&releases, later + Duration::from_secs(30), Ok(ok("\"b\""))).await;
        assert_eq!(sent, None);
        let (sent, _) = ask(&releases, later + RETRY, Ok(ok("\"b\""))).await;
        assert!(sent.is_some());
    }

    #[tokio::test]
    async fn a_refusal_waits_for_githubs_reset() {
        let releases = Releases::default();
        let start = Instant::now();
        let (_, got) = ask(
            &releases,
            start,
            Ok(refused(403, Some(Duration::from_mins(40)))),
        )
        .await;
        assert_eq!(got.unwrap_err(), "GitHub answered 403");

        let (sent, _) = ask(&releases, start + Duration::from_mins(39), Ok(ok("\"a\""))).await;
        assert_eq!(sent, None);
        let (sent, got) = ask(&releases, start + Duration::from_mins(40), Ok(ok("\"a\""))).await;
        assert!(sent.is_some());
        assert_eq!(got.unwrap().len(), 2);
    }
}
