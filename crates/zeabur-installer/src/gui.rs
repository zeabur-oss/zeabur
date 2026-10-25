//! `gui`: the same plan and `apply` behind a page in the browser, for people who would rather not
//! write YAML.
//!
//! It listens on the loopback address and is reached through `ssh -L`, so nothing new is exposed.
//! The page writes the same configuration file the command line reads and runs `apply` as a child
//! process, so both paths install exactly the same thing. A random token in the printed link
//! becomes a `SameSite=Strict` cookie; every API call needs it, and requests with a `Host` other
//! than the loopback address are refused, which also stops DNS rebinding.

mod http;

use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io::{self, BufRead as _, BufReader, Read};
use std::net::{SocketAddr, TcpListener, ToSocketAddrs as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};

use serde_json::{Value, json};
use zeabur_installer_host::{facts, files, preflight, systemd};
use zeabur_installer_plan::catalog::{Default, Index};
use zeabur_installer_plan::config::{Config, Sources};
use zeabur_installer_plan::plan;

use self::http::{Request, Response};
use crate::{InstallError, apply, load, out, render};

/// A file of the web interface (`web/`), embedded by `build.rs`.
pub(crate) struct Asset {
    path: &'static str,
    content_type: &'static str,
    #[expect(
        dead_code,
        reason = "build.rs writes it; every response here is no-store"
    )]
    etag: &'static str,
    #[expect(
        dead_code,
        reason = "build.rs writes it; every response here is no-store"
    )]
    immutable: bool,
    identity: &'static [u8],
    brotli: Option<&'static [u8]>,
}

include!(concat!(env!("OUT_DIR"), "/embedded.rs"));

const COOKIE: &str = "zeabur_installer";
/// The most connections the page serves at a time. The server closes other connections.
const MAX_CONNECTIONS: usize = 16;
/// The most lines of `apply` output a job keeps. The job drops the oldest lines first.
const MAX_LINES: usize = 10_000;
/// The most bytes a job keeps of one line.
const MAX_LINE: usize = 4096;

#[derive(Debug, Default)]
struct Job {
    lines: VecDeque<String>,
    /// The number of lines dropped from the front of `lines`.
    dropped: usize,
    running: bool,
    code: Option<i32>,
}

#[derive(Debug)]
struct Server {
    token: String,
    port: u16,
    loopback: bool,
    config_path: PathBuf,
    job: Arc<Mutex<Job>>,
}

pub(crate) fn run(config_path: &Path, listen: SocketAddr) -> Result<(), InstallError> {
    let listener =
        TcpListener::bind(listen).map_err(|e| InstallError::Message(format!("{listen}: {e}")))?;
    let token = files::random_hex()?.chars().take(32).collect::<String>();
    let server = Arc::new(Server {
        token,
        port: listen.port(),
        loopback: listen.ip().is_loopback(),
        config_path: config_path.to_owned(),
        job: Arc::default(),
    });
    let host = if server.loopback {
        format!("127.0.0.1:{}", listen.port())
    } else {
        listen.to_string()
    };
    out::line(&format!(
        "Open http://{host}/?token={} in a browser.",
        server.token
    ));
    if server.loopback {
        out::line(&format!(
            "From your computer, forward the port first: ssh -L {0}:127.0.0.1:{0} root@<this server>",
            listen.port()
        ));
    } else {
        out::err("warning: the page is served over plain HTTP; anyone on the path can read it.");
    }
    out::line("Stop with Ctrl-C once the install has finished.");
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming().flatten() {
        if active.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
            active.fetch_sub(1, Ordering::AcqRel);
            continue;
        }
        let server = Arc::clone(&server);
        let active = Arc::clone(&active);
        thread::spawn(move || {
            let response = http::read(&stream).map_or_else(
                || Response::new(400, "text/plain", "bad request"),
                |request| server.handle(&request),
            );
            http::write(&stream, &response);
            active.fetch_sub(1, Ordering::AcqRel);
        });
    }
    Ok(())
}

