//! Zeabur's releases on GitHub, for the interface's update check (docs/reference/telemetry.mdx,
//! "Update check").
//!
//! The server asks, not the browser, so a cluster makes at most one request an hour however many
//! people open it, and people who share an address do not share GitHub's limit of 60 requests an
//! hour without signing in [gh-limit]. Each request names the last answer's `ETag`; a `304` keeps
//! that answer. When GitHub refuses, the last answer stands until GitHub's reset, or a minute.
//!
//! [gh-limit]: https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api#primary-rate-limit-for-unauthenticated-users

use std::future::Future;
use std::time::{Duration, Instant, SystemTime};

use axum::Json;
use axum::extract::State;
use axum::http::{self, HeaderMap, Request};
use serde::Deserialize;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};

use crate::error::ApiError;
use crate::handlers::{AppState, Person};
use crate::images::https::Https;
use crate::views::{ZeaburRelease, ZeaburReleaseList};

pub(crate) const RELEASES_URL: &str = "https://api.github.com/repos/zeabur-oss/zeabur/releases";
/// How long an answer is used before GitHub is asked again.
const EVERY: Duration = Duration::from_hours(1);
/// How long to wait after a failure GitHub gives no wait for.
const RETRY: Duration = Duration::from_mins(1);
/// The most a list of releases may be.
const LIMIT: usize = 8 * 1024 * 1024;

