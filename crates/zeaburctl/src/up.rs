//! `zeaburctl up`: uploads a folder to a service that builds one, and makes the service when it
//! does not exist.
//!
//! The folder is packed (`.gitignore` and `.dockerignore` applied, see [`crate::pack`]), then the
//! steps of the web interface follow: a plan asks the `Build` for an upload token, the API pushes
//! the folder to the registry with it, and a second plan names the pushed digest in the `Build`.
//! The writes are plans, applied at once because this command is the confirmation.
//!
//! The API is the Zeabur server of the cluster ([`Server`]) when the kubeconfig signs in with a
//! token from Zeabur Identity. Otherwise it runs in this process ([`InProcess`]), as in
//! `zeaburctl ui`: the credentials stay on this computer, and the folder goes to the registry
//! from here, through a port-forward when only the cluster reaches it ([`forward`]).

use std::io::IsTerminal as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use secrecy::ExposeSecret as _;
use serde_json::{Value, json};
use zeabur_kube::k8s_openapi::api::core::v1::ConfigMap;
use zeabur_kube::{Cluster, UserClient, UserToken, ZeaburInfo};

use crate::forward::{self, ServiceAddress};
use crate::in_process::InProcess;
use crate::server::Server;
use crate::{output, pack};

/// How long the controller may take to answer an upload token request.
const TOKEN_WAIT: Duration = Duration::from_secs(60);
/// The longest service name.
const NAME_MAX: usize = 40;
const NO_BUILDER: &str = "Zeabur Builder is not installed in this cluster, so it cannot build a folder. Nothing was uploaded. A personal install has no Zeabur Builder: deploy an image instead.";

/// How a new service is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum Method {
    /// Zeabur Builder picks the language.
    Railpack,
    /// The Dockerfile at the top of the folder.
    Dockerfile,
}

/// Options of `zeaburctl up`.
#[derive(Debug, clap::Args)]
pub(crate) struct UpArgs {
    /// The folder to upload (default: the current one).
    #[arg(default_value = ".")]
    folder: PathBuf,
    /// The address of Zeabur, like `https://zeabur.example.com`. Default: the one Zeabur
    /// publishes in the cluster (the ConfigMap `zeabur-info` in `kube-public`).
    #[arg(long, env = "ZEABUR_SERVER")]
    server: Option<String>,
    /// The project the service is in. Asked for in a terminal when left out.
    #[arg(long)]
    project: Option<String>,
    /// The service, which has a Build of the same name. Asked for in a terminal when left out. It
    /// is made when it does not exist.
    #[arg(long)]
    service: Option<String>,
    /// The kubeconfig context to use (default: the current one).
    #[arg(long)]
    context: Option<String>,
    /// For a new service: how it is built.
    #[arg(long, value_enum, default_value_t = Method::Railpack)]
    method: Method,
    /// For a new service: the port the app listens on. Without one the service takes no
    /// connections.
    #[arg(long)]
    port: Option<u16>,
    /// Make a missing service without asking.
    #[arg(short, long)]
    yes: bool,
}

pub(crate) fn run(args: &UpArgs) -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            output::error(&format!("cannot start: {error}"));
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(up(args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            output::error(&message);
            ExitCode::FAILURE
        }
    }
}

/// Asks the person a question and gives the answer; `None` where nobody can be asked.
type Ask<'a> = Option<&'a mut (dyn FnMut(&str) -> Option<String> + Send)>;

/// What the person chose with the options, for a service that may be new.
#[derive(Debug, Clone)]
struct Target {
    project: Option<String>,
    service: Option<String>,
    method: Method,
    port: Option<u16>,
    yes: bool,
    /// The folder's name, for a suggested service name.
    folder_name: String,
}

