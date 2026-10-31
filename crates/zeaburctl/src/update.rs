//! The update check (docs/reference/telemetry.mdx, "Update check"): at most once a day, any
//! command asks GitHub for Zeabur's releases on a thread of its own, and prints one line when a
//! newer `zeaburctl` is out. The command never waits for it: an answer that has not come by the
//! time the command ends is dropped.

use std::cmp::Ordering;
use std::ffi::OsString;
use std::io::IsTerminal as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use http::{Request, header};
use http_body_util::{BodyExt as _, Empty, Limited};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::{Deserialize, Serialize};

use crate::output;

/// Zeabur's releases, newest first; one page is enough.
const RELEASES: &str = "https://api.github.com/repos/zeabur-oss/zeabur/releases";
/// Where to download and verify a release.
const DOWNLOAD: &str = "https://oss.zeabur.com/docs/zeabur/reference/releases/";
/// The least time between two checks.
const EVERY: Duration = Duration::from_hours(24);
/// How long a check may take.
const TIMEOUT: Duration = Duration::from_secs(5);
/// The largest answer read.
const MAX_BODY: usize = 8 * 1024 * 1024;
/// The running version.
const RUNNING: &str = env!("CARGO_PKG_VERSION");

/// Whether the environment allows update checks: not with `DO_NOT_TRACK` or
/// `NO_UPDATE_NOTIFIER`. `zeaburctl ui` passes it on to the interface.
pub(crate) fn allowed() -> bool {
    allowed_in(&|name| std::env::var_os(name))
}

fn allowed_in(env: &dyn Fn(&str) -> Option<OsString>) -> bool {
    let do_not_track = env("DO_NOT_TRACK").map(|v| v.to_string_lossy().into_owned());
    env("NO_UPDATE_NOTIFIER").is_none()
        && !zeabur_telemetry::do_not_track_in(do_not_track.as_deref())
}

/// Whether this run may check: someone reads stderr, it is not CI, and the environment allows it.
fn wanted(env: &dyn Fn(&str) -> Option<OsString>, terminal: bool) -> bool {
    terminal && env("CI").is_none() && allowed_in(env)
}

/// A check under way.
#[derive(Debug)]
pub(crate) struct Check {
    answer: Receiver<Option<Newer>>,
}

impl Check {
    /// Starts a check when one is wanted and the last was more than a day ago.
    pub(crate) fn start() -> Option<Self> {
        if !wanted(
            &|name| std::env::var_os(name),
            std::io::stderr().is_terminal(),
        ) {
            return None;
        }
        Self::start_at(RELEASES, &cache_dir()?.join("zeaburctl"), SystemTime::now())
    }

    fn start_at(url: &str, dir: &Path, now: SystemTime) -> Option<Self> {
        let path = dir.join("update-check.json");
        let now = now.duration_since(UNIX_EPOCH).ok()?.as_secs();
        let last = Cache::read(&path);
        if !last.as_ref().is_none_or(|c| c.due(now)) {
            return None;
        }
        // Claimed before asking, so a command that ends first does not ask again on each run.
        let claim = Cache {
            checked_at: now,
            latest: last.and_then(|c| c.latest),
        };
        claim.write(&path).ok()?;
        let (send, answer) = mpsc::channel();
        let url = url.to_owned();
        std::thread::spawn(move || {
            let releases = fetch(&url);
            if let Some(releases) = &releases {
                let cache = Cache {
                    checked_at: now,
                    latest: latest(releases, false).map(|(version, _)| version.to_string()),
                };
                drop(cache.write(&path));
            }
            drop(send.send(releases.and_then(|r| newer(&r, RUNNING))));
        });
        Some(Self { answer })
    }

    /// Prints the notice when the answer has come and names a newer version; never waits.
    pub(crate) fn finish(self) {
        if let Ok(Some(newer)) = self.answer.try_recv() {
            output::info(&newer.notice(RUNNING));
        }
    }
}

/// The last check, in the user's cache directory.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Cache {
    /// Seconds since the Unix epoch.
    checked_at: u64,
    /// The newest release then, when there was one.
    #[serde(skip_serializing_if = "Option::is_none")]
    latest: Option<String>,
}

