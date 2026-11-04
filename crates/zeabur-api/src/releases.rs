//! Zeabur's releases on GitHub and which of them is newer than a running version: the update
//! check of the server and of `zeaburctl` (docs/reference/telemetry.mdx, "Update check").
//!
//! [`ask`] makes one request. The server keeps its answer for an hour (`cluster::update`), and
//! `zeaburctl` asks on each command.

use std::time::{Duration, SystemTime};

use axum::http::{self, HeaderMap, HeaderValue, Request};
use semver::Version;
use serde::Deserialize;

use crate::images::https::Https;
use crate::views::{ZeaburNewer, ZeaburRelease};

/// Zeabur's releases, newest first. One page is enough.
pub const RELEASES_URL: &str = "https://api.github.com/repos/zeabur-oss/zeabur/releases";
/// The most a list of releases may be.
const LIMIT: usize = 8 * 1024 * 1024;

/// GitHub's answer to one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// The published releases, and the `ETag` to name next time.
    Releases {
        /// Newest first.
        releases: Vec<ZeaburRelease>,
        /// The answer's `ETag`.
        etag: Option<String>,
    },
    /// `304`: the releases are those of the `ETag` named.
    NotModified,
    /// Any other status.
    Refused {
        /// The status, such as `403`.
        status: u16,
        /// How long GitHub asks to be left alone.
        wait: Option<Duration>,
        /// `403` or `429` with no requests left: the address, or the token, has used up its
        /// limit [gh-limit].
        ///
        /// [gh-limit]: https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api#exceeding-the-rate-limit
        used_up: bool,
    },
}

/// Asks `url` for the releases as `user_agent`, naming the last answer's `etag`, with `token` as
/// a bearer when there is one.
///
/// # Errors
///
/// When no answer comes in time, or a `200` is not a list of releases.
pub async fn ask(
    http: &Https,
    url: &str,
    user_agent: &str,
    etag: Option<&str>,
    token: Option<&str>,
) -> Result<Answer, String> {
    let mut request = Request::get(url)
        .header(http::header::USER_AGENT, user_agent)
        .header(http::header::ACCEPT, "application/vnd.github+json");
    if let Some(etag) = etag {
        request = request.header(http::header::IF_NONE_MATCH, etag);
    }
    if let Some(token) = token {
        let mut value =
            HeaderValue::from_str(&format!("Bearer {token}")).map_err(|e| e.to_string())?;
        value.set_sensitive(true);
        request = request.header(http::header::AUTHORIZATION, value);
    }
    let request = request
        .body(http_body_util::Full::default())
        .map_err(|e| e.to_string())?;
    let (status, headers, body) = http.exchange_with_headers(request, LIMIT).await?;
    Ok(match status {
        200 => Answer::Releases {
            releases: parse(&body)?,
            etag: text(&headers, http::header::ETAG.as_str()).map(str::to_owned),
        },
        304 => Answer::NotModified,
        _ => Answer::Refused {
            status,
            wait: wait(&headers, SystemTime::now()),
            used_up: matches!(status, 403 | 429)
                && text(&headers, "x-ratelimit-remaining") == Some("0"),
        },
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

/// The published releases in GitHub's list, with the tag's `v` dropped.
///
/// # Errors
///
/// When `body` is not GitHub's list of releases.
pub fn parse(body: &[u8]) -> Result<Vec<ZeaburRelease>, String> {
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
            // The changelog's heading for fixed vulnerabilities.
            security: r
                .body
                .as_deref()
                .is_some_and(|b| b.lines().any(|line| line.trim() == "### Security")),
        })
        .collect())
}

/// The release to update to from `running`, if one is newer.
///
/// That is the newest normal release, or the newest pre-release when `running` is one. The newer
/// normal releases that fix vulnerabilities come with it. `None` when `running` is not a version.
#[must_use]
pub fn newer(releases: &[ZeaburRelease], running: &str) -> Option<ZeaburNewer> {
    let running = version(running)?;
    let mut newer: Vec<(Version, &ZeaburRelease)> = releases
        .iter()
        .filter(|r| !r.prerelease || !running.pre.is_empty())
        .filter_map(|r| Some((version(&r.version)?, r)))
        .filter(|(v, _)| v.cmp_precedence(&running).is_gt())
        .collect();
    newer.sort_by(|a, b| b.0.cmp_precedence(&a.0));
    let (_, latest) = newer.first()?;
    Some(ZeaburNewer {
        latest: (*latest).clone(),
        security: newer
            .iter()
            .filter(|(_, r)| r.security && !r.prerelease)
            .map(|(_, r)| (*r).clone())
            .collect(),
    })
}