async fn up(args: &UpArgs) -> Result<(), String> {
    let packed = pack::pack(&args.folder).map_err(|e| e.to_string())?;
    output::info(&format!(
        "Packed {} files, {} KiB.",
        packed.files,
        packed.bytes.len() >> 10
    ));
    if packed.skipped > 0 {
        output::info(&format!(
            "Left out {} special files that cannot go in the archive.",
            packed.skipped
        ));
    }
    let client = UserClient::from_kubeconfig(args.context.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    // A cluster without Zeabur has no zeabur-info.
    let info = client.zeabur_info().await.unwrap_or_default();
    let token = UserToken::from_kubeconfig(args.context.as_deref())
        .await
        .ok();
    let api = match mode(token, info.issuer.as_deref()) {
        Mode::Server(token) => {
            let address = match (&args.server, &info.url) {
                (Some(address), _) => address.clone(),
                (None, Some(url)) => url.clone(),
                (None, None) => {
                    return Err("This cluster does not say where Zeabur is. Pass --server with the address you open Zeabur at.".to_owned());
                }
            };
            output::info(&format!(
                "Server mode: through Zeabur at {address}, signed in with Zeabur Identity."
            ));
            Zeabur::Server(Server::new(&address, token)?)
        }
        Mode::Local => {
            output::info(
                "Local mode: zeaburctl acts with your kubeconfig, and your credentials stay on this computer.",
            );
            local(client, &info).await?
        }
    };
    let target = Target {
        project: args.project.clone(),
        service: args.service.clone(),
        method: args.method,
        port: args.port,
        yes: args.yes,
        folder_name: folder_name(&args.folder),
    };
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let interactive = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    let mut ask = |question: &str| output::ask(question);
    let ask: Ask<'_> = interactive.then_some(&mut ask);
    let service = deploy(
        &api,
        &target,
        packed.bytes,
        &format!("up-{nanos}"),
        Timing {
            poll: Duration::from_secs(1),
            wait: TOKEN_WAIT,
        },
        ask,
    )
    .await?;
    output::info(&format!("Uploaded. {service} is building the new folder."));
    Ok(())
}

/// Where the API runs.
#[derive(Debug)]
enum Mode {
    /// On the Zeabur server, which gets the token.
    Server(UserToken),
    /// In this process.
    Local,
}

/// The server gets the kubeconfig's token only when Zeabur Identity issued it, as `zeabur-info`
/// names the issuer. Another credential (a client certificate, a cloud provider's token, a
/// ServiceAccount's, a static one) is of no use to Zeabur, and Zeabur must not get it.
fn mode(token: Option<UserToken>, issuer: Option<&str>) -> Mode {
    match (token, issuer) {
        (Some(token), Some(issuer))
            if zeabur_kube::issued_by(token.secret().expose_secret(), issuer) =>
        {
            Mode::Server(token)
        }
        _ => Mode::Local,
    }
}

/// The API in this process, acting with the kubeconfig. The registry Zeabur publishes is reached
/// through a port-forward when it is a Service.
async fn local(client: UserClient, info: &ZeaburInfo) -> Result<Zeabur, String> {
    builder_installed(&client).await?;
    let mut settings = zeabur_api::Settings::default();
    settings.builds.registry.clone_from(&info.registry);
    if let Some(service) = info.registry.as_deref().and_then(ServiceAddress::parse) {
        if let Some(reference) = info.registry_ca.as_deref() {
            settings.trusted_ca = Some(registry_ca(client.kube(), reference).await?);
        }
        let port = forward::start(client.kube().clone(), &service).await?;
        settings.upload_address = Some(format!("https://127.0.0.1:{port}"));
    }
    Ok(Zeabur::Local(InProcess::with_client(client, settings)))
}

async fn builder_installed(cluster: &dyn Cluster) -> Result<(), String> {
    let served = cluster
        .served_resources()
        .await
        .map_err(|e| e.into_problem("the resources the cluster serves").detail)?;
    if served
        .iter()
        .any(|r| r.group == "builder.zeabur.com" && r.resource == "builds")
    {
        Ok(())
    } else {
        Err(NO_BUILDER.to_owned())
    }
}

/// The PEM at `reference`, `<namespace>/<ConfigMap>/<key>`.
async fn registry_ca(client: &kube::Client, reference: &str) -> Result<Vec<u8>, String> {
    let cannot = |why: &str| format!("Cannot read the registry's CA ({reference}): {why}.");
    let mut parts = reference.splitn(3, '/');
    let (Some(namespace), Some(name), Some(key)) = (parts.next(), parts.next(), parts.next())
    else {
        return Err(cannot("not namespace/name/key"));
    };
    let map = kube::Api::<ConfigMap>::namespaced(client.clone(), namespace)
        .get(name)
        .await
        .map_err(|e| cannot(&e.to_string()))?;
    map.data
        .and_then(|mut data| data.remove(key))
        .map(String::into_bytes)
        .ok_or_else(|| cannot("the key is missing"))
}

/// Zeabur's API, on the server or in this process.
#[derive(Debug)]
enum Zeabur {
    Server(Server),
    Local(InProcess),
}

impl Zeabur {
    async fn get(&self, path_and_query: &str) -> Result<Value, Value> {
        match self {
            Self::Server(server) => server.get(path_and_query).await,
            Self::Local(api) => api.get(path_and_query).await,
        }
    }

    async fn post(&self, path_and_query: &str, body: &Value) -> Result<Value, Value> {
        match self {
            Self::Server(server) => server.post(path_and_query, body).await,
            Self::Local(api) => api.post(path_and_query, body).await,
        }
    }

    async fn post_bytes(
        &self,
        path_and_query: &str,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> Result<Value, Value> {
        match self {
            Self::Server(server) => server.post_bytes(path_and_query, content_type, bytes).await,
            Self::Local(api) => api.post_bytes(path_and_query, content_type, bytes).await,
        }
    }
}

/// How often and how long to ask for the upload token.
#[derive(Debug, Clone, Copy)]
struct Timing {
    poll: Duration,
    wait: Duration,
}

/// The name of the folder `path` is, as a service name: lower case letters, digits and hyphens,
/// starting with a letter.
fn folder_name(path: &Path) -> String {
    let name = std::fs::canonicalize(path)
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()))
        .unwrap_or_default();
    let mut slug = String::new();
    for c in name.chars() {
        let c = if c.is_ascii_lowercase() || c.is_ascii_digit() {
            c
        } else {
            '-'
        };
        if c == '-' && slug.ends_with('-') {
            continue;
        }
        slug.push(c);
    }
    let slug = slug.trim_start_matches(|c: char| !c.is_ascii_lowercase());
    let slug: String = slug.chars().take(NAME_MAX - 3).collect();
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "app".to_owned()
    } else {
        slug.to_owned()
    }
}