impl Cache {
    fn read(path: &Path) -> Option<Self> {
        serde_json::from_slice(&std::fs::read(path).ok()?).ok()
    }

    fn write(&self, path: &Path) -> Result<(), String> {
        let dir = path.parent().ok_or("no directory")?;
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    /// A day has passed, or the clock went back.
    const fn due(&self, now: u64) -> bool {
        now < self.checked_at || now - self.checked_at >= EVERY.as_secs()
    }
}

/// `$XDG_CACHE_HOME`, else `~/Library/Caches` on macOS and `~/.cache` elsewhere.
fn cache_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from)
        && dir.is_absolute()
    {
        return Some(dir);
    }
    let home = PathBuf::from(std::env::var_os("HOME")?);
    if cfg!(target_os = "macos") {
        Some(home.join("Library/Caches"))
    } else {
        Some(home.join(".cache"))
    }
}

fn fetch(url: &str) -> Option<Vec<Release>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?;
    runtime.block_on(async {
        let client = Client::builder(TokioExecutor::new()).build(crate::server::connector().ok()?);
        let request = Request::get(url)
            .header(header::USER_AGENT, format!("zeaburctl/{RUNNING}"))
            .header(header::ACCEPT, "application/vnd.github+json")
            .body(Empty::<Bytes>::new())
            .ok()?;
        let exchange = async {
            let response = client.request(request).await.ok()?;
            if !response.status().is_success() {
                return None;
            }
            let body = Limited::new(response.into_body(), MAX_BODY)
                .collect()
                .await
                .ok()?
                .to_bytes();
            serde_json::from_slice(&body).ok()
        };
        tokio::time::timeout(TIMEOUT, exchange).await.ok()?
    })
}

/// A release as GitHub's API lists it.
#[derive(Debug, Clone, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    body: Option<String>,
}

impl Release {
    /// Its version, for a published release that is not a pre-release.
    fn normal(&self) -> Option<Version> {
        self.version(false)
    }

    /// Its version, for a published release; a pre-release only when `pre` allows it.
    fn version(&self, pre: bool) -> Option<Version> {
        if self.draft || (self.prerelease && !pre) {
            return None;
        }
        Version::parse(&self.tag_name)
    }

    /// Its notes have a `### Security` section, the changelog's heading for fixed vulnerabilities.
    fn fixes_security(&self) -> bool {
        self.body
            .as_deref()
            .is_some_and(|body| body.lines().any(|line| line.trim() == "### Security"))
    }
}

fn latest(releases: &[Release], pre: bool) -> Option<(Version, &Release)> {
    releases
        .iter()
        .filter_map(|r| Some((r.version(pre)?, r)))
        .max_by(|a, b| a.0.cmp(&b.0))
}

/// A newer release than the running one.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Newer {
    /// The newest, such as `0.2.0`.
    latest: String,
    /// A newer release fixes vulnerabilities, so the running version has them.
    security: bool,
}

impl Newer {
    fn notice(&self, running: &str) -> String {
        if self.security {
            format!(
                "zeaburctl {running} has known security problems. Update to {} from {DOWNLOAD}",
                self.latest
            )
        } else {
            format!(
                "zeaburctl {} is out, and you have {running}. Download it from {DOWNLOAD}",
                self.latest
            )
        }
    }
}

/// The newest normal release when it is newer than `running`, or the newest pre-release when
/// `running` is one; `None` without one, or when `running` is not a version. Only normal
/// releases mark the running version insecure.
fn newer(releases: &[Release], running: &str) -> Option<Newer> {
    let running = Version::parse(running)?;
    let (latest, _) = latest(releases, !running.pre.is_empty())?;
    if latest <= running {
        return None;
    }
    let security = releases
        .iter()
        .filter(|r| r.normal().is_some_and(|v| v > running))
        .any(Release::fixes_security);
    Some(Newer {
        latest: latest.to_string(),
        security,
    })
}

