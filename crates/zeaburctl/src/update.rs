//! The update check (docs/reference/telemetry.mdx, "Update check"). Each command asks GitHub for
//! the releases on a thread of its own while it runs, and prints one line when they name a newer
//! `zeaburctl`. At its end the command waits at most [`GRACE`] for the answer.
//!
//! Without a token GitHub answers 60 requests an hour from one address [gh-limit]. With a token in
//! `GH_TOKEN` or `GITHUB_TOKEN`, the variables `gh` reads, the requests count against the token's
//! own limit. The line says so when the limit of the address is used up.
//!
//! [gh-limit]: https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api#primary-rate-limit-for-unauthenticated-users

use std::ffi::OsString;
use std::io::IsTerminal as _;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use zeabur_api::images::https::{Https, Reach, tls};
use zeabur_api::releases::{self, Answer, RELEASES_URL};
use zeabur_api::views::ZeaburNewer;

use crate::output;

/// Where to download and verify a release.
const DOWNLOAD: &str = "https://oss.zeabur.com/docs/zeabur/reference/releases/";
/// How long a command that has ended waits for the answer.
const GRACE: Duration = Duration::from_secs(1);
/// The running version.
const RUNNING: &str = env!("CARGO_PKG_VERSION");
/// The line printed when GitHub's limit for the address is used up.
const LIMITED: &str = "zeaburctl could not check for a newer version: GitHub's limit for this \
                       address is used up. Set GH_TOKEN to a GitHub token to use your own limit.";

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

/// `GH_TOKEN`, else `GITHUB_TOKEN`. An empty one counts as none.
fn token(env: &dyn Fn(&str) -> Option<OsString>) -> Option<String> {
    ["GH_TOKEN", "GITHUB_TOKEN"].iter().find_map(|name| {
        env(name)
            .and_then(|v| v.into_string().ok())
            .filter(|v| !v.is_empty())
    })
}

/// A check under way.
#[derive(Debug)]
pub(crate) struct Check {
    notice: Receiver<Option<String>>,
}

impl Check {
    /// Starts a check when one is wanted.
    pub(crate) fn start() -> Option<Self> {
        let env = |name: &str| std::env::var_os(name);
        if !wanted(&env, std::io::stderr().is_terminal()) {
            return None;
        }
        Some(Self::start_at(RELEASES_URL, token(&env)))
    }

    fn start_at(url: &str, token: Option<String>) -> Self {
        let (send, receive) = mpsc::channel();
        let url = url.to_owned();
        std::thread::spawn(move || drop(send.send(fetch(&url, token.as_deref()))));
        Self { notice: receive }
    }

    /// Prints the notice once the answer comes, waiting at most [`GRACE`].
    pub(crate) fn finish(self) {
        if let Some(notice) = self.notice(GRACE) {
            output::info(&notice);
        }
    }

    fn notice(self, grace: Duration) -> Option<String> {
        self.notice.recv_timeout(grace).ok().flatten()
    }
}

/// The line to print for GitHub's answer, if any. A failure says nothing.
fn fetch(url: &str, token: Option<&str>) -> Option<String> {
    let tls = tls(None).ok()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?;
    let answer = runtime
        .block_on(async {
            let http = Https::new(Reach::Any, &tls);
            let agent = format!("zeaburctl/{RUNNING}");
            releases::ask(&http, url, &agent, None, token).await
        })
        .ok()?;
    match answer {
        Answer::Releases { releases, .. } => {
            releases::newer(&releases, RUNNING).map(|newer| notice(&newer, RUNNING))
        }
        Answer::Refused { used_up: true, .. } => Some(LIMITED.to_owned()),
        Answer::NotModified | Answer::Refused { .. } => None,
    }
}

