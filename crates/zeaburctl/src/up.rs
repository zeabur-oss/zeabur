//! `zeaburctl up`: uploads a folder to a service that builds one, and makes the service when it
//! does not exist.
//!
//! The folder is packed (`.gitignore` and `.dockerignore` applied, see [`crate::pack`]), then the
//! steps of the web interface follow through the Zeabur server of the cluster ([`Server`]), with
//! the token of the person's kubeconfig: a plan asks the `Build` for an upload token, the server
//! pushes the folder to the registry with it, and a second plan names the pushed digest in the
//! `Build`. The writes are plans, applied at once because this command is the confirmation. The
//! registry is never reached from this machine.

use std::io::IsTerminal as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use zeabur_kube::UserToken;

use crate::server::Server;
use crate::{output, pack};

/// How long the controller may take to answer an upload token request.
const TOKEN_WAIT: Duration = Duration::from_secs(60);
/// The longest service name.
const NAME_MAX: usize = 40;

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
    /// The address of Zeabur in the cluster, like `https://zeabur.example.com`.
    #[arg(long, env = "ZEABUR_SERVER")]
    server: String,
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
    let token = UserToken::from_kubeconfig(args.context.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    let server = Server::new(&args.server, token)?;
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
        &server,
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
        .unwrap_or("the server gave no reason")
        .to_owned()
}

fn named<'a>(builds: &'a Value, name: &str) -> Option<&'a Value> {
    builds
        .get("builds")?
        .as_array()?
        .iter()
        .find(|b| b.get("name").and_then(Value::as_str) == Some(name))
}

async fn plan_and_apply(server: &Server, project: &str, changes: Vec<Value>) -> Result<(), String> {
    let planned = server
        .post(
            &format!("/api/v1/projects/{project}/plan"),
            &json!({"changes": changes}),
        )
        .await
        .map_err(|p| said(&p))?;
    server
        .post(
            &format!("/api/v1/projects/{project}/apply"),
            &json!({"token": planned.get("token")}),
        )
        .await
        .map_err(|p| said(&p))?;
    Ok(())
}

/// The projects the person may put a service in.
async fn projects(server: &Server) -> Result<Vec<String>, String> {
    let list = server.get("/api/v1/projects").await.map_err(|p| said(&p))?;
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
    server: &Server,
    given: Option<&str>,
    ask: &mut Ask<'_>,
) -> Result<String, String> {
    let names = projects(server).await?;
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
    server: &Server,
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
    plan_and_apply(server, project, vec![change]).await
}

/// Chooses the project and the service, makes the service when it is missing, then pushes the
/// folder and names its digest. The service it built.
async fn deploy(
    server: &Server,
    target: &Target,
    folder: Vec<u8>,
    request: &str,
    timing: Timing,
    mut ask: Ask<'_>,
) -> Result<String, String> {
    let project = choose_project(server, target.project.as_deref(), &mut ask).await?;
    let project = project.as_str();
    let builds = server
        .get(&format!("/api/v1/projects/{project}/builds"))
        .await
        .map_err(|p| said(&p))?;
    if builds.get("state").and_then(Value::as_str) == Some("not-installed") {
        return Err("Zeabur Builder is not installed in this cluster.".to_owned());
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
                server,
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
            create_service(server, project, service, target, request).await?;
        }
    }
    wait_for_token(server, project, service, request, timing).await?;
    output::info("Uploading.");
    let uploaded = server
        .post_bytes(
            &format!("/api/v1/projects/{project}/builds/{service}/source?request={request}"),
            "application/gzip",
            folder,
        )
        .await
        .map_err(|p| said(&p))?;
    plan_and_apply(
        server,
        project,
        vec![json!({"op": "archiveDigest", "build": service, "digest": uploaded.get("digest")})],
    )
    .await?;
    Ok(service.to_owned())
}

async fn wait_for_token(
    server: &Server,
    project: &str,
    service: &str,
    request: &str,
    timing: Timing,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + timing.wait;
    loop {
        let builds = server
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

    /// The API of `cluster` on a loopback port, and a client for it.
    async fn server(cluster: &MemoryCluster) -> Server {
        let settings = zeabur_api::Settings {
            builds: zeabur_api::settings::BuildSettings {
                registry: Some("registry.test:5000".to_owned()),
            },
            ..zeabur_api::Settings::default()
        };
        let router = zeabur_api::router_with_settings(
            Arc::new(SingleCluster(Arc::new(cluster.clone()))),
            settings,
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await });
        let token = UserToken::new(SecretString::from("t".to_owned())).unwrap();
        Server::new(&format!("http://{address}"), token).unwrap()
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