/// A Semantic Versioning 2.0.0 version; build metadata is dropped, as it takes no part in
/// precedence.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Version {
    core: [u64; 3],
    pre: Vec<String>,
    text: String,
}

impl Version {
    /// `1.2.3`, `v1.2.3-beta.1` or `1.2.3+build`.
    fn parse(tag: &str) -> Option<Self> {
        let text = tag.strip_prefix('v').unwrap_or(tag);
        let text = text.split_once('+').map_or(text, |(version, _)| version);
        let (core, pre) = text
            .split_once('-')
            .map_or((text, None), |(core, pre)| (core, Some(pre)));
        let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
        let core = [parts.next()??, parts.next()??, parts.next()??];
        if parts.next().is_some() {
            return None;
        }
        let pre: Vec<String> =
            pre.map_or_else(Vec::new, |pre| pre.split('.').map(str::to_owned).collect());
        if pre.iter().any(String::is_empty) {
            return None;
        }
        Some(Self {
            core,
            pre,
            text: text.to_owned(),
        })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

impl Ord for Version {
    /// [SemVer 2.0.0, section 11]: a pre-release comes before its normal version; identifiers
    /// compare numerically when both are numbers, else as ASCII, and numbers come first.
    ///
    /// [SemVer 2.0.0, section 11]: https://semver.org/spec/v2.0.0.html#spec-item-11
    fn cmp(&self, other: &Self) -> Ordering {
        self.core
            .cmp(&other.core)
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => {
                    for (a, b) in self.pre.iter().zip(&other.pre) {
                        let order = match (a.parse::<u64>(), b.parse::<u64>()) {
                            (Ok(a), Ok(b)) => a.cmp(&b),
                            (Ok(_), Err(_)) => Ordering::Less,
                            (Err(_), Ok(_)) => Ordering::Greater,
                            (Err(_), Err(_)) => a.cmp(b),
                        };
                        if order != Ordering::Equal {
                            return order;
                        }
                    }
                    self.pre.len().cmp(&other.pre.len())
                }
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::time::Instant;

    use super::*;

    fn release(tag: &str, body: &str) -> Release {
        Release {
            tag_name: tag.to_owned(),
            draft: false,
            prerelease: tag.contains('-'),
            body: Some(body.to_owned()),
        }
    }

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
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
            assert!(v(pair[0]) < v(pair[1]), "{} < {}", pair[0], pair[1]);
        }
        assert_eq!(v("v1.2.3").cmp(&v("1.2.3+build.5")), Ordering::Equal);
        for bad in ["1.2", "1.2.3.4", "x.1.2", "1.2.3-", "1.2.3-a..b", ""] {
            assert!(Version::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn the_newest_normal_release_counts_and_drafts_and_pre_releases_do_not() {
        let mut draft = release("v0.4.0", "");
        draft.draft = true;
        let releases = [
            release("v0.3.0-beta.1", ""),
            draft,
            release("v0.1.2", ""),
            release("v0.2.0", ""),
        ];
        let found = newer(&releases, "0.1.1").unwrap();
        assert_eq!(found.latest, "0.2.0");
        assert!(!found.security);
        assert_eq!(newer(&releases, "0.2.0"), None);
        assert_eq!(newer(&releases, "0.3.0"), None);
        assert_eq!(newer(&[release("v0.1.1-beta.1", "")], "0.1.0"), None);
        assert_eq!(newer(&[], "0.1.0"), None);
    }

    #[test]
    fn someone_on_a_pre_release_hears_of_newer_pre_releases() {
        let releases = [
            release("v0.2.0-rc.2", "### Security\n- In a pre-release"),
            release("v0.1.1", ""),
        ];
        let found = newer(&releases, "0.2.0-rc.1").unwrap();
        assert_eq!(found.latest, "0.2.0-rc.2");
        assert!(!found.security);
        assert_eq!(newer(&releases, "0.1.1"), None);
        let out = [release("v0.2.0", ""), release("v0.2.0-rc.2", "")];
        assert_eq!(newer(&out, "0.2.0-rc.1").unwrap().latest, "0.2.0");
    }

    #[test]
    fn a_pre_release_is_behind_its_normal_release() {
        let found = newer(&[release("v0.1.1", "")], "0.1.1-beta.2").unwrap();
        assert_eq!(found.latest, "0.1.1");
    }

    #[test]
    fn a_security_section_in_any_newer_release_marks_the_running_version() {
        let releases = [
            release("v0.3.0", "### Added\n- Things"),
            release("v0.2.0", "## 0.2.0\n\n### Security\n\n- Fixed a hole"),
            release("v0.1.0", "### Security\n- Older"),
        ];
        assert!(newer(&releases, "0.1.0").unwrap().security);
        assert!(!newer(&releases, "0.2.0").unwrap().security);
        let mentioned = [release(
            "v0.2.0",
            "Security fixes are listed below.\n#### Security",
        )];
        assert!(!newer(&mentioned, "0.1.0").unwrap().security);
    }

    #[test]
    fn the_notice_is_one_line_that_says_what_to_do() {
        let plain = Newer {
            latest: "0.2.0".to_owned(),
            security: false,
        };
        let security = Newer {
            security: true,
            ..plain.clone()
        };
        for notice in [plain.notice("0.1.1"), security.notice("0.1.1")] {
            assert!(!notice.contains('\n'));
            assert!(
                notice.contains("0.2.0") && notice.contains(DOWNLOAD),
                "{notice}"
            );
        }
        assert!(security.notice("0.1.1").contains("security"));
    }

    #[test]
    fn checks_only_at_a_terminal_outside_ci_and_without_an_opt_out() {
        let env = |vars: &[(&str, &str)]| {
            let vars: BTreeMap<String, OsString> = vars
                .iter()
                .map(|(k, v)| ((*k).to_owned(), OsString::from(v)))
                .collect();
            move |name: &str| vars.get(name).cloned()
        };
        assert!(wanted(&env(&[]), true));
        assert!(!wanted(&env(&[]), false));
        assert!(!wanted(&env(&[("CI", "true")]), true));
        assert!(!wanted(&env(&[("NO_UPDATE_NOTIFIER", "")]), true));
        assert!(!wanted(&env(&[("DO_NOT_TRACK", "1")]), true));
        assert!(wanted(&env(&[("DO_NOT_TRACK", "0")]), true));
        assert!(!allowed_in(&env(&[("NO_UPDATE_NOTIFIER", "1")])));
        assert!(allowed_in(&env(&[("DO_NOT_TRACK", "")])));
    }

    /// Answers each request with `body`, after `delay`.
    fn github(body: &'static str, delay: Duration) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                std::thread::spawn(move || {
                    let mut buffer = [0; 4096];
                    drop(stream.read(&mut buffer));
                    std::thread::sleep(delay);
                    drop(write!(
                        stream,
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    ));
                });
            }
        });
        format!("http://{address}/releases")
    }

    const RELEASES_JSON: &str = r####"[{"tag_name":"v99.0.0","draft":false,"prerelease":false,"body":"### Security\n- x"}]"####;

    #[test]
    fn a_slow_answer_never_holds_the_command_up() {
        let dir = tempfile::tempdir().unwrap();
        let url = github(RELEASES_JSON, Duration::from_secs(30));
        let check = Check::start_at(&url, dir.path(), SystemTime::now()).unwrap();
        let started = Instant::now();
        check.finish();
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn one_check_a_day_and_its_answer_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let url = github(RELEASES_JSON, Duration::ZERO);
        let now = SystemTime::now();
        let check = Check::start_at(&url, dir.path(), now).unwrap();
        let answer = check.answer.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(
            answer,
            Some(Newer {
                latest: "99.0.0".to_owned(),
                security: true
            })
        );
        let cache = Cache::read(&dir.path().join("update-check.json")).unwrap();
        assert_eq!(cache.latest.as_deref(), Some("99.0.0"));

        let hour = Duration::from_hours(1);
        assert!(Check::start_at(&url, dir.path(), now + 23 * hour).is_none());
        assert!(Check::start_at(&url, dir.path(), now + 25 * hour).is_some());
    }
}
