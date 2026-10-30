//! `gui`: the same plan and `apply` behind a page in the browser, for people who would rather not
//! write YAML.
//!
//! It listens on the loopback address and is reached through `ssh -L`, so nothing new is exposed.
//! The page writes the same configuration file the command line reads and runs `apply` as a child
//! process, so both paths install exactly the same thing. A random token in the printed link
//! becomes a `SameSite=Strict` cookie; every request needs it, and requests with a `Host` other
//! than the loopback address are refused, which also stops DNS rebinding.
//!
//! The page's files are served by `zeabur_ui`, as Zeabur serves its own; the API answers are never
//! stored.

use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io::{self, BufRead as _, BufReader, Read};
use std::net::{SocketAddr, ToSocketAddrs as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::get;
use serde_json::{Value, json};
use tower::ServiceExt as _;
use zeabur_installer_host::{facts, files, preflight, systemd};
use zeabur_installer_plan::catalog::{Default, Index};
use zeabur_installer_plan::cluster::Distribution;
use zeabur_installer_plan::config::{Config, Document, PersonalConfig};
use zeabur_installer_plan::select::{self, PERSONAL_CAPABILITIES, PERSONAL_DISTRIBUTION};
use zeabur_installer_plan::{kubesolo, node_name, personal, plan};
use zeabur_ui::Asset;

use crate::{InstallError, apply, load, out, render};

/// The files of the web interface (`web/`), embedded by `build.rs`.
mod embedded {
    use zeabur_ui::{Asset, Decoded};
    include!(concat!(env!("OUT_DIR"), "/embedded.rs"));
}

const COOKIE: &str = "zeabur_installer";
/// The most connections the page serves at a time. The server closes other connections.
const MAX_CONNECTIONS: usize = 16;
/// The most bytes of a request's head.
const MAX_HEADER: usize = 16 * 1024;
/// The most bytes of a request's body.
const MAX_BODY: usize = 1024 * 1024;
/// How long a client may take to send a request's head, and then its body.
const READ_WAIT: Duration = Duration::from_secs(30);
/// The most lines of `apply` output a job keeps. The job drops the oldest lines first.
const MAX_LINES: usize = 10_000;
/// The most bytes a job keeps of one line.
const MAX_LINE: usize = 4096;

/// Content Security Policy of every response, in place of the one `zeabur_ui` gives Zeabur's
/// interface: the page talks to this server only. Base UI adds a `<style>` element at run time
/// (hiding scroll bars), hence `'unsafe-inline'` for styles; scripts stay limited to this origin,
/// and the build emits no inline script.
const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
img-src 'self'; font-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'self'; \
frame-ancestors 'none'; require-trusted-types-for 'script'; trusted-types 'none'";

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
    let message = |e: io::Error| InstallError::Message(e.to_string());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(message)?;
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind(listen))
        .map_err(|e| InstallError::Message(format!("{listen}: {e}")))?;
    let server = Arc::new(Server {
        token: files::random_hex()?.chars().take(32).collect(),
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
        let user = std::env::var("SUDO_USER")
            .ok()
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| "root".to_owned());
        let host = ssh_host(
            &facts::addresses(),
            facts::read().ok().map(|f| f.hostname).as_deref(),
        );
        out::line(&format!(
            "From your computer, forward the port first: ssh -L {0}:127.0.0.1:{0} {user}@{host}",
            listen.port()
        ));
    } else {
        out::err("warning: the page is served over plain HTTP; anyone on the path can read it.");
    }
    out::line("Stop with Ctrl-C once the install has finished.");
    runtime.block_on(serve(listener, app(server, &embedded::ASSETS)))
}

/// Serves `app` on `listener`, one request per connection and at most [`MAX_CONNECTIONS`]
/// connections at a time.
async fn serve(listener: tokio::net::TcpListener, app: Router) -> ! {
    let connections = Arc::new(tokio::sync::Semaphore::new(MAX_CONNECTIONS));
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        // Dropping the stream closes the connection.
        let Ok(permit) = Arc::clone(&connections).try_acquire_owned() else {
            continue;
        };
        let app = app.clone();
        tokio::spawn(async move {
            let service = hyper::service::service_fn(move |request: Request<_>| {
                app.clone().oneshot(request.map(Body::new))
            });
            let mut http = hyper::server::conn::http1::Builder::new();
            http.timer(hyper_util::rt::TokioTimer::new())
                .header_read_timeout(READ_WAIT)
                .max_buf_size(MAX_HEADER)
                .keep_alive(false);
            // The browser may have gone; nothing to do about it.
            drop(
                http.serve_connection(hyper_util::rt::TokioIo::new(stream), service)
                    .await,
            );
            drop(permit);
        });
    }
}

