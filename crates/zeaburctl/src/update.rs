//! The update check (docs/reference/telemetry.mdx, "Update check"): any command prints one line
//! when the releases it last read name a newer `zeaburctl`, at most once a day for each version,
//! and at most once an hour asks GitHub again on a thread of its own. The command never waits
//! for it: an answer that has not come by the time the command ends is left for the next one.

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
/// The least time between two requests to GitHub.
const EVERY: Duration = Duration::from_hours(1);
/// The least time between two notices of the same version.
const REMIND: Duration = Duration::from_hours(24);
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
    path: PathBuf,
    now: u64,
    last: Cache,
    answer: Option<Receiver<Vec<Known>>>,
}

impl Check {
    /// Starts a check when one is wanted.
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
        let mut ask = false;
        // Claimed before asking, so commands that end first ask at most once an hour.
        let last = Cache::update(&path, |cache| {
            ask = since(cache.checked_at, now) >= EVERY.as_secs();
            if ask {
                cache.checked_at = now;
            }
            ask
        })
        .ok()?;
        let answer = ask.then(|| {
            let (send, receive) = mpsc::channel();
            let url = url.to_owned();
            let path = path.clone();
            std::thread::spawn(move || {
                let Some(releases) = fetch(&url) else { return };
                let known = known(&releases);
                drop(Cache::update(&path, |cache| {
                    cache.releases.clone_from(&known);
                    true
                }));
                drop(send.send(known));
            });
            receive
        });
        Some(Self {
            path,
            now,
            last,
            answer,
        })
    }

    /// Prints the notice from the answer when it has come, else from the releases read last
    /// time; never waits.
    pub(crate) fn finish(self) {
        if let Some(notice) = self.notice() {
            output::info(&notice);
        }
    }

    /// The notice to print now, recorded so the same version is not named again within a day.
    fn notice(self) -> Option<String> {
        let fresh = self.answer.and_then(|answer| answer.try_recv().ok());
        let releases = fresh.as_deref().unwrap_or(&self.last.releases);
        let newer = newer(releases, RUNNING)?;
        let mut tell = false;
        drop(Cache::update(&self.path, |cache| {
            tell = cache.notified.as_deref() != Some(newer.latest.as_str())
                || since(cache.notified_at, self.now) >= REMIND.as_secs();
            if tell {
                cache.notified = Some(newer.latest.clone());
                cache.notified_at = self.now;
            }
            tell
        }));
        tell.then(|| newer.notice(RUNNING))
    }
}

/// Seconds from `then` to `now`, as if a long time had passed when the clock went back.
const fn since(then: u64, now: u64) -> u64 {
    if now < then { u64::MAX } else { now - then }
}

/// The last check, in the user's cache directory.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Cache {
    /// When GitHub was last asked, in seconds since the Unix epoch.
    checked_at: u64,
    /// The releases of the last answer.
    releases: Vec<Known>,
    /// The version the last notice named, and when.
    #[serde(skip_serializing_if = "Option::is_none")]
    notified: Option<String>,
    notified_at: u64,
}

impl Cache {
    fn read(path: &Path) -> Option<Self> {
        serde_json::from_slice(&std::fs::read(path).ok()?).ok()
    }

    /// Reads the cache, lets `change` change it, and writes it back when `change` says so; the
    /// cache as `change` left it. Commands running at once, and a command's own check, take
    /// turns through a lock file, and a new file replaces the old whole, so neither undoes the
    /// other's change nor reads half a file.
    fn update(path: &Path, change: impl FnOnce(&mut Self) -> bool) -> Result<Self, String> {
        let dir = path.parent().ok_or("no directory")?;
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let lock = std::fs::File::create(path.with_extension("lock")).map_err(|e| e.to_string())?;
        lock.lock().map_err(|e| e.to_string())?;
        let mut cache = Self::read(path).unwrap_or_default();
        if change(&mut cache) {
            let json = serde_json::to_vec(&cache).map_err(|e| e.to_string())?;
            let new = path.with_extension(format!("json.{}", std::process::id()));
            std::fs::write(&new, json).map_err(|e| e.to_string())?;
            std::fs::rename(&new, path).map_err(|e| e.to_string())?;
        }
        Ok(cache)
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
    /// Its notes have a `### Security` section, the changelog's heading for fixed vulnerabilities.
    fn fixes_security(&self) -> bool {
        self.body
            .as_deref()
            .is_some_and(|body| body.lines().any(|line| line.trim() == "### Security"))
    }
}

/// A published release, as the cache keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Known {
    tag: String,
    prerelease: bool,
    security: bool,
}

impl Known {
    fn version(&self, pre: bool) -> Option<Version> {
        if self.prerelease && !pre {
            return None;
        }
        Version::parse(&self.tag)
    }
}