/// What a problem says, for the terminal.
fn said(problem: &Value) -> String {
    problem
        .get("detail")
        .and_then(Value::as_str)
        .unwrap_or("Zeabur gave no reason")
        .to_owned()
}

fn named<'a>(builds: &'a Value, name: &str) -> Option<&'a Value> {
    builds
        .get("builds")?
        .as_array()?
        .iter()
        .find(|b| b.get("name").and_then(Value::as_str) == Some(name))
}

async fn plan_and_apply(api: &Zeabur, project: &str, changes: Vec<Value>) -> Result<(), String> {
    let planned = api
        .post(
            &format!("/api/v1/projects/{project}/plan"),
            &json!({"changes": changes}),
        )
        .await
        .map_err(|p| said(&p))?;
    api.post(
        &format!("/api/v1/projects/{project}/apply"),
        &json!({"token": planned.get("token")}),
    )
    .await
    .map_err(|p| said(&p))?;
    Ok(())
}

/// The projects the person may put a service in.
async fn projects(api: &Zeabur) -> Result<Vec<String>, String> {
    let list = api.get("/api/v1/projects").await.map_err(|p| said(&p))?;
    let items = list
        .get("items")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    Ok(items
        .iter()
        .filter(|p| {
            p.get("adopted") == Some(&json!(true)) && p.get("deleting") != Some(&json!(true))
        })
        .filter_map(|p| p.get("name").and_then(Value::as_str).map(str::to_owned))
        .collect())
}