/// The page's API and files, behind the token and the `Host` check.
fn app(server: Arc<Server>, assets: &'static [Asset]) -> Router {
    Router::new()
        .route("/api/state", get(state))
        .route("/api/resolve", get(resolve))
        .route("/api/plan", axum::routing::post(plan))
        .route("/api/apply", get(progress).post(apply))
        .route("/api/local-ca.crt", get(local_ca))
        .fallback_service(zeabur_ui::router_for(assets))
        .layer(middleware::from_fn_with_state(Arc::clone(&server), gate))
        .layer(middleware::map_response(harden))
        .with_state(server)
}

async fn gate(State(server): State<Arc<Server>>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    if server.loopback && !server.allowed_host(host.unwrap_or_default()) {
        return text(StatusCode::FORBIDDEN, "this page answers on 127.0.0.1 only");
    }
    let query = request.uri().query().unwrap_or_default();
    if request.method() == Method::GET
        && request.uri().path() == "/"
        && let Some(token) = query_param(query, "token")
    {
        if !server.token_matches(Some(&token)) {
            return text(
                StatusCode::FORBIDDEN,
                "this link is not valid; use the one printed in the terminal",
            );
        }
        let mut response = text(StatusCode::FOUND, "");
        let h = response.headers_mut();
        h.insert(header::LOCATION, HeaderValue::from_static("/"));
        if let Ok(cookie) = HeaderValue::from_str(&format!(
            "{COOKIE}={}; HttpOnly; SameSite=Strict; Path=/",
            server.token
        )) {
            h.insert(header::SET_COOKIE, cookie);
        }
        return response;
    }
    if !server.token_matches(cookie(headers, COOKIE)) {
        return text(
            StatusCode::FORBIDDEN,
            "open the link printed in the terminal",
        );
    }
    if request.method() == Method::POST
        && headers.get(header::CONTENT_TYPE) != Some(&HeaderValue::from_static("application/json"))
    {
        return text(StatusCode::BAD_REQUEST, "JSON only");
    }
    next.run(request).await
}

/// API answers are never stored; the page's files keep `zeabur_ui`'s caching.
async fn harden(mut response: Response) -> Response {
    let h = response.headers_mut();
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CSP),
    );
    for (name, value) in [
        (header::CACHE_CONTROL, "no-store"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "no-referrer"),
    ] {
        h.entry(name).or_insert(HeaderValue::from_static(value));
    }
    response
}

async fn state(State(server): State<Arc<Server>>) -> Response {
    blocking(move || server.state()).await
}

async fn resolve(request: Request) -> Response {
    let name = query_param(request.uri().query().unwrap_or_default(), "name").unwrap_or_default();
    blocking(move || resolve_name(&name)).await
}

async fn plan(State(server): State<Arc<Server>>, body: Body) -> Response {
    match read_body(body).await {
        Ok(body) => blocking(move || server.plan(&body)).await,
        Err((status, why)) => text(status, why),
    }
}

async fn apply(State(server): State<Arc<Server>>, body: Body) -> Response {
    match read_body(body).await {
        Ok(body) => blocking(move || server.apply(&body)).await,
        Err((status, why)) => text(status, why),
    }
}

async fn progress(State(server): State<Arc<Server>>, request: Request) -> Response {
    let from = query_param(request.uri().query().unwrap_or_default(), "from")
        .and_then(|f| f.parse().ok())
        .unwrap_or(0);
    server.progress(from)
}

async fn local_ca() -> Response {
    blocking(|| {
        fs::read(Path::new(apply::STATE_DIR).join("local-ca.crt")).map_or_else(
            |_| text(StatusCode::NOT_FOUND, "no local CA yet"),
            |pem| {
                let mut response = Response::new(Body::from(pem));
                let h = response.headers_mut();
                h.insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/x-x509-ca-cert"),
                );
                h.insert(
                    header::CONTENT_DISPOSITION,
                    HeaderValue::from_static("attachment; filename=\"zeabur-local-ca.crt\""),
                );
                response
            },
        )
    })
    .await
}

/// Runs `answer` where it may block: it reads files, runs checks and resolves names.
async fn blocking(answer: impl FnOnce() -> Response + Send + 'static) -> Response {
    tokio::task::spawn_blocking(answer)
        .await
        .unwrap_or_else(|_| text(StatusCode::INTERNAL_SERVER_ERROR, "the request failed"))
}

