//! `zeaburctl up`: uploads a folder to a service that builds one.
//!
//! The folder is packed (`.gitignore` and `.dockerignore` applied, see [`crate::pack`]), then the
//! same steps as the web interface follow through the API in this process, as the person whose
//! kubeconfig this is: a plan asks the `Build` for an upload token, the API pushes the folder to
//! the registry with it, and a second plan names the pushed digest in the `Build`. Both writes
//! are plans, applied at once because this command is the confirmation.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::in_process::InProcess;
use crate::{output, pack};

/// How long the controller may take to answer an upload token request.
const TOKEN_WAIT: Duration = Duration::from_secs(60);

/// Options of `zeaburctl up`.
#[derive(Debug, clap::Args)]
pub(crate) struct UpArgs {
    /// The folder to upload (default: the current one).
    #[arg(default_value = ".")]
    folder: PathBuf,
    /// The project the service is in.
    #[arg(long)]
    project: String,
    /// The service, made with "Upload a folder" in the web interface; its build has the same
    /// name.
    #[arg(long)]
    service: String,
    /// The kubeconfig context to use (default: the current one).
    #[arg(long)]
    context: Option<String>,
    /// Where to reach the registry from this machine, as `host:port` or a URL, when the address
    /// the cluster uses does not resolve here (for example a port-forward, `localhost:5000`).
    #[arg(long)]
    registry_address: Option<String>,
    /// A PEM file with the CA certificate of the registry, when it is not a public one.
    #[arg(long)]
    registry_ca_file: Option<PathBuf>,
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

async fn up(args: &UpArgs) -> Result<(), String> {
    let packed = pack::pack(&args.folder).map_err(|e| e.to_string())?;
    output::info(&format!(
        "Packed {} files, {} KiB.",
        packed.files,
        packed.bytes.len() >> 10
    ));
    if packed.skipped > 0 {
        output::info(&format!(
            "Left out {} links or special files that cannot go in the archive.",
            packed.skipped
        ));
    }
    let mut settings = crate::in_process::settings(None)?;
    settings.upload_address.clone_from(&args.registry_address);
    if let Some(file) = &args.registry_ca_file {
        settings.trusted_ca =
            Some(std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?);
    }
    let api = InProcess::from_kubeconfig(args.context.as_deref(), settings).await?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    deploy(
        &api,
        &args.project,
        &args.service,
        packed.bytes,
        &format!("up-{nanos}"),
        Duration::from_secs(1),
    )
    .await?;
    output::info(&format!(
        "Uploaded. {} is building the new folder.",
        args.service
    ));
    Ok(())
}

/// What a problem says, for the terminal.
fn said(problem: &Value) -> String {
    problem
        .get("detail")
        .and_then(Value::as_str)
        .unwrap_or("the API gave no reason")
        .to_owned()
}

fn named<'a>(builds: &'a Value, name: &str) -> Option<&'a Value> {
    builds
        .get("builds")?
        .as_array()?
        .iter()
        .find(|b| b.get("name").and_then(Value::as_str) == Some(name))
}

async fn plan_and_apply(api: &InProcess, project: &str, change: Value) -> Result<(), String> {
    let planned = api
        .post(
            &format!("/api/v1/projects/{project}/plan"),
            &json!({"changes": [change]}),
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

/// Asks for the token, pushes `folder`, and names its digest, for the `Build` `service`.
async fn deploy(
    api: &InProcess,
    project: &str,
    service: &str,
    folder: Vec<u8>,
    request: &str,
    poll: Duration,
) -> Result<(), String> {
    let builds = api
        .get(&format!("/api/v1/projects/{project}/builds"))
        .await
        .map_err(|p| said(&p))?;
    let build = named(&builds, service)
        .ok_or_else(|| format!("{project} has no build named {service}."))?;
    if build.get("archive") != Some(&json!(true)) {
        return Err(format!(
            "{service} builds from Git. Make a service with \"Upload a folder\" in the web interface first."
        ));
    }
    output::info("Asking for an upload token.");
    plan_and_apply(
        api,
        project,
        json!({"op": "uploadToken", "build": service, "request": request}),
    )
    .await?;
    let deadline = tokio::time::Instant::now() + TOKEN_WAIT;
    loop {
        let builds = api
            .get(&format!("/api/v1/projects/{project}/builds"))
            .await
            .map_err(|p| said(&p))?;
        let answered = named(&builds, service)
            .and_then(|b| b.get("uploadRequest"))
            .is_some_and(|asked| asked == request);
        if answered {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("Zeabur Builder did not answer the upload token request.".to_owned());
        }
        tokio::time::sleep(poll).await;
    }
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
        json!({"op": "archiveDigest", "build": service, "digest": uploaded.get("digest")}),
    )
    .await
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use axum::Router;
    use axum::http::{StatusCode, header};
    use axum::response::IntoResponse as _;
    use zeabur_api::access::SingleCluster;
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects, ServedResource, WriteBody};

    use super::*;

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

    fn cluster(registry: &str, source: &Value) -> MemoryCluster {
        let namespace: Namespace = serde_json::from_value(
            json!({"metadata": {"name": "shop", "labels": {"zeabur.com/project": "true"}}}),
        )
        .unwrap();
        let cluster = MemoryCluster::new(State {
            identity: Identity {
                username: "alice".to_owned(),
                groups: Vec::new(),
            },
            namespaces: vec![namespace],
            objects: BTreeMap::from([(
                "shop".to_owned(),
                NamespaceObjects {
                    namespace: "shop".to_owned(),
                    ..NamespaceObjects::default()
                },
            )]),
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

    fn api(cluster: &MemoryCluster) -> InProcess {
        InProcess::new(zeabur_api::router(Arc::new(SingleCluster(Arc::new(
            cluster.clone(),
        )))))
    }

    /// The command asks for a token, waits for the controller's answer to its own request, pushes
    /// and names the digest it was given; both writes are plans of the API.
    #[tokio::test]
    async fn the_folder_goes_up_in_the_order_the_api_expects() {
        let address = registry().await;
        let cluster = cluster(&address, &json!({"archive": {}}));
        deploy(
            &api(&cluster),
            "shop",
            "site",
            vec![7; 100],
            "up-1",
            Duration::ZERO,
        )
        .await
        .unwrap();
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        let patches: Vec<String> = writes
            .iter()
            .filter_map(|w| match &w.body {
                WriteBody::JsonPatch(operations) => Some(operations.to_string()),
                WriteBody::Create(_) | WriteBody::EphemeralContainers(_) | WriteBody::Delete(_) => {
                    None
                }
            })
            .collect();
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
        let cluster = cluster(
            &address,
            &json!({"git": {"url": "https://example.com/a/b"}}),
        );
        let error = deploy(
            &api(&cluster),
            "shop",
            "site",
            vec![7; 10],
            "up-1",
            Duration::ZERO,
        )
        .await
        .unwrap_err();
        assert!(error.contains("Git"), "{error}");
        assert!(cluster.inspect(|s| s.writes.is_empty()).unwrap());
    }
}