/// `1.2.3`, `v1.2.3-beta.1` or `1.2.3+build`.
fn version(tag: &str) -> Option<Version> {
    Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(version: &str, security: bool) -> ZeaburRelease {
        ZeaburRelease {
            version: version.to_owned(),
            url: format!("https://example.test/{version}"),
            prerelease: version.contains('-'),
            security,
        }
    }

    fn latest(releases: &[ZeaburRelease], running: &str) -> Option<String> {
        newer(releases, running).map(|n| n.latest.version)
    }

    fn v(text: &str) -> Version {
        version(text).unwrap()
    }

    #[test]
    fn versions_follow_semver_precedence() {
        let ordered = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
            "1.0.1",
            "1.1.0",
            "1.10.0",
            "2.0.0",
        ];
        for pair in ordered.windows(2) {
            assert!(
                v(pair[0]).cmp_precedence(&v(pair[1])).is_lt(),
                "{} < {}",
                pair[0],
                pair[1]
            );
        }
        assert!(v("v1.2.3").cmp_precedence(&v("1.2.3+build.5")).is_eq());
        for bad in ["1.2", "1.2.3.4", "x.1.2", "1.2.3-", "1.2.3-a..b", ""] {
            assert!(version(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn drafts_are_left_out_and_security_notes_are_marked() {
        let list = parse(
            br#####"[
                {"tag_name":"v0.1.2","html_url":"https://example.test/0.1.2","body":"## 0.1.2\n\n### Security\n- x"},
                {"tag_name":"v0.1.3","html_url":"https://example.test/0.1.3","draft":true},
                {"tag_name":"v0.1.2-beta.0","html_url":"https://example.test/b0","prerelease":true,"body":"#### Security"}
            ]"#####,
        )
        .unwrap();
        let versions: Vec<_> = list.iter().map(|r| r.version.as_str()).collect();
        assert_eq!(versions, ["0.1.2", "0.1.2-beta.0"]);
        assert!(list[0].security);
        assert!(!list[1].security);
        assert!(list[1].prerelease);
    }

    #[test]
    fn the_newest_normal_release_counts_and_pre_releases_do_not() {
        let releases = [
            release("0.3.0-beta.1", false),
            release("0.1.2", false),
            release("0.2.0", false),
            release("not-a-version", false),
        ];
        assert_eq!(latest(&releases, "0.1.1").as_deref(), Some("0.2.0"));
        assert_eq!(latest(&releases, "0.2.0"), None);
        assert_eq!(latest(&releases, "0.3.0"), None);
        assert_eq!(latest(&[release("0.1.1-beta.1", false)], "0.1.0"), None);
        assert_eq!(latest(&releases, "dev"), None);
    }

    #[test]
    fn someone_on_a_pre_release_hears_of_newer_pre_releases() {
        let releases = [release("0.2.0-rc.2", true), release("0.1.1", false)];
        let found = newer(&releases, "0.2.0-rc.1").unwrap();
        assert_eq!(found.latest.version, "0.2.0-rc.2");
        assert!(found.security.is_empty(), "only normal releases mark one");
        let out = [release("0.2.0", false), release("0.2.0-rc.2", false)];
        assert_eq!(latest(&out, "0.2.0-rc.1").as_deref(), Some("0.2.0"));
        assert_eq!(
            latest(&[release("0.1.1", false)], "0.1.1-beta.2").as_deref(),
            Some("0.1.1")
        );
    }

    #[test]
    fn every_newer_release_that_fixes_vulnerabilities_is_named() {
        let releases = [
            release("0.4.0", false),
            release("0.3.0", true),
            release("0.2.0", true),
            release("0.1.0", true),
        ];
        let found = newer(&releases, "0.1.0").unwrap();
        assert_eq!(found.latest.version, "0.4.0");
        let fixed: Vec<_> = found.security.iter().map(|r| r.version.as_str()).collect();
        assert_eq!(fixed, ["0.3.0", "0.2.0"]);
        assert_eq!(newer(&releases, "0.3.0").unwrap().security, []);
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
