//! `zeaburctl mcp`: the read-only MCP server (`zeabur-mcp`) over stdio, reading the API in this
//! process with the person's kubeconfig.
//!
//! Standard output carries only protocol messages; messages for the person go to standard error,
//! and the server exits when standard input closes [mcp-stdio].
//!
//! [mcp-stdio]: https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio

use std::future::Future;
use std::process::ExitCode;

use rmcp::ServiceExt as _;
use rmcp::service::{QuitReason, ServerInitializeError};
use serde_json::Value;
use zeabur_mcp::{Reader, Server};

use crate::in_process::InProcess;
use crate::output;

/// Options of `zeaburctl mcp`.
#[derive(Debug, clap::Args)]
pub(crate) struct McpArgs {
    /// The kubeconfig context to use (default: the current one).
    #[arg(long)]
    context: Option<String>,
    /// The metrics store history is read from: a Service port serving the Prometheus HTTP API,
    /// as `namespace/name:port` (default: where Zeabur's recommendation installs
    /// VictoriaMetrics).
    #[arg(long, env = "ZEABUR_METRICS_SERVICE")]
    metrics_service: Option<String>,
}

pub(crate) fn run(args: &McpArgs) -> ExitCode {
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
    match runtime.block_on(serve(args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            output::error(&message);
            ExitCode::FAILURE
        }
    }
}