async fn choose_project(
    api: &Zeabur,
    given: Option<&str>,
    ask: &mut Ask<'_>,
) -> Result<String, String> {
    let names = projects(api).await?;
    let list = names.join(", ");
    if let Some(project) = given {
        return if names.iter().any(|n| n == project) {
            Ok(project.to_owned())
        } else {
            Err(format!(
                "{project} is not a project you can use. Projects: {list}."
            ))
        };
    }
    match (names.as_slice(), ask.as_mut()) {
        ([], _) => Err("You have no project to put a service in.".to_owned()),
        ([only], _) => Ok(only.clone()),
        (_, None) => Err(format!(
            "Name the project with --project. Projects: {list}."
        )),
        (_, Some(ask)) => {
            let menu = names
                .iter()
                .enumerate()
                .map(|(i, n)| format!("  {}  {n}", i + 1))
                .collect::<Vec<_>>()
                .join("\n");
            let answer =
                ask(&format!("Projects:\n{menu}\nProject (number or name): ")).unwrap_or_default();
            let answer = answer.trim();
            answer
                .parse::<usize>()
                .ok()
                .and_then(|n| names.get(n.checked_sub(1)?))
                .or_else(|| names.iter().find(|n| *n == answer))
                .cloned()
                .ok_or_else(|| format!("{answer:?} is not one of: {list}."))
        }
    }
}

fn choose_service(target: &Target, ask: &mut Ask<'_>) -> Result<String, String> {
    if let Some(service) = &target.service {
        return Ok(service.clone());
    }
    let Some(ask) = ask.as_mut() else {
        return Err("Name the service with --service.".to_owned());
    };
    let suggested = &target.folder_name;
    let answer = ask(&format!("Service name [{suggested}]: ")).unwrap_or_default();
    let answer = answer.trim();
    Ok(if answer.is_empty() {
        suggested.clone()
    } else {
        answer.to_owned()
    })
}

/// Makes the service `name` in `project`: one change, which Zeabur turns into the objects the
/// web interface makes.
async fn create_service(
    api: &Zeabur,
    project: &str,
    name: &str,
    target: &Target,
    request: &str,
) -> Result<(), String> {
    let mut change = json!({
        "op": "createUploadService",
        "name": name,
        "request": request,
        "method": match target.method {
            Method::Railpack => "railpack",
            Method::Dockerfile => "dockerfile",
        },
    });
    if let (Some(port), Some(change)) = (target.port, change.as_object_mut()) {
        change.insert("port".to_owned(), json!(port));
    }
    plan_and_apply(api, project, vec![change]).await
}