/// The published releases in GitHub's answer.
fn known(releases: &[Release]) -> Vec<Known> {
    releases
        .iter()
        .filter(|r| !r.draft)
        .map(|r| Known {
            tag: r.tag_name.clone(),
            prerelease: r.prerelease,
            security: r.fixes_security(),
        })
        .collect()
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
fn newer(releases: &[Known], running: &str) -> Option<Newer> {
    let running = Version::parse(running)?;
    let latest = releases
        .iter()
        .filter_map(|r| r.version(!running.pre.is_empty()))
        .max()?;
    if latest <= running {
        return None;
    }
    let security = releases
        .iter()
        .any(|r| r.security && r.version(false).is_some_and(|v| v > running));
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
        let found = newer(&known(&releases), "0.1.1").unwrap();
        assert_eq!(found.latest, "0.2.0");
        assert!(!found.security);
        assert_eq!(newer(&known(&releases), "0.2.0"), None);
        assert_eq!(newer(&known(&releases), "0.3.0"), None);
        assert_eq!(
            newer(&known(&[release("v0.1.1-beta.1", "")]), "0.1.0"),
            None
        );
        assert_eq!(newer(&[], "0.1.0"), None);
    }

    #[test]
    fn someone_on_a_pre_release_hears_of_newer_pre_releases() {
        let releases = [
            release("v0.2.0-rc.2", "### Security\n- In a pre-release"),
            release("v0.1.1", ""),
        ];
        let found = newer(&known(&releases), "0.2.0-rc.1").unwrap();
        assert_eq!(found.latest, "0.2.0-rc.2");
        assert!(!found.security);
        assert_eq!(newer(&known(&releases), "0.1.1"), None);
        let out = [release("v0.2.0", ""), release("v0.2.0-rc.2", "")];
        assert_eq!(newer(&known(&out), "0.2.0-rc.1").unwrap().latest, "0.2.0");
    }

    #[test]
    fn a_pre_release_is_behind_its_normal_release() {
        let found = newer(&known(&[release("v0.1.1", "")]), "0.1.1-beta.2").unwrap();
        assert_eq!(found.latest, "0.1.1");
    }

    #[test]
    fn a_security_section_in_any_newer_release_marks_the_running_version() {
        let releases = [
            release("v0.3.0", "### Added\n- Things"),
            release("v0.2.0", "## 0.2.0\n\n### Security\n\n- Fixed a hole"),
            release("v0.1.0", "### Security\n- Older"),
        ];
        assert!(newer(&known(&releases), "0.1.0").unwrap().security);
        assert!(!newer(&known(&releases), "0.2.0").unwrap().security);
        let mentioned = [release(
            "v0.2.0",
            "Security fixes are listed below.\n#### Security",
        )];
        assert!(!newer(&known(&mentioned), "0.1.0").unwrap().security);
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
    fn the_answer_is_kept_for_the_next_command_and_asked_again_after_an_hour() {
        let dir = tempfile::tempdir().unwrap();
        let url = github(RELEASES_JSON, Duration::ZERO);
        let now = SystemTime::now();
        let check = Check::start_at(&url, dir.path(), now).unwrap();
        let answer = check
            .answer
            .as_ref()
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        assert_eq!(answer[0].tag, "v99.0.0");
        let cache = Cache::read(&dir.path().join("update-check.json")).unwrap();
        assert_eq!(cache.releases, answer);

        let minute = Duration::from_secs(60);
        let next = Check::start_at(&url, dir.path(), now + minute).unwrap();
        assert!(next.answer.is_none());
        assert!(
            Check::start_at(&url, dir.path(), now + 61 * minute)
                .unwrap()
                .answer
                .is_some()
        );
    }

    #[test]
    fn a_command_that_ends_first_leaves_the_answer_to_the_next() {
        let dir = tempfile::tempdir().unwrap();
        let url = github(RELEASES_JSON, Duration::from_millis(300));
        let now = SystemTime::now();
        assert_eq!(
            Check::start_at(&url, dir.path(), now).unwrap().notice(),
            None
        );
        let path = dir.path().join("update-check.json");
        let started = Instant::now();
        while Cache::read(&path).is_none_or(|c| c.releases.is_empty()) {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "no answer was kept"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        let later = Check::start_at(&url, dir.path(), now + Duration::from_secs(60)).unwrap();
        let notice = later.notice().unwrap();
        assert!(notice.contains("99.0.0"), "{notice}");
    }

    #[test]
    fn the_same_version_is_named_once_a_day() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check.json");
        let now = SystemTime::now();
        let checked = now.duration_since(UNIX_EPOCH).unwrap().as_secs();
        let cache = |tag: &str| Cache {
            checked_at: checked,
            releases: known(&[release(tag, "")]),
            ..Cache::default()
        };
        put(&path, cache("v99.0.0"));
        let at = |hours: u64| {
            Check::start_at(
                "http://127.0.0.1:9/",
                dir.path(),
                now + Duration::from_hours(hours),
            )
            .unwrap()
            .notice()
        };
        assert!(at(0).is_some());
        assert_eq!(at(0), None);
        Cache::update(&path, |c| {
            c.releases = cache("v99.1.0").releases;
            true
        })
        .unwrap();
        assert!(at(0).is_some(), "a newer version is named at once");
        assert!(at(25).is_some());
    }

    fn put(path: &Path, cache: Cache) {
        Cache::update(path, |c| {
            *c = cache;
            true
        })
        .unwrap();
    }

    #[test]
    fn changes_made_at_once_are_all_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check.json");
        let tags: Vec<String> = (0..16).map(|i| format!("v1.0.{i}")).collect();
        std::thread::scope(|scope| {
            for tag in &tags {
                let path = &path;
                scope.spawn(move || {
                    Cache::update(path, |c| {
                        c.releases.extend(known(&[release(tag, "")]));
                        true
                    })
                    .unwrap();
                });
            }
        });
        let kept = Cache::read(&path).unwrap();
        for tag in &tags {
            assert!(
                kept.releases.iter().any(|r| &r.tag == tag),
                "{tag} was lost"
            );
        }
    }
}