impl Server {
    fn handle(&self, request: &Request) -> Response {
        if self.loopback && !self.allowed_host(request.header("host").unwrap_or_default()) {
            return Response::new(403, "text/plain", "this page answers on 127.0.0.1 only");
        }
        if request.method == "GET" && request.path == "/" && request.query_param("token").is_some()
        {
            if !self.token_matches(request.query_param("token")) {
                return Response::new(
                    403,
                    "text/plain",
                    "this link is not valid; use the one printed in the terminal",
                );
            }
            return Response::new(302, "text/plain", "")
                .with_header("Location", "/".to_owned())
                .with_header(
                    "Set-Cookie",
                    format!("{COOKIE}={}; HttpOnly; SameSite=Strict; Path=/", self.token),
                );
        }
        if !self.token_matches(request.cookie(COOKIE)) {
            return Response::new(403, "text/plain", "open the link printed in the terminal");
        }
        if request.method == "POST" && request.header("content-type") != Some("application/json") {
            return Response::new(400, "text/plain", "JSON only");
        }
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/api/state") => self.state(),
            ("GET", "/api/resolve") => resolve(request.query_param("name").unwrap_or_default()),
            ("POST", "/api/plan") => self.plan(&request.body),
            ("POST", "/api/apply") => self.apply(&request.body),
            ("GET", "/api/apply") => self.progress(
                request
                    .query_param("from")
                    .and_then(|f| f.parse().ok())
                    .unwrap_or(0),
            ),
            ("GET", "/api/local-ca.crt") => {
                fs::read(Path::new(apply::STATE_DIR).join("local-ca.crt")).map_or_else(
                    |_| Response::new(404, "text/plain", "no local CA yet"),
                    |pem| {
                        Response::new(200, "application/x-x509-ca-cert", pem).with_header(
                            "Content-Disposition",
                            "attachment; filename=\"zeabur-local-ca.crt\"".to_owned(),
                        )
                    },
                )
            }
            ("GET", path) if !path.starts_with("/api/") => asset(request, path),
            _ => Response::new(404, "text/plain", "not found"),
        }
    }

    fn allowed_host(&self, host: &str) -> bool {
        host == format!("127.0.0.1:{}", self.port) || host == format!("localhost:{}", self.port)
    }

    fn token_matches(&self, given: Option<&str>) -> bool {
        let given = given.unwrap_or_default().as_bytes();
        let token = self.token.as_bytes();
        given.len() == token.len()
            && given.iter().zip(token).fold(0, |acc, (a, b)| acc | (a ^ b)) == 0
    }

    /// Everything the first page needs: host facts, checks, addresses, the catalog and an
    /// existing configuration to start from.
    fn state(&self) -> Response {
        let existing = fs::read_to_string(&self.config_path)
            .ok()
            .and_then(|text| Config::parse(&text).ok());
        let config_exists = existing.is_some();
        let addresses = facts::addresses();
        let existing = existing.or_else(|| addresses.first().map(|a| Config::defaults(a)));
        let host = facts::read().ok();
        let facts = host.as_ref().map_or(Value::Null, |f| json!(f));
        let checks = preflight::run(preflight::Options {
            rke2_running: systemd::is_active("rke2-server"),
            require_encrypted_disk: existing
                .as_ref()
                .is_some_and(|c| c.host.require_encrypted_disk),
            server: true,
        });
        let sources = existing
            .as_ref()
            .map_or_else(Sources::default, |c| c.sources.clone());
        let (catalog, catalog_error) = match load::catalog(&sources) {
            Ok(index) => (
                catalog_json(
                    &index,
                    existing
                        .as_ref()
                        .map_or("single-node", |c| c.profile.as_str()),
                    host.as_ref().map_or("", |f| f.arch.as_str()),
                ),
                None,
            ),
            Err(e) => (Value::Null, Some(e.to_string())),
        };
        let applying = self
            .job
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .running;
        Response::json(
            200,
            &json!({
                "facts": facts,
                "checks": checks,
                "addresses": addresses,
                "config": existing,
                "configExists": config_exists,
                "configPath": self.config_path,
                "catalog": catalog,
                "catalogError": catalog_error,
                "applying": applying,
            }),
        )
    }

    /// The configuration the page sends, with secret values replaced by the files they will be
    /// written to.
    fn config(&self, body: &[u8]) -> Result<(Config, Vec<(PathBuf, String)>), InstallError> {
        let mut input: Value =
            serde_json::from_slice(body).map_err(|e| InstallError::Message(e.to_string()))?;
        let secrets: BTreeMap<String, BTreeMap<String, String>> = serde_json::from_value(
            input
                .get_mut("secrets")
                .map_or_else(|| json!({}), Value::take),
        )
        .map_err(|e| InstallError::Message(e.to_string()))?;
        let mut config: Config =
            serde_json::from_value(input.get_mut("config").map_or(Value::Null, Value::take))
                .map_err(|e| InstallError::Message(format!("configuration: {e}")))?;
        let dir = self
            .config_path
            .parent()
            .unwrap_or_else(|| Path::new("/etc/zeabur-installer"))
            .join("secrets");
        let mut files = Vec::new();
        for (component, values) in secrets {
            for (name, value) in values.into_iter().filter(|(_, v)| !v.is_empty()) {
                let path = dir.join(format!("{}.{name}", component.replace('/', "_")));
                config
                    .secret_files
                    .entry(component.clone())
                    .or_default()
                    .insert(name, path.display().to_string());
                files.push((path, value));
            }
        }
        config.check()?;
        Ok((config, files))
    }

    fn plan(&self, body: &[u8]) -> Response {
        let planned = self.config(body).and_then(|(config, _)| {
            let index = load::catalog(&config.sources)?;
            let plan = plan::plan(&index, &config, &facts::read()?)?;
            let yaml = serde_saphyr::to_string(&config)
                .map_err(|e| InstallError::Message(e.to_string()))?;
            Ok((plan, yaml))
        });
        match planned {
            Ok((plan, yaml)) => Response::json(
                200,
                &json!({"plan": plan, "text": render::plan(&plan), "yaml": yaml}),
            ),
            Err(e) => Response::json(200, &json!({"error": e.to_string()})),
        }
    }

    #[expect(
        clippy::significant_drop_tightening,
        reason = "the lock is held until the child is recorded, so two installs cannot start"
    )]
    fn apply(&self, body: &[u8]) -> Response {
        let mut job = self.job.lock().unwrap_or_else(PoisonError::into_inner);
        if job.running {
            return Response::json(409, &json!({"error": "an install is already running"}));
        }
        let started = self.config(body).and_then(|(config, secrets)| {
            for (path, value) in &secrets {
                files::write(path, 0o600, value.as_bytes())?;
            }
            let yaml = serde_saphyr::to_string(&config)
                .map_err(|e| InstallError::Message(e.to_string()))?;
            files::write(&self.config_path, 0o600, yaml.as_bytes())?;
            let exe = std::env::current_exe().map_err(|e| InstallError::Message(e.to_string()))?;
            // One pipe for both streams keeps the steps (stderr) and results (stdout) in order.
            let message = |e: io::Error| InstallError::Message(e.to_string());
            let (reader, writer) = io::pipe().map_err(message)?;
            let child = Command::new(exe)
                .arg("--config")
                .arg(&self.config_path)
                .arg("apply")
                .stdin(Stdio::null())
                .stdout(writer.try_clone().map_err(message)?)
                .stderr(writer)
                .spawn()
                .map_err(message)?;
            Ok((child, reader))
        });
        let (mut child, reader) = match started {
            Ok(started) => started,
            Err(e) => return Response::json(200, &json!({"error": e.to_string()})),
        };
        *job = Job {
            running: true,
            ..Job::default()
        };
        let pipe = follow(reader, Arc::clone(&self.job));
        let job_ref = Arc::clone(&self.job);
        thread::spawn(move || {
            let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
            drop(pipe.join());
            let mut job = job_ref.lock().unwrap_or_else(PoisonError::into_inner);
            job.running = false;
            job.code = Some(code);
        });
        Response::json(200, &json!({"started": true}))
    }

    fn progress(&self, from: usize) -> Response {
        let job = self.job.lock().unwrap_or_else(PoisonError::into_inner);
        let skip = from.saturating_sub(job.dropped);
        let lines: Vec<&String> = job.lines.iter().skip(skip).collect();
        Response::json(
            200,
            &json!({"lines": lines, "next": job.dropped + skip + lines.len(), "running": job.running, "code": job.code}),
        )
    }
}