/// Chooses the project and the service, makes the service when it is missing, then pushes the
/// folder and names its digest. The service it built.
async fn deploy(
    api: &Zeabur,
    target: &Target,
    folder: Vec<u8>,
    request: &str,
    timing: Timing,
    mut ask: Ask<'_>,
) -> Result<String, String> {
    let project = choose_project(api, target.project.as_deref(), &mut ask).await?;
    let project = project.as_str();
    let builds = api
        .get(&format!("/api/v1/projects/{project}/builds"))
        .await
        .map_err(|p| said(&p))?;
    if builds.get("state").and_then(Value::as_str) == Some("not-installed") {
        return Err(NO_BUILDER.to_owned());
    }
    let service = choose_service(target, &mut ask)?;
    let service = service.as_str();
    match named(&builds, service) {
        Some(build) if build.get("archive") != Some(&json!(true)) => {
            return Err(format!(
                "{service} builds from Git. Pick another name to make a service from a folder."
            ));
        }
        Some(_) => {
            output::info("Asking for an upload token.");
            plan_and_apply(
                api,
                project,
                vec![json!({"op": "uploadToken", "build": service, "request": request})],
            )
            .await?;
        }
        None => {
            if !target.yes
                && let Some(ask) = ask.as_mut()
            {
                let answer = ask(&format!(
                    "{service} does not exist in {project}. Make it? [Y/n] "
                ))
                .unwrap_or_default();
                if answer.trim().to_lowercase().starts_with('n') {
                    return Err(format!("{service} was not made."));
                }
            }
            output::info(&format!("Making {service} in {project}."));
            create_service(api, project, service, target, request).await?;
        }
    }
    wait_for_token(api, project, service, request, timing).await?;
    output::info("Uploading.");
    let uploaded = api
        .post_bytes(
            &format!("/api/v1/projects/{project}/builds/{service}/source?request={request}"),
            "application/gzip",
            folder,
        )
        .await
        .map_err(|p| said(&p))?;
    plan_and_apply(
        api,
        project,
        vec![json!({"op": "archiveDigest", "build": service, "digest": uploaded.get("digest")})],
    )
    .await?;
    Ok(service.to_owned())
}