async fn serve(args: &McpArgs) -> Result<(), String> {
    let settings = crate::in_process::settings(args.metrics_service.as_deref())?;
    let api = InProcess::from_kubeconfig(args.context.as_deref(), settings).await?;
    let running = match Server::new(api).serve(rmcp::transport::stdio()).await {
        Ok(running) => running,
        // Standard input closed before a first request: nothing to serve.
        Err(ServerInitializeError::ConnectionClosed(_)) => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    match running.waiting().await {
        Ok(QuitReason::JoinError(error)) | Err(error) => Err(error.to_string()),
        Ok(_closed_or_cancelled) => Ok(()),
    }
}

impl Reader for InProcess {
    fn get(&self, path_and_query: &str) -> impl Future<Output = Result<Value, Value>> + Send {
        Self::get(self, path_and_query)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use rmcp::model::{CallToolRequestParams, CallToolResult, ProtocolVersion};
    use rmcp::service::{RoleClient, RunningService};
    use rmcp::{ClientLifecycleMode, ClientServiceExt as _, ServiceExt as _};
    use serde_json::{Value, json};
    use zeabur_api::access::SingleCluster;
    use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
    use zeabur_kube::k8s_openapi::api::core::v1::{Namespace, Pod, Secret};
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{NamespaceObjects, SecretSummary};
    use zeabur_mcp::Server;

    use crate::in_process::InProcess;

    const SECRET_VALUE: &str = "correct-horse-battery-staple";

    /// A project `shop` with a crash-looping Deployment `web` and a Secret `db`.
    fn cluster() -> MemoryCluster {
        let deployment: Deployment = serde_json::from_value(json!({
            "metadata": {"name": "web", "namespace": "shop", "labels": {"app": "web"}},
            "spec": {
                "selector": {"matchLabels": {"app": "web"}},
                "template": {
                    "metadata": {"labels": {"app": "web"}},
                    "spec": {"containers": [{
                        "name": "web", "image": "busybox:1.37",
                        "env": [{"name": "PASSWORD", "valueFrom": {"secretKeyRef": {"name": "db", "key": "password"}}}]
                    }]}
                }
            }
        }))
        .unwrap();
        let pod: Pod = serde_json::from_value(json!({
            "metadata": {"name": "web-1", "namespace": "shop", "labels": {"app": "web"}},
            "spec": {"containers": [{"name": "web", "image": "busybox:1.37"}]},
            "status": {"phase": "Running", "containerStatuses": [{
                "name": "web", "image": "busybox:1.37", "imageID": "", "ready": false,
                "restartCount": 4, "state": {"waiting": {"reason": "CrashLoopBackOff"}},
                "lastState": {"terminated": {"exitCode": 3, "reason": "Error"}}
            }]}
        }))
        .unwrap();
        let secret = Secret {
            metadata: serde_json::from_value(json!({"name": "db", "namespace": "shop"})).unwrap(),
            type_: Some("Opaque".to_owned()),
            ..Secret::default()
        };
        let namespaces: Vec<Namespace> = vec![
            serde_json::from_value(json!({"metadata": {"name": "shop",
                "labels": {"zeabur.com/project": "true"}}}))
            .unwrap(),
        ];
        MemoryCluster::new(State {
            namespaces,
            objects: BTreeMap::from([(
                "shop".to_owned(),
                NamespaceObjects {
                    namespace: "shop".to_owned(),
                    deployments: vec![deployment],
                    pods: vec![pod],
                    secrets: vec![SecretSummary {
                        secret,
                        key_count: 1,
                    }],
                    ..NamespaceObjects::default()
                },
            )]),
            logs: BTreeMap::from([("shop/web-1".to_owned(), "starting\n".to_owned())]),
            previous_logs: BTreeMap::from([(
                "shop/web-1".to_owned(),
                "starting\nfatal: no database\n".to_owned(),
            )]),
            secret_values: BTreeMap::from([(
                "shop/db".to_owned(),
                BTreeMap::from([("password".to_owned(), SECRET_VALUE.as_bytes().to_vec())]),
            )]),
            ..State::default()
        })
    }

    /// A client session with `zeaburctl mcp`'s server over `cluster`, in `lifecycle`.
    async fn session(
        cluster: &MemoryCluster,
        lifecycle: ClientLifecycleMode,
    ) -> RunningService<RoleClient, ()> {
        let api = InProcess::new(zeabur_api::router(Arc::new(SingleCluster(Arc::new(
            cluster.clone(),
        )))));
        let (server_end, client_end) = tokio::io::duplex(64 * 1024);
        tokio::spawn(async move {
            let running = Server::new(api).serve(server_end).await.unwrap();
            running.waiting().await.unwrap();
        });
        ().serve_with_lifecycle(client_end, lifecycle)
            .await
            .unwrap()
    }

    /// Both revisions: 2025-11-25's `initialize` and 2026-07-28's `server/discover`.
    fn eras() -> [ClientLifecycleMode; 2] {
        [
            ClientLifecycleMode::Initialize,
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
            },
        ]
    }

    async fn call(
        client: &RunningService<RoleClient, ()>,
        tool: &str,
        args: Value,
    ) -> CallToolResult {
        let Value::Object(args) = args else {
            panic!("arguments are an object");
        };
        client
            .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(args))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn both_eras_list_the_tools_and_answer_a_call() {
        for era in eras() {
            let client = session(&cluster(), era.clone()).await;
            let mut tools: Vec<String> = client
                .list_all_tools()
                .await
                .unwrap()
                .into_iter()
                .map(|tool| tool.name.into_owned())
                .collect();
            tools.sort();
            assert_eq!(
                tools,
                [
                    "cluster_summary",
                    "get_project",
                    "get_service",
                    "list_projects",
                    "object_yaml",
                    "pod_logs",
                    "project_events",
                    "project_usage",
                    "service_history"
                ],
                "{era:?}"
            );

            let projects = call(&client, "list_projects", json!({})).await;
            assert_eq!(projects.is_error, Some(false), "{era:?}");
            let structured = projects.structured_content.unwrap();
            assert_eq!(structured["items"][0]["name"], "shop", "{era:?}");
            let text = projects.content[0].as_text().unwrap().text.clone();
            assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), structured);

            let unknown = client
                .call_tool(CallToolRequestParams::new("delete_project"))
                .await;
            assert!(unknown.is_err(), "{era:?}: {unknown:?}");
            client.cancel().await.unwrap();
        }
    }

    /// The question the server is for: which service is restarting, and why.
    #[tokio::test]
    async fn a_project_names_the_restarting_service_and_how_its_last_run_ended() {
        let client = session(&cluster(), ClientLifecycleMode::Initialize).await;
        let project = call(&client, "get_project", json!({"project": "shop"}))
            .await
            .structured_content
            .unwrap();
        let web = &project["services"][0];
        assert_eq!(web["name"], "web", "{project}");
        assert_eq!(web["restarts"], 4, "{project}");
        let status = &web["pods"][0]["containerStatuses"][0];
        assert_eq!(status["lastTermination"]["exitCode"], 3, "{project}");
        assert_eq!(status["lastTermination"]["reason"], "Error", "{project}");

        let logs = call(
            &client,
            "pod_logs",
            json!({"project": "shop", "pod": "web-1", "previous": true}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(logs["lines"][1], "fatal: no database", "{logs}");
    }

    /// A refusal reaches the model as kube-apiserver and Zeabur gave it: the missing right and
    /// what to do.
    #[tokio::test]
    async fn a_refusal_is_an_error_result_with_the_api_problem() {
        let cluster = cluster();
        cluster.update(|s| {
            s.forbidden_namespaces.insert("shop".to_owned());
        });
        let client = session(&cluster, ClientLifecycleMode::Initialize).await;
        let refused = call(&client, "get_project", json!({"project": "shop"})).await;
        assert_eq!(refused.is_error, Some(true));
        let problem = refused.structured_content.unwrap();
        assert_eq!(problem["kind"], "forbidden", "{problem}");
        assert_eq!(problem["kubernetes"]["reason"], "Forbidden", "{problem}");
        assert_eq!(problem["denied"]["namespace"], "shop", "{problem}");
        assert!(
            problem["fix"].as_str().is_some_and(|f| !f.is_empty()),
            "{problem}"
        );

        let malformed = call(&client, "get_project", json!({"project": "shop/secrets"})).await;
        assert_eq!(malformed.is_error, Some(true));
        assert_eq!(malformed.structured_content.unwrap()["kind"], "bad-request");
    }

    /// No tool returns a Secret's value: every tool is called, `object_yaml` on the Secret
    /// itself, and the value appears in no answer.
    #[tokio::test]
    async fn no_tool_shows_a_secret_value() {
        let cluster = cluster();
        let client = session(&cluster, ClientLifecycleMode::Initialize).await;
        let calls = BTreeMap::from([
            ("cluster_summary", json!({})),
            ("list_projects", json!({})),
            ("get_project", json!({"project": "shop"})),
            (
                "get_service",
                json!({"project": "shop", "kind": "deployment", "name": "web"}),
            ),
            ("project_events", json!({"project": "shop"})),
            ("pod_logs", json!({"project": "shop", "pod": "web-1"})),
            (
                "object_yaml",
                json!({"project": "shop", "group": "core", "version": "v1",
                    "resource": "secrets", "name": "db"}),
            ),
            ("project_usage", json!({"project": "shop"})),
            (
                "service_history",
                json!({"project": "shop", "kind": "deployment", "name": "web", "range": "24h"}),
            ),
        ]);
        for tool in client.list_all_tools().await.unwrap() {
            let args = calls
                .get(tool.name.as_ref())
                .unwrap_or_else(|| panic!("{} is not called here", tool.name));
            let answer =
                serde_json::to_string(&call(&client, &tool.name, args.clone()).await).unwrap();
            assert!(
                !answer.contains(SECRET_VALUE),
                "{} leaked the value",
                tool.name
            );
            if tool.name == "object_yaml" {
                assert!(answer.contains("password: (hidden)"), "{answer}");
            }
        }
        assert_eq!(cluster.inspect(|s| s.secret_reads), Some(0));
    }

    /// Every path a tool may read is a `GET` of the API, and none reveals, plans, applies, opens
    /// a terminal or streams.
    #[test]
    fn the_allowlist_is_only_reads() {
        let api = zeabur_api::openapi();
        for template in zeabur_mcp::ALLOWLIST {
            let item = api
                .paths
                .paths
                .get(*template)
                .unwrap_or_else(|| panic!("{template} is not an API path"));
            assert!(item.get.is_some(), "{template} is not a GET");
            for word in ["reveal", "plan", "apply", "exec", "follow"] {
                assert!(!template.contains(word), "{template}");
            }
        }
    }
}