/// Copies a child's output into the job's log, line by line.
fn follow(reader: impl Read + Send + 'static, job: Arc<Mutex<Job>>) -> JoinHandle<()> {
    thread::spawn(move || {
        for mut line in BufReader::new(reader).lines().map_while(Result::ok) {
            line.truncate(line.floor_char_boundary(MAX_LINE));
            let mut job = job.lock().unwrap_or_else(PoisonError::into_inner);
            if job.lines.len() >= MAX_LINES {
                job.lines.pop_front();
                job.dropped += 1;
            }
            job.lines.push_back(line);
        }
    })
}

/// The catalog, as the components page shows it.
fn catalog_json(index: &Index, profile: &str, arch: &str) -> Value {
    let components: Vec<Value> = index
        .components
        .iter()
        .map(|c| {
            let inputs: Vec<Value> = c
                .inputs
                .iter()
                .map(|i| {
                    json!({
                        "name": i.name,
                        "description": i.description,
                        "secret": i.secret.is_some(),
                        "generated": i.generate.is_some(),
                        "required": i.required,
                        "default": i.default,
                    })
                })
                .collect();
            json!({
                "id": c.id,
                "capability": c.capability,
                "displayName": c.metadata.name,
                "default": match c.default_for("single-node") {
                    Default::Recommended => "recommended",
                    Default::Optional => "optional",
                    Default::Unsupported => "unsupported",
                },
                "supported": c.supports(zeabur_installer_plan::select::DISTRIBUTION),
                "distributions": c.distributions,
                "needs": c
                    .install_order
                    .get(profile)
                    .into_iter()
                    .flatten()
                    .filter(|id| **id != c.id)
                    .collect::<Vec<_>>(),
                "idleMemoryMib": c.idle_memory_mib(profile, arch),
                "security": c.security.summary,
                "privileged": c.security.privileged,
                "inputs": inputs,
            })
        })
        .collect();
    json!({"components": components})
}