/// The body, up to [`MAX_BODY`] bytes and [`READ_WAIT`] long.
async fn read_body(body: Body) -> Result<Bytes, (StatusCode, &'static str)> {
    match tokio::time::timeout(READ_WAIT, axum::body::to_bytes(body, MAX_BODY)).await {
        Ok(Ok(body)) => Ok(body),
        Ok(Err(_)) => Err((StatusCode::PAYLOAD_TOO_LARGE, "the request is over 1 MiB")),
        Err(_) => Err((StatusCode::REQUEST_TIMEOUT, "the request took too long")),
    }
}

fn text(status: StatusCode, body: &'static str) -> Response {
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

fn json_response(status: StatusCode, value: &Value) -> Response {
    let mut response = Response::new(Body::from(value.to_string()));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

fn query_param(query: &str, name: &str) -> Option<String> {
    form_urlencoded::parse(query.as_bytes())
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.into_owned())
}

fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|pair| {
            let (k, v) = pair.trim().split_once('=')?;
            (k == name).then_some(v)
        })
}

impl Server {
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
            .and_then(|text| Document::parse(&text).ok());
        let config_exists = existing.is_some();
        let addresses = facts::addresses();
        let (team, personal) = match existing {
            Some(Document::Install(config)) => (Some(config), None),
            Some(Document::Personal(config)) => (None, Some(config)),
            Some(Document::Join(_)) | None => (None, None),
        };
        let chosen_use = if personal.is_some() {
            "personal"
        } else {
            "team"
        };
        let mut team = team.or_else(|| Config::defaults(&addresses));
        let mut personal = personal.unwrap_or_else(PersonalConfig::defaults);
        let host = facts::read().ok();
        if let Some(f) = &host {
            // Written into the saved configuration, so apply uses the name the page showed.
            for configured in team
                .iter_mut()
                .map(|c| &mut c.host.node_name)
                .chain([&mut personal.host.node_name])
            {
                if let Ok(name) = node_name::choose(configured.as_deref(), f, &[]) {
                    *configured = Some(name);
                }
            }
        }
        let facts = host.as_ref().map_or(Value::Null, |f| json!(f));
        let checks: serde_json::Map<String, Value> = Distribution::ALL
            .into_iter()
            .map(|d| {
                let checks = preflight::run(preflight::Options {
                    cluster_running: systemd::is_active(d.server_unit()),
                    require_encrypted_disk: team
                        .as_ref()
                        .is_some_and(|c| c.host.require_encrypted_disk),
                    node: preflight::Node::Server(d),
                });
                (d.id().to_owned(), json!(checks))
            })
            .collect();
        let min_memory: serde_json::Map<String, Value> = Distribution::ALL
            .into_iter()
            .map(|d| (d.id().to_owned(), json!(d.min_memory_mib())))
            .collect();
        let personal_checks = preflight::run(preflight::Options {
            cluster_running: systemd::is_active(kubesolo::UNIT),
            require_encrypted_disk: personal.host.require_encrypted_disk,
            node: preflight::Node::KubeSolo,
        });
        let source = team
            .as_ref()
            .and_then(|c| c.sources.catalog.clone())
            .or_else(|| personal.sources.catalog.clone());
        let (catalog, catalog_error) = match load::catalog(source.as_deref(), apply::STATE_DIR) {
            Ok(index) => (
                catalog_json(
                    &index,
                    team.as_ref().map_or("single-node", |c| c.profile.as_str()),
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
        json_response(
            StatusCode::OK,
            &json!({
                "facts": facts,
                "checks": checks,
                "personalChecks": personal_checks,
                "addresses": addresses,
                "use": chosen_use,
                "config": team,
                "personal": personal,
                "teamMinMemoryMib": min_memory,
                "rke2SuggestedMib": plan::RKE2_SUGGESTED_MIB,
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
    fn config(&self, body: &[u8]) -> Result<(Chosen, Vec<(PathBuf, String)>), InstallError> {
        let mut input: Value =
            serde_json::from_slice(body).map_err(|e| InstallError::Message(e.to_string()))?;
        let secrets: BTreeMap<String, BTreeMap<String, String>> = serde_json::from_value(
            input
                .get_mut("secrets")
                .map_or_else(|| json!({}), Value::take),
        )
        .map_err(|e| InstallError::Message(e.to_string()))?;
        let value = input.get_mut("config").map_or(Value::Null, Value::take);
        let parse = |e: serde_json::Error| InstallError::Message(format!("configuration: {e}"));
        let mut config = if value.get("use").and_then(Value::as_str) == Some("personal") {
            Chosen::Personal(serde_json::from_value(value).map_err(parse)?)
        } else {
            Chosen::Team(Box::new(serde_json::from_value(value).map_err(parse)?))
        };
        let dir = self
            .config_path
            .parent()
            .unwrap_or_else(|| Path::new("/etc/zeabur-installer"))
            .join("secrets");
        let mut files = Vec::new();
        for (component, mut values) in secrets {
            if component == VELERO {
                velero_credentials(&mut values)?;
            }
            for (name, value) in values.into_iter().filter(|(_, v)| !v.is_empty()) {
                let path = dir.join(format!("{}.{name}", component.replace('/', "_")));
                config
                    .secret_files()
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
            let (plan, text) = match &config {
                Chosen::Team(config) => {
                    let index = load::catalog(config.sources.catalog.as_deref(), apply::STATE_DIR)?;
                    let plan = apply::make_plan(&index, config)?;
                    (json!(plan), render::plan(&plan))
                }
                Chosen::Personal(config) => {
                    let plan = crate::personal::make_plan(config)?;
                    (json!(plan), render::personal_plan(&plan))
                }
            };
            Ok((plan, text, config.yaml()?))
        });
        match planned {
            Ok((plan, text, yaml)) => json_response(
                StatusCode::OK,
                &json!({"plan": plan, "text": text, "yaml": yaml}),
            ),
            Err(e) => json_response(StatusCode::OK, &json!({"error": e.to_string()})),
        }
    }

    #[expect(
        clippy::significant_drop_tightening,
        reason = "the lock is held until the child is recorded, so two installs cannot start"
    )]
    fn apply(&self, body: &[u8]) -> Response {
        let mut job = self.job.lock().unwrap_or_else(PoisonError::into_inner);
        if job.running {
            return json_response(
                StatusCode::CONFLICT,
                &json!({"error": "an install is already running"}),
            );
        }
        let started = self.config(body).and_then(|(config, secrets)| {
            for (path, value) in &secrets {
                files::write(path, 0o600, value.as_bytes())?;
            }
            files::write(&self.config_path, 0o600, config.yaml()?.as_bytes())?;
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
            Err(e) => return json_response(StatusCode::OK, &json!({"error": e.to_string()})),
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
        json_response(StatusCode::OK, &json!({"started": true}))
    }

    fn progress(&self, from: usize) -> Response {
        let job = self.job.lock().unwrap_or_else(PoisonError::into_inner);
        let skip = from.saturating_sub(job.dropped);
        let lines: Vec<&String> = job.lines.iter().skip(skip).collect();
        json_response(
            StatusCode::OK,
            &json!({"lines": lines, "next": job.dropped + skip + lines.len(), "running": job.running, "code": job.code}),
        )
    }
}

/// The configuration of either kind of install the page sends.
enum Chosen {
    Team(Box<Config>),
    Personal(PersonalConfig),
}

impl Chosen {
    const fn secret_files(&mut self) -> &mut BTreeMap<String, BTreeMap<String, String>> {
        match self {
            Self::Team(config) => &mut config.secret_files,
            Self::Personal(config) => &mut config.secret_files,
        }
    }

    fn check(&self) -> Result<(), InstallError> {
        match self {
            Self::Team(config) => config.check()?,
            Self::Personal(config) => config.check()?,
        }
        Ok(())
    }

    fn yaml(&self) -> Result<String, InstallError> {
        match self {
            Self::Team(config) => serde_saphyr::to_string(config),
            Self::Personal(config) => serde_saphyr::to_string(config),
        }
        .map_err(|e| InstallError::Message(e.to_string()))
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

/// The address that `ssh` reaches this server at: the first public address, else the first
/// address, else the host name.
fn ssh_host(addresses: &[String], hostname: Option<&str>) -> String {
    addresses
        .iter()
        .find(|a| plan::is_public(a))
        .or_else(|| addresses.first())
        .cloned()
        .or_else(|| hostname.filter(|h| !h.is_empty()).map(str::to_owned))
        .unwrap_or_else(|| "<this server>".to_owned())
}

const VELERO: &str = "volume-backup/velero";

/// Replaces the two S3 keys that the page asks for with Velero's `credentials` input, an AWS
/// credentials file.
fn velero_credentials(values: &mut BTreeMap<String, String>) -> Result<(), InstallError> {
    let id = values.remove("access-key-id").unwrap_or_default();
    let key = values.remove("secret-access-key").unwrap_or_default();
    let (id, key) = (id.trim(), key.trim());
    if id.is_empty() && key.is_empty() {
        return Ok(());
    }
    if id.is_empty() || key.is_empty() {
        return Err(InstallError::Message(
            "Velero needs both the access key ID and the secret access key".to_owned(),
        ));
    }
    if [id, key].iter().any(|k| k.chars().any(char::is_whitespace)) {
        return Err(InstallError::Message(
            "Velero's access keys cannot contain spaces or line breaks".to_owned(),
        ));
    }
    values.insert(
        "credentials".to_owned(),
        format!("[default]\naws_access_key_id={id}\naws_secret_access_key={key}\n"),
    );
    Ok(())
}

/// The catalog, as the components page shows it.
fn catalog_json(index: &Index, profile: &str, arch: &str) -> Value {
    let required = select::required(index, profile);
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
                        "filled": plan::installer_fills(i),
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
                "personal": c.supports(PERSONAL_DISTRIBUTION)
                    && PERSONAL_CAPABILITIES.contains(&c.capability.as_str()),
                "distributions": c.distributions,
                "personalMinMemoryMib": personal::min_memory_mib(c, arch),
                "required": required.contains(&c.id),
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

fn resolve_name(name: &str) -> Response {
    let found: Vec<String> = (name, 443).to_socket_addrs().map_or_else(
        |_| Vec::new(),
        |addrs| addrs.map(|a| a.ip().to_string()).collect(),
    );
    json_response(StatusCode::OK, &json!({"name": name, "addresses": found}))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::Path;

    use serde_json::{Value, json};
    use zeabur_installer_host::preflight::{Code, Topic};
    use zeabur_installer_plan::catalog::Index;
    use zeabur_installer_plan::config::{Config, PersonalConfig};
    use zeabur_installer_plan::plan::{self, Facts};
    use zeabur_installer_plan::{personal, pins};

    use super::{catalog_json, ssh_host, velero_credentials};
    use crate::render;

    mod server {
        use std::path::PathBuf;
        use std::sync::Arc;
        use std::time::Duration;

        use axum::body::{Body, to_bytes};
        use axum::http::{Request, StatusCode};
        use axum::response::Response;
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        use tower::ServiceExt as _;
        use zeabur_ui::{Asset, Decoded};

        use super::super::{CSP, MAX_BODY, MAX_CONNECTIONS, Server, app, serve};

        const TOKEN: &str = "0123456789abcdef0123456789abcdef";
        const COOKIE: &str = "zeabur_installer=0123456789abcdef0123456789abcdef";
        const HOST: &str = "127.0.0.1:8085";

        /// `console.log(1)` in brotli.
        const SCRIPT_BR: &[u8] = &[
            0x8B, 0x06, 0x80, 0x63, 0x6F, 0x6E, 0x73, 0x6F, 0x6C, 0x65, 0x2E, 0x6C, 0x6F, 0x67,
            0x28, 0x31, 0x29, 0x03,
        ];

        static ASSETS: [Asset; 2] = [
            Asset {
                path: "index.html",
                content_type: "text/html; charset=utf-8",
                etag: "\"aaaa\"",
                immutable: false,
                len: 18,
                identity: Some(b"<html>index</html>"),
                brotli: Some(b"BR-INDEX"),
                decoded: Decoded::new(),
            },
            Asset {
                path: "assets/index-3f2a.js",
                content_type: "text/javascript; charset=utf-8",
                etag: "\"bbbb\"",
                immutable: true,
                len: 14,
                identity: None,
                brotli: Some(SCRIPT_BR),
                decoded: Decoded::new(),
            },
        ];

        fn server(loopback: bool) -> Arc<Server> {
            Arc::new(Server {
                token: TOKEN.to_owned(),
                port: 8085,
                loopback,
                config_path: PathBuf::from("/nonexistent/zeabur-installer.yaml"),
                job: Arc::default(),
            })
        }

        async fn send(
            loopback: bool,
            method: &str,
            uri: &str,
            headers: &[(&str, &str)],
            body: impl Into<Body>,
        ) -> Response {
            let mut request = Request::builder().method(method).uri(uri);
            for (name, value) in headers {
                request = request.header(*name, *value);
            }
            app(server(loopback), &ASSETS)
                .oneshot(request.body(body.into()).unwrap())
                .await
                .unwrap()
        }

        async fn get(uri: &str, headers: &[(&str, &str)]) -> Response {
            send(true, "GET", uri, headers, Body::empty()).await
        }

        fn header<'a>(response: &'a Response, name: &str) -> Option<&'a str> {
            response.headers().get(name).and_then(|v| v.to_str().ok())
        }

        async fn body(response: Response) -> Vec<u8> {
            to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec()
        }

        #[tokio::test]
        async fn the_printed_link_becomes_a_strict_cookie() {
            let response = get(&format!("/?token={TOKEN}"), &[("host", HOST)]).await;
            assert_eq!(response.status(), StatusCode::FOUND);
            assert_eq!(header(&response, "location"), Some("/"));
            assert_eq!(
                header(&response, "set-cookie"),
                Some(format!("{COOKIE}; HttpOnly; SameSite=Strict; Path=/").as_str())
            );

            let response = get("/", &[("host", HOST), ("cookie", COOKIE)]).await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(body(response).await, b"<html>index</html>");
        }

        #[tokio::test]
        async fn a_link_with_another_token_is_refused() {
            for token in ["", "0123", &TOKEN.replace('0', "1")] {
                let response = get(&format!("/?token={token}"), &[("host", HOST)]).await;
                assert_eq!(response.status(), StatusCode::FORBIDDEN, "{token}");
                assert_eq!(header(&response, "set-cookie"), None);
            }
        }

        #[tokio::test]
        async fn nothing_is_served_without_the_cookie() {
            let wrong = format!("zeabur_installer={}", TOKEN.replace('a', "b"));
            for path in ["/", "/assets/index-3f2a.js", "/api/state", "/api/apply"] {
                for cookie in [None, Some(wrong.as_str()), Some("other=x")] {
                    let mut headers = vec![("host", HOST)];
                    headers.extend(cookie.map(|c| ("cookie", c)));
                    let response = get(path, &headers).await;
                    assert_eq!(
                        response.status(),
                        StatusCode::FORBIDDEN,
                        "{path} {cookie:?}"
                    );
                }
            }
            let response = get(
                "/api/apply",
                &[("host", HOST), ("cookie", &format!("theme=dark; {COOKIE}"))],
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
        }

        #[tokio::test]
        async fn a_host_other_than_the_loopback_address_is_refused() {
            for host in [
                None,
                Some("evil.example:8085"),
                Some("127.0.0.1:9999"),
                Some("127.0.0.1"),
            ] {
                let mut headers = vec![("cookie", COOKIE)];
                headers.extend(host.map(|h| ("host", h)));
                let response = get("/", &headers).await;
                assert_eq!(response.status(), StatusCode::FORBIDDEN, "{host:?}");
                let response = get(&format!("/?token={TOKEN}"), &headers[1..]).await;
                assert_eq!(response.status(), StatusCode::FORBIDDEN, "{host:?}");
            }
            let response = get("/", &[("host", "localhost:8085"), ("cookie", COOKIE)]).await;
            assert_eq!(response.status(), StatusCode::OK);
        }

        #[tokio::test]
        async fn a_server_on_another_address_takes_any_host() {
            let response = send(
                false,
                "GET",
                "/",
                &[("host", "203.0.113.7:8085"), ("cookie", COOKIE)],
                Body::empty(),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
        }

        #[tokio::test]
        async fn a_post_must_be_json() {
            let response = send(
                true,
                "POST",
                "/api/plan",
                &[
                    ("host", HOST),
                    ("cookie", COOKIE),
                    ("content-type", "text/plain"),
                ],
                "{}",
            )
            .await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }

        #[tokio::test]
        async fn a_body_over_1_mib_is_refused() {
            let headers = [
                ("host", HOST),
                ("cookie", COOKIE),
                ("content-type", "application/json"),
            ];
            let mut at_limit = vec![b' '; MAX_BODY - 2];
            at_limit.extend_from_slice(b"{}");
            let response = send(true, "POST", "/api/plan", &headers, at_limit).await;
            assert_eq!(response.status(), StatusCode::OK);
            let answer: serde_json::Value = serde_json::from_slice(&body(response).await).unwrap();
            assert!(answer["error"].is_string(), "{answer}");

            for path in ["/api/plan", "/api/apply"] {
                let response = send(true, "POST", path, &headers, vec![b' '; MAX_BODY + 1]).await;
                assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE, "{path}");
            }
        }

        #[tokio::test]
        async fn brotli_only_to_a_client_that_accepts_it() {
            let page = [("host", HOST), ("cookie", COOKIE)];
            for accept in ["br", "gzip, br", "br;q=0.5"] {
                let response = get("/", &[page[0], page[1], ("accept-encoding", accept)]).await;
                assert_eq!(
                    header(&response, "content-encoding"),
                    Some("br"),
                    "{accept}"
                );
                assert_eq!(body(response).await, b"BR-INDEX");
            }
            for accept in [None, Some("gzip"), Some("br;q=0"), Some("gzip, br;q=0")] {
                let mut headers = page.to_vec();
                headers.extend(accept.map(|a| ("accept-encoding", a)));
                let response = get("/", &headers).await;
                assert_eq!(header(&response, "content-encoding"), None, "{accept:?}");
                assert_eq!(body(response).await, b"<html>index</html>");
                let response = get("/assets/index-3f2a.js", &headers).await;
                assert_eq!(header(&response, "content-encoding"), None, "{accept:?}");
                assert_eq!(body(response).await, b"console.log(1)");
            }
        }

        #[tokio::test]
        async fn api_answers_are_never_stored_and_page_files_are_cached() {
            let headers = [("host", HOST), ("cookie", COOKIE)];
            for path in ["/api/apply", "/api/nothing"] {
                let response = get(path, &headers).await;
                assert_eq!(
                    header(&response, "cache-control"),
                    Some("no-store"),
                    "{path}"
                );
                assert_eq!(header(&response, "x-content-type-options"), Some("nosniff"));
                assert_eq!(header(&response, "referrer-policy"), Some("no-referrer"));
                assert_eq!(header(&response, "content-security-policy"), Some(CSP));
            }
            let refused = get("/", &[("host", HOST)]).await;
            assert_eq!(header(&refused, "cache-control"), Some("no-store"));

            let response = get("/", &headers).await;
            assert_eq!(header(&response, "cache-control"), Some("no-cache"));
            assert_eq!(header(&response, "etag"), Some("\"aaaa\""));
            assert_eq!(header(&response, "content-security-policy"), Some(CSP));
            assert_eq!(header(&response, "x-frame-options"), Some("DENY"));

            let response = get("/assets/index-3f2a.js", &headers).await;
            assert_eq!(
                header(&response, "cache-control"),
                Some("public, max-age=31536000, immutable")
            );
        }

        /// Scripts load from this origin only, so the page may carry no inline script.
        #[test]
        fn the_page_needs_nothing_the_policy_refuses() {
            assert!(CSP.contains("script-src 'self';"), "{CSP}");
            let index = include_str!("../web/index.html");
            assert!(!index.contains("<style"), "web/index.html");
            assert!(
                index
                    .split("<script")
                    .skip(1)
                    .all(|tag| tag.starts_with(" type=\"module\" src=")),
                "web/index.html has an inline script"
            );
        }

        #[tokio::test]
        async fn connections_beyond_the_limit_are_closed() {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            tokio::spawn(async move { serve(listener, app(server(false), &ASSETS)).await });

            let mut held = Vec::new();
            for _ in 0..MAX_CONNECTIONS {
                held.push(tokio::net::TcpStream::connect(address).await.unwrap());
            }
            let mut extra = tokio::net::TcpStream::connect(address).await.unwrap();
            let mut answer = Vec::new();
            let read = tokio::time::timeout(Duration::from_secs(5), extra.read_to_end(&mut answer))
                .await
                .unwrap();
            assert!(read.is_err() || answer.is_empty(), "{answer:?}");

            let mut first = held.swap_remove(0);
            first
                .write_all(b"GET /api/state HTTP/1.1\r\nhost: x\r\n\r\n")
                .await
                .unwrap();
            let mut answer = Vec::new();
            tokio::time::timeout(Duration::from_secs(5), first.read_to_end(&mut answer))
                .await
                .unwrap()
                .unwrap();
            let answer = String::from_utf8_lossy(&answer);
            assert!(answer.starts_with("HTTP/1.1 403"), "{answer}");
            assert!(
                answer.to_ascii_lowercase().contains("connection: close"),
                "{answer}"
            );
        }
    }

    #[test]
    fn velero_gets_a_credentials_file_from_two_keys() {
        let keys = |id: &str, key: &str| {
            let mut values = BTreeMap::from([
                ("access-key-id".to_owned(), id.to_owned()),
                ("secret-access-key".to_owned(), key.to_owned()),
            ]);
            velero_credentials(&mut values).map(|()| values)
        };
        let values = keys(" AKIA1 ", "s3cret").unwrap();
        assert_eq!(
            values.get("credentials").map(String::as_str),
            Some("[default]\naws_access_key_id=AKIA1\naws_secret_access_key=s3cret\n")
        );
        assert!(!values.contains_key("access-key-id"));
        assert!(keys("", "").unwrap().is_empty());
        assert!(keys("AKIA1", "").is_err());
        assert!(keys("AKIA1", "a\naws_session_token=x").is_err());
    }

    #[test]
    fn the_ssh_hint_names_this_server() {
        let addresses = ["10.0.0.5".to_owned(), "203.0.114.7".to_owned()];
        assert_eq!(ssh_host(&addresses, Some("vps-1")), "203.0.114.7");
        assert_eq!(ssh_host(&addresses[..1], Some("vps-1")), "10.0.0.5");
        assert_eq!(ssh_host(&[], Some("vps-1")), "vps-1");
    }

    /// The captured responses that `bun run dev` answers with follow the pinned catalog. After a
    /// catalog bump, `ZEABUR_UPDATE_GOLDEN=1 cargo nextest run -p zeabur-installer fixtures`
    /// rewrites them, and `bun run fmt` in web/ formats them.
    #[test]
    fn the_web_fixtures_follow_the_catalog() {
        let index = Index::parse(include_bytes!(concat!(
            env!("ZEABUR_CATALOG_RELEASE"),
            "/index.json"
        )))
        .unwrap();
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("web/scripts/fixtures");
        let read = |name: &str| -> Value {
            serde_json::from_str(&fs::read_to_string(dir.join(name)).unwrap()).unwrap()
        };
        let mut expected = BTreeMap::new();

        let mut state = read("state.json");
        state["catalog"] = catalog_json(
            &index,
            state["config"]["profile"].as_str().unwrap(),
            state["facts"]["arch"].as_str().unwrap(),
        );
        expected.insert("state.json", state);

        let mut team = read("plan.json");
        let config = Config::parse(team["yaml"].as_str().unwrap()).unwrap();
        let facts: Facts = serde_json::from_value(team["plan"]["facts"].clone()).unwrap();
        let fake = "a".repeat(64).leak();
        let image = |repository: &str| format!("ghcr.io/zeabur-oss/{repository}@sha256:{fake}");
        let products = pins::Products {
            catalog: pins::Catalog {
                version: "v0.0.1",
                index_sha256: fake,
            },
            identity: pins::Identity {
                version: "v0.0.1",
                image: image("zeabur-identity").leak(),
                identityctl_amd64: fake,
                identityctl_arm64: fake,
            },
            zeabur: pins::Zeabur {
                version: "v0.0.1",
                image: image("zeabur").leak(),
                chart_sha256: fake,
            },
        };
        let plan = plan::plan_with(&index, &config, &facts, Some(&products)).unwrap();
        team["text"] = json!(render::plan(&plan));
        team["plan"] = json!(plan);
        expected.insert("plan.json", team);

        for name in ["personal-plan.json", "personal-builds-plan.json"] {
            let mut fixture = read(name);
            let config = PersonalConfig::parse(fixture["yaml"].as_str().unwrap()).unwrap();
            let facts: Facts = serde_json::from_value(fixture["plan"]["facts"].clone()).unwrap();
            let plan = personal::plan(&index, &config, &facts).unwrap();
            fixture["text"] = json!(render::personal_plan(&plan));
            fixture["plan"] = json!(plan);
            expected.insert(name, fixture);
        }

        let update = std::env::var_os("ZEABUR_UPDATE_GOLDEN").is_some();
        let mut drifted = Vec::new();
        for (name, value) in expected {
            if update {
                let text = serde_json::to_string_pretty(&value).unwrap() + "\n";
                fs::write(dir.join(name), text).unwrap();
            } else if read(name) != value {
                drifted.push(name);
            }
        }
        assert!(
            drifted.is_empty(),
            "{drifted:?} differ from what this catalog gives: ZEABUR_UPDATE_GOLDEN=1 cargo nextest \
             run -p zeabur-installer fixtures, then bun run fmt in crates/zeabur-installer/web"
        );
    }

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

    /// The configuration the page writes reads back as the same configuration.
    #[test]
    fn a_written_configuration_reads_back_the_same() {
        let strings = [
            "yes",
            "on",
            "no",
            "off",
            "0123",
            "1e3",
            "1:20",
            ":",
            "#",
            "a #b",
            "a: b",
            "true",
            "null",
            "~",
            "",
            "- x",
            "2024-01-01",
            " lead",
            "trail ",
            "multi\nline\n",
            "\"q\"",
            "'q'",
        ];
        let mut config = PersonalConfig::defaults();
        config.enable = strings.iter().map(|s| (*s).to_owned()).collect();
        config.inputs.insert(
            "comp/id".to_owned(),
            strings
                .iter()
                .map(|s| ((*s).to_owned(), serde_json::json!(s)))
                .collect(),
        );
        let yaml = serde_saphyr::to_string(&config).unwrap();
        let back: PersonalConfig = serde_saphyr::from_str(&yaml).unwrap();
        assert_eq!(back, config, "{yaml}");
    }
}