async fn wait_for_token(
    api: &Zeabur,
    project: &str,
    service: &str,
    request: &str,
    timing: Timing,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + timing.wait;
    loop {
        let builds = api
            .get(&format!("/api/v1/projects/{project}/builds"))
            .await
            .map_err(|p| said(&p))?;
        let answered = named(&builds, service)
            .and_then(|b| b.get("uploadRequest"))
            .is_some_and(|asked| asked == request);
        if answered {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("Zeabur Builder did not answer the upload token request.".to_owned());
        }
        tokio::time::sleep(timing.poll).await;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use axum::Router;
    use axum::http::{StatusCode, header};
    use axum::response::IntoResponse as _;
    use secrecy::SecretString;
    use zeabur_api::access::SingleCluster;
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects, ServedResource, WriteBody};

    use super::*;

    const TOKEN_REQUEST: &str = "builder.zeabur.com/upload-token-request";

    /// A registry that takes everything and says where to go on.
    async fn registry() -> String {
        let router = Router::new().fallback(|request: axum::extract::Request| async move {
            let path = request.uri().path().to_owned();
            if request.method() == "PUT" {
                StatusCode::CREATED.into_response()
            } else {
                (
                    StatusCode::ACCEPTED,
                    [(header::LOCATION, format!("{path}x"))],
                )
                    .into_response()
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        tokio::spawn(async move { axum::serve(listener, router).await });
        address
    }

    fn namespace(name: &str) -> Namespace {
        serde_json::from_value(
            json!({"metadata": {"name": name, "labels": {"zeabur.com/project": "true"}}}),
        )
        .unwrap()
    }

    /// Two projects, `shop` and `blog`, and the registry of the upload Secret `site-upload`.
    fn cluster(registry: &str) -> MemoryCluster {
        let cluster = MemoryCluster::new(State {
            identity: Identity {
                username: "alice".to_owned(),
                groups: Vec::new(),
            },
            namespaces: vec![namespace("shop"), namespace("blog")],
            objects: ["shop", "blog"]
                .into_iter()
                .map(|name| {
                    (
                        name.to_owned(),
                        NamespaceObjects {
                            namespace: name.to_owned(),
                            ..NamespaceObjects::default()
                        },
                    )
                })
                .collect(),
            ..State::default()
        });
        let config = json!({"auths": {registry: {"registrytoken": "t"}}}).to_string();
        cluster.update(|s| {
            s.served.push(ServedResource::readable(
                "builder.zeabur.com",
                "v1alpha1",
                "builds",
                "Build",
                true,
            ));
            s.secret_values.insert(
                "shop/site-upload".to_owned(),
                BTreeMap::from([
                    ("token".to_owned(), b"t".to_vec()),
                    (".dockerconfigjson".to_owned(), config.into_bytes()),
                ]),
            );
        });
        cluster
    }

    fn with_build(cluster: &MemoryCluster, source: &Value) {
        cluster.update(|s| {
            s.dynamic.push(
                serde_json::from_value(json!({
                    "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "Build",
                    "metadata": {"name": "site", "namespace": "shop", "resourceVersion": "7",
                        "generation": 1},
                    "spec": {"builderRef": "railpack", "source": source},
                    "status": {"uploadToken": {"request": "up-1", "secretName": "site-upload",
                        "expiresAt": "2099-01-01T00:00:00Z"}}
                }))
                .unwrap(),
            );
        });
    }

    /// Zeabur Builder's controller: answers the request a `Build` asks with.
    fn controller(cluster: &MemoryCluster) {
        let cluster = cluster.clone();
        tokio::spawn(async move {
            loop {
                cluster.update(|s| {
                    for build in &mut s.dynamic {
                        let request = build
                            .metadata
                            .annotations
                            .as_ref()
                            .and_then(|a| a.get(TOKEN_REQUEST))
                            .cloned();
                        if let Some(request) = request {
                            build.data["status"] = json!({"uploadToken": {"request": request,
                                "secretName": "site-upload",
                                "expiresAt": "2099-01-01T00:00:00Z"}});
                        }
                    }
                });
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        });
    }

    fn router(cluster: &MemoryCluster) -> Router {
        let settings = zeabur_api::Settings {
            builds: zeabur_api::settings::BuildSettings {
                registry: Some("registry.test:5000".to_owned()),
            },
            ..zeabur_api::Settings::default()
        };
        zeabur_api::router_with_settings(
            Arc::new(SingleCluster(Arc::new(cluster.clone()))),
            settings,
        )
    }

    /// The API of `cluster` on a loopback port, and a client for it.
    async fn server(cluster: &MemoryCluster) -> Zeabur {
        let router = router(cluster);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await });
        let token = UserToken::new(SecretString::from("t".to_owned())).unwrap();
        Zeabur::Server(Server::new(&format!("http://{address}"), token).unwrap())
    }

    /// The API of `cluster` in this process, as local mode runs it.
    fn local(cluster: &MemoryCluster) -> Zeabur {
        Zeabur::Local(InProcess::new(router(cluster)))
    }

    const ISSUER: &str = "https://identity.example.com";

    fn token(value: &str) -> UserToken {
        UserToken::new(SecretString::from(value.to_owned())).unwrap()
    }

    /// Only a token from the Zeabur Identity that `zeabur-info` names goes to the server. Any
    /// other credential, or a cluster without `zeabur-info`, keeps it on this computer.
    #[test]
    fn only_a_zeabur_identity_token_goes_to_the_server() {
        let identity = "e30.eyJpc3MiOiJodHRwczovL2lkZW50aXR5LmV4YW1wbGUuY29tIn0.sig";
        let google = "e30.eyJpc3MiOiJodHRwczovL2FjY291bnRzLmdvb2dsZS5jb20ifQ.sig";
        assert!(matches!(
            mode(Some(token(identity)), Some(ISSUER)),
            Mode::Server(_)
        ));
        for (credential, issuer) in [
            (Some(token(google)), Some(ISSUER)),
            (Some(token("static-token")), Some(ISSUER)),
            (None, Some(ISSUER)),
            (Some(token(identity)), None),
        ] {
            assert!(matches!(mode(credential, issuer), Mode::Local));
        }
    }

    /// Local mode stops before it uploads anything when the cluster has no Zeabur Builder.
    #[tokio::test]
    async fn local_mode_stops_without_zeabur_builder() {
        let bare = MemoryCluster::new(State::default());
        let error = builder_installed(&bare).await.unwrap_err();
        assert!(error.contains("Zeabur Builder") && error.contains("Nothing was uploaded"));
        assert!(bare.inspect(|s| s.writes.is_empty()).unwrap());
        let address = registry().await;
        builder_installed(&cluster(&address)).await.unwrap();
    }

    /// Local mode runs the same plans and the same upload as the server, in this process.
    #[tokio::test]
    async fn local_mode_makes_the_service_and_uploads_in_this_process() {
        let address = registry().await;
        let cluster = cluster(&address);
        controller(&cluster);
        deploy(
            &local(&cluster),
            &target(Some("shop"), Some("site")),
            vec![7; 100],
            "up-5",
            QUICK,
            None,
        )
        .await
        .unwrap();
        assert!(created(&cluster, "\"Build\"")[0].contains("up-5"));
        assert!(
            patches(&cluster)
                .iter()
                .any(|p| p.contains("/spec/source/archive/digest") && p.contains("sha256:"))
        );
    }

    fn target(project: Option<&str>, service: Option<&str>) -> Target {
        Target {
            project: project.map(str::to_owned),
            service: service.map(str::to_owned),
            method: Method::Railpack,
            port: None,
            yes: false,
            folder_name: "my-app".to_owned(),
        }
    }

    const QUICK: Timing = Timing {
        poll: Duration::from_millis(5),
        wait: Duration::from_secs(5),
    };

    fn patches(cluster: &MemoryCluster) -> Vec<String> {
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        writes
            .iter()
            .filter_map(|w| match &w.body {
                WriteBody::JsonPatch(operations) => Some(operations.to_string()),
                WriteBody::Create(_) | WriteBody::EphemeralContainers(_) | WriteBody::Delete(_) => {
                    None
                }
            })
            .collect()
    }

    fn created(cluster: &MemoryCluster, kind: &str) -> Vec<String> {
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        writes
            .iter()
            .filter_map(|w| match &w.body {
                WriteBody::Create(object) if object.to_string().contains(kind) => {
                    Some(object.to_string())
                }
                WriteBody::Create(_)
                | WriteBody::JsonPatch(_)
                | WriteBody::EphemeralContainers(_)
                | WriteBody::Delete(_) => None,
            })
            .collect()
    }

    /// A service that exists gets a token request, the folder, and the digest it was given, in
    /// that order; both writes are plans of the API.
    #[tokio::test]
    async fn the_folder_goes_up_in_the_order_the_server_expects() {
        let address = registry().await;
        let cluster = cluster(&address);
        with_build(&cluster, &json!({"archive": {}}));
        let server = server(&cluster).await;
        let service = deploy(
            &server,
            &target(Some("shop"), Some("site")),
            vec![7; 100],
            "up-1",
            QUICK,
            None,
        )
        .await
        .unwrap();
        assert_eq!(service, "site");
        let patches = patches(&cluster);
        assert_eq!(patches.len(), 2, "{patches:?}");
        assert!(patches[0].contains("upload-token-request") && patches[0].contains("up-1"));
        assert!(
            patches[1].contains("/spec/source/archive/digest") && patches[1].contains("sha256:")
        );
    }

    /// A service that builds from Git has no upload, and nothing is written.
    #[tokio::test]
    async fn a_service_from_git_is_refused_before_anything_is_written() {
        let address = registry().await;
        let cluster = cluster(&address);
        with_build(
            &cluster,
            &json!({"git": {"url": "https://example.com/a/b"}}),
        );
        let server = server(&cluster).await;
        let error = deploy(
            &server,
            &target(Some("shop"), Some("site")),
            vec![7; 10],
            "up-1",
            QUICK,
            None,
        )
        .await
        .unwrap_err();
        assert!(error.contains("Git"), "{error}");
        assert!(cluster.inspect(|s| s.writes.is_empty()).unwrap());
    }

    /// A service that does not exist is made with its port and a Build that asks for the token,
    /// then the folder goes up to it, with no question where flags say it all.
    #[tokio::test]
    async fn a_missing_service_is_made_and_then_uploaded() {
        let address = registry().await;
        let cluster = cluster(&address);
        controller(&cluster);
        let server = server(&cluster).await;
        let mut wanted = target(Some("shop"), Some("site"));
        wanted.port = Some(8080);
        deploy(&server, &wanted, vec![7; 100], "up-2", QUICK, None)
            .await
            .unwrap();
        let builds = created(&cluster, "\"Build\"");
        assert_eq!(builds.len(), 1, "{builds:?}");
        assert!(builds[0].contains("up-2") && builds[0].contains("registry.test:5000/shop/site"));
        assert!(builds[0].contains("\"archive\""));
        assert!(
            created(&cluster, "\"Deployment\"")
                .iter()
                .any(|d| d.contains("registry.test:5000/shop/site:pending"))
        );
        assert!(
            created(&cluster, "\"Service\"")
                .iter()
                .any(|d| d.contains("8080"))
        );
        let patches = patches(&cluster);
        assert!(
            patches
                .iter()
                .any(|p| p.contains("/spec/source/archive/digest") && p.contains("sha256:")),
            "{patches:?}"
        );
    }

    /// Without a terminal, a missing project or name is an error that names the flag, and
    /// nothing is written.
    #[tokio::test]
    async fn without_a_terminal_the_flags_name_the_project_and_the_service() {
        let address = registry().await;
        let cluster = cluster(&address);
        let server = server(&cluster).await;
        let error = deploy(
            &server,
            &target(None, Some("site")),
            vec![7],
            "up-3",
            QUICK,
            None,
        )
        .await
        .unwrap_err();
        assert!(
            error.contains("--project") && error.contains("shop"),
            "{error}"
        );
        let error = deploy(
            &server,
            &target(Some("shop"), None),
            vec![7],
            "up-3",
            QUICK,
            None,
        )
        .await
        .unwrap_err();
        assert!(error.contains("--service"), "{error}");
        let error = deploy(
            &server,
            &target(Some("nowhere"), Some("site")),
            vec![7],
            "up-3",
            QUICK,
            None,
        )
        .await
        .unwrap_err();
        assert!(
            error.contains("nowhere") && error.contains("blog"),
            "{error}"
        );
        assert!(cluster.inspect(|s| s.writes.is_empty()).unwrap());
    }

    /// In a terminal the project is picked from the list and the name defaults to the folder's;
    /// saying no to making the service writes nothing.
    #[tokio::test]
    async fn in_a_terminal_the_person_picks_the_project_and_may_decline() {
        let address = registry().await;
        let cluster = cluster(&address);
        let server = server(&cluster).await;
        let mut questions = Vec::new();
        let mut answers = vec!["n", "", "2"]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut ask = |question: &str| {
            questions.push(question.to_owned());
            answers.pop()
        };
        let error = deploy(
            &server,
            &target(None, None),
            vec![7],
            "up-4",
            QUICK,
            Some(&mut ask),
        )
        .await
        .unwrap_err();
        assert!(error.contains("my-app was not made"), "{error}");
        assert!(questions[0].contains("blog") && questions[0].contains("shop"));
        assert!(questions[1].contains("[my-app]"));
        assert!(questions[2].contains("my-app") && questions[2].contains("shop"));
        assert!(cluster.inspect(|s| s.writes.is_empty()).unwrap());
    }

    #[test]
    fn a_folder_name_becomes_a_service_name_the_web_interface_accepts() {
        let dir = tempfile::tempdir().unwrap();
        for (folder, expected) in [
            ("My App_v2", "my-app-v2"),
            ("2fast", "fast"),
            ("---", "app"),
            ("site-", "site"),
        ] {
            let path = dir.path().join(folder);
            std::fs::create_dir(&path).unwrap();
            let name = folder_name(&path);
            assert_eq!(name, expected, "{folder}");
        }
    }
}