fn resolve(name: &str) -> Response {
    let found: Vec<String> = (name, 443).to_socket_addrs().map_or_else(
        |_| Vec::new(),
        |addrs| addrs.map(|a| a.ip().to_string()).collect(),
    );
    Response::json(200, &json!({"name": name, "addresses": found}))
}

fn asset(request: &Request, path: &str) -> Response {
    let wanted = match path.trim_start_matches('/') {
        "" => "index.html",
        other => other,
    };
    let Some(found) = ASSETS.iter().find(|a| a.path == wanted) else {
        return Response::new(404, "text/plain", "not found");
    };
    let accepts_br = request
        .header("accept-encoding")
        .is_some_and(|v| v.split(',').any(|e| e.trim().starts_with("br")));
    found.brotli.filter(|_| accepts_br).map_or_else(
        || Response::new(200, found.content_type, found.identity),
        |br| {
            Response::new(200, found.content_type, br)
                .with_header("Content-Encoding", "br".to_owned())
                .with_header("Vary", "Accept-Encoding".to_owned())
        },
    )
}

#[cfg(test)]
mod tests {
    use zeabur_installer_host::preflight::{Code, Topic};

    /// zh-TW and zh-CN are typed against English, so TypeScript rejects a key missing there.
    #[test]
    fn the_page_has_words_for_every_check() {
        let en = include_str!("../web/src/i18n/en.ts");
        let missing: Vec<String> = Topic::ALL
            .iter()
            .map(|topic| format!("check.{}", topic.key()))
            .chain(Code::ALL.iter().map(|code| format!("code.{}", code.key())))
            .filter(|key| !en.contains(&format!("\"{key}\":")))
            .collect();
        assert!(missing.is_empty(), "web/src/i18n/en.ts lacks {missing:?}");
    }
}