/// Zeabur's published releases, newest first. An empty list while the update check is off, which
/// asks GitHub nothing.
#[utoipa::path(get, path = "/api/v1/cluster/zeabur/releases", tag = "cluster",
    operation_id = "getZeaburReleases",
    responses((status = 200, body = ZeaburReleaseList),
        (status = 401, body = Problem, content_type = PROBLEM),
        (status = 503, description = "GitHub has not answered since the server started", body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    State(state): State<AppState>,
    Person(_): Person,
) -> Result<Json<ZeaburReleaseList>, ApiError> {
    if !state.settings.update_check {
        return Ok(Json(ZeaburReleaseList { items: Vec::new() }));
    }
    let Some(http) = state.github.as_ref() else {
        return Err(unavailable("TLS could not be set up"));
    };
    let items = state
        .releases
        .get(Instant::now(), |etag| ask(http, etag))
        .await
        .map_err(|e| unavailable(&e))?;
    Ok(Json(ZeaburReleaseList { items }))
}

fn unavailable(error: &str) -> ApiError {
    ApiError::from(Problem::new(Kind::Unavailable, format!("GitHub: {error}")))
}

/// GitHub's answer to one request.
#[derive(Debug, Default)]
pub(crate) struct Answer {
    pub status: u16,
    pub etag: Option<String>,
    /// How long GitHub asks to be left alone.
    pub wait: Option<Duration>,
    pub body: Vec<u8>,
}

async fn ask(http: &Https, etag: Option<String>) -> Result<Answer, String> {
    let mut request = Request::get(RELEASES_URL)
        .header(http::header::USER_AGENT, "zeabur")
        .header(http::header::ACCEPT, "application/vnd.github+json");
    if let Some(etag) = etag {
        request = request.header(http::header::IF_NONE_MATCH, etag);
    }
    let request = request
        .body(http_body_util::Full::default())
        .map_err(|e| e.to_string())?;
    let (status, headers, body) = http.exchange_with_headers(request, LIMIT).await?;
    Ok(Answer {
        status,
        etag: text(&headers, http::header::ETAG.as_str()).map(str::to_owned),
        wait: wait(&headers, SystemTime::now()),
        body,
    })
}

fn text<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

/// `Retry-After` in seconds, or the time to `X-RateLimit-Reset` once none are left
/// [gh-headers].
///
/// [gh-headers]: https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api#checking-the-status-of-your-rate-limit
fn wait(headers: &HeaderMap, now: SystemTime) -> Option<Duration> {
    if let Some(seconds) = text(headers, "retry-after").and_then(|v| v.parse().ok()) {
        return Some(Duration::from_secs(seconds));
    }
    if text(headers, "x-ratelimit-remaining") != Some("0") {
        return None;
    }
    let reset = text(headers, "x-ratelimit-reset")?.parse().ok()?;
    let reset = SystemTime::UNIX_EPOCH + Duration::from_secs(reset);
    Some(reset.duration_since(now).unwrap_or_default())
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
    /// The releases, asking GitHub with `ask` when the last answer is an hour old. One request
    /// at a time: the others wait for its answer.
    pub(crate) async fn get<F, A>(&self, now: Instant, ask: F) -> Result<Vec<ZeaburRelease>, String>
    where
        F: FnOnce(Option<String>) -> A,
        A: Future<Output = Result<Answer, String>>,
    {
        let mut kept = self.kept.lock().await;
        if kept.next.is_none_or(|next| now >= next) {
            let wait = match ask(kept.etag.clone()).await {
                Ok(answer) if answer.status == 304 && kept.releases.is_some() => Ok(EVERY),
                Ok(answer) if answer.status == 200 => match published(&answer.body) {
                    Ok(releases) => {
                        kept.releases = Some(releases);
                        kept.etag = answer.etag;
                        Ok(EVERY)
                    }
                    Err(e) => Err((RETRY, e)),
                },
                Ok(answer) => Err((
                    answer.wait.unwrap_or(RETRY).max(RETRY),
                    format!("GitHub answered {}", answer.status),
                )),
                Err(e) => Err((RETRY, e)),
            };
            let (wait, error) = match wait {
                Ok(wait) => (wait, None),
                Err((wait, error)) => (wait, Some(error)),
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

/// The published releases in GitHub's list, with the tag's `v` dropped.
fn published(body: &[u8]) -> Result<Vec<ZeaburRelease>, String> {
    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
        html_url: String,
        #[serde(default)]
        draft: bool,
        #[serde(default)]
        prerelease: bool,
        #[serde(default)]
        body: Option<String>,
    }
    let list: Vec<Release> = serde_json::from_slice(body)
        .map_err(|e| format!("GitHub's answer is not expected: {e}"))?;
    Ok(list
        .into_iter()
        .filter(|r| !r.draft)
        .map(|r| ZeaburRelease {
            version: r
                .tag_name
                .strip_prefix('v')
                .unwrap_or(&r.tag_name)
                .to_owned(),
            url: r.html_url,
            prerelease: r.prerelease,
            security: r
                .body
                .as_deref()
                .is_some_and(|b| b.lines().any(|line| line.trim() == "### Security")),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    const LIST: &str = r####"[
        {"tag_name":"v0.1.2","html_url":"https://example.test/0.1.2","prerelease":false,"body":"### Security\n- x"},
        {"tag_name":"v0.1.3","html_url":"https://example.test/0.1.3","draft":true},
        {"tag_name":"v0.1.2-beta.0","html_url":"https://example.test/b0","prerelease":true}
    ]"####;

    fn ok(etag: &str) -> Answer {
        Answer {
            status: 200,
            etag: Some(etag.to_owned()),
            body: LIST.as_bytes().to_vec(),
            ..Answer::default()
        }
    }

    fn status(status: u16, wait: Option<Duration>) -> Answer {
        Answer {
            status,
            wait,
            ..Answer::default()
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

    #[test]
    fn drafts_are_left_out_and_security_notes_are_marked() {
        let list = published(LIST.as_bytes()).unwrap();
        let versions: Vec<_> = list.iter().map(|r| r.version.as_str()).collect();
        assert_eq!(versions, ["0.1.2", "0.1.2-beta.0"]);
        assert!(list[0].security);
        assert!(!list[1].security);
        assert!(list[1].prerelease);
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

        let (sent, got) = ask(&releases, start + EVERY, Ok(status(304, None))).await;
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
            Ok(status(403, Some(Duration::from_mins(40)))),
        )
        .await;
        assert_eq!(got.unwrap_err(), "GitHub answered 403");

        let (sent, _) = ask(&releases, start + Duration::from_mins(39), Ok(ok("\"a\""))).await;
        assert_eq!(sent, None);
        let (sent, got) = ask(&releases, start + Duration::from_mins(40), Ok(ok("\"a\""))).await;
        assert!(sent.is_some());
        assert_eq!(got.unwrap().len(), 2);
    }

    #[test]
    fn github_says_how_long_to_wait() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let headers = |pairs: &[(&'static str, &'static str)]| {
            pairs
                .iter()
                .map(|(k, v)| (http::HeaderName::from_static(k), v.parse().unwrap()))
                .collect::<HeaderMap>()
        };
        let used_up = headers(&[
            ("x-ratelimit-remaining", "0"),
            ("x-ratelimit-reset", "1600"),
        ]);
        assert_eq!(wait(&used_up, now), Some(Duration::from_secs(600)));
        let left = headers(&[
            ("x-ratelimit-remaining", "3"),
            ("x-ratelimit-reset", "1600"),
        ]);
        assert_eq!(wait(&left, now), None);
        assert_eq!(
            wait(&headers(&[("retry-after", "90")]), now),
            Some(Duration::from_secs(90))
        );
    }
}