fn notice(newer: &ZeaburNewer, running: &str) -> String {
    let latest = &newer.latest.version;
    if newer.security.is_empty() {
        format!("zeaburctl {latest} is out, and you have {running}. Download it from {DOWNLOAD}")
    } else {
        format!(
            "zeaburctl {running} has known security problems. Update to {latest} from {DOWNLOAD}"
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::time::Instant;

    use zeabur_api::views::ZeaburRelease;

    use super::*;

    #[test]
    fn the_notice_is_one_line_that_says_what_to_do() {
        let release = |security| ZeaburRelease {
            version: "0.2.0".to_owned(),
            url: String::new(),
            prerelease: false,
            security,
        };
        let plain = ZeaburNewer {
            latest: release(false),
            security: Vec::new(),
        };
        let security = ZeaburNewer {
            latest: release(true),
            security: vec![release(true)],
        };
        for notice in [notice(&plain, "0.1.1"), notice(&security, "0.1.1")] {
            assert!(!notice.contains('\n'));
            assert!(
                notice.contains("0.2.0") && notice.contains(DOWNLOAD),
                "{notice}"
            );
        }
        assert!(notice(&security, "0.1.1").contains("security"));
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

    /// Answers each request with `status`, `headers` and `body` after `delay`, and sends on
    /// each request it read.
    fn github(
        status: &'static str,
        headers: &'static str,
        body: &'static str,
        delay: Duration,
    ) -> (String, Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (seen, requests) = mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let seen = seen.clone();
                std::thread::spawn(move || {
                    let mut buffer = [0; 4096];
                    let read = stream.read(&mut buffer).unwrap_or(0);
                    drop(seen.send(String::from_utf8_lossy(&buffer[..read]).into_owned()));
                    std::thread::sleep(delay);
                    drop(write!(
                        stream,
                        "HTTP/1.1 {status}\r\n{headers}content-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    ));
                });
            }
        });
        (format!("http://{address}/releases"), requests)
    }

    const RELEASES_JSON: &str = r####"[{"tag_name":"v99.0.0","html_url":"https://example.test/99","body":"### Security\n- x"}]"####;
    const WAIT: Duration = Duration::from_secs(10);

    #[test]
    fn a_slow_answer_holds_the_command_up_no_longer_than_the_grace() {
        let (url, _) = github("200 OK", "", RELEASES_JSON, Duration::from_secs(30));
        let started = Instant::now();
        assert_eq!(
            Check::start_at(&url, None).notice(Duration::from_millis(200)),
            None
        );
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn every_command_asks_and_names_a_newer_release() {
        let (url, requests) = github("200 OK", "", RELEASES_JSON, Duration::ZERO);
        for _ in 0..2 {
            let notice = Check::start_at(&url, None).notice(WAIT).unwrap();
            assert!(notice.contains("99.0.0"), "{notice}");
            assert!(requests.recv_timeout(WAIT).is_ok());
        }
    }

    #[test]
    fn a_token_is_sent_to_github_as_a_bearer() {
        let (url, requests) = github("200 OK", "", "[]", Duration::ZERO);
        Check::start_at(&url, Some("t0ken".to_owned())).notice(WAIT);
        let request = requests.recv_timeout(WAIT).unwrap().to_ascii_lowercase();
        assert!(request.contains("authorization: bearer t0ken"), "{request}");
        let (url, requests) = github("200 OK", "", "[]", Duration::ZERO);
        Check::start_at(&url, None).notice(WAIT);
        let request = requests.recv_timeout(WAIT).unwrap().to_ascii_lowercase();
        assert!(!request.contains("authorization"), "{request}");
    }

    #[test]
    fn a_used_up_limit_says_to_set_a_token_and_other_failures_say_nothing() {
        let used_up = "x-ratelimit-remaining: 0\r\n";
        for status in ["403 Forbidden", "429 Too Many Requests"] {
            let (url, _) = github(status, used_up, "{}", Duration::ZERO);
            let notice = Check::start_at(&url, None).notice(WAIT).unwrap();
            assert!(notice.contains("GH_TOKEN"), "{notice}");
        }
        let (url, _) = github("403 Forbidden", "", "{}", Duration::ZERO);
        assert_eq!(Check::start_at(&url, None).notice(WAIT), None);
        let (url, _) = github("500 Internal Server Error", "", "{}", Duration::ZERO);
        assert_eq!(Check::start_at(&url, None).notice(WAIT), None);
    }

    #[test]
    fn the_token_comes_from_gh_token_then_github_token() {
        let env = |vars: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                vars.iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| OsString::from(v))
            }
        };
        assert_eq!(
            token(&env(&[("GH_TOKEN", "a"), ("GITHUB_TOKEN", "b")])).as_deref(),
            Some("a")
        );
        assert_eq!(
            token(&env(&[("GH_TOKEN", ""), ("GITHUB_TOKEN", "b")])).as_deref(),
            Some("b")
        );
        assert_eq!(token(&env(&[])), None);
    }
}
