//! `zeaburctl mcp` against a real kube-apiserver, as a person with a namespace-scoped role: a
//! session in each protocol revision (2025-11-25's `initialize`, 2026-07-28's
//! `server/discover`) with the built binary over stdio, through rmcp's client, finds the
//! crash-looping service with its restart count and how its last run ended, and reads the log of
//! the run that crashed.
//!
//! Needs a disposable cluster where the current kubeconfig context is cluster admin, with
//! `busybox:1.37` available to its nodes (CONTRIBUTING.md, "Tests"); the admin connection only
//! prepares the namespace and the person.

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    use kube::config::Kubeconfig;
    use rmcp::model::{CallToolRequestParams, ProtocolVersion};
    use rmcp::service::{RoleClient, RunningService};
    use rmcp::{ClientLifecycleMode, ClientServiceExt as _};
    use serde_json::{Value, json};
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
    use tokio::process::{Child, Command};
    use tokio::task::JoinHandle;
    use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
    use zeabur_test_support::sandbox::{PERSON, Sandbox, create};

    /// A kubeconfig for the person: the admin context's cluster, with their token.
    fn person_kubeconfig(token: &str) -> Value {
        let admin = Kubeconfig::read().unwrap();
        let current = admin.current_context.clone().unwrap();
        let cluster_name = admin
            .contexts
            .iter()
            .find(|c| c.name == current)
            .and_then(|c| c.context.as_ref())
            .map(|c| c.cluster.clone())
            .unwrap();
        let cluster = admin
            .clusters
            .iter()
            .find(|c| c.name == cluster_name)
            .unwrap();
        json!({
            "apiVersion": "v1",
            "kind": "Config",
            "clusters": [cluster],
            "users": [{"name": PERSON, "user": {"token": token}}],
            "contexts": [{"name": PERSON, "context": {"cluster": cluster_name, "user": PERSON}}],
            "current-context": PERSON,
        })
    }

    /// `zeaburctl mcp`, spoken to over its standard input and output with rmcp's client, as
    /// `src/mcp.rs`'s tests speak to the server in process.
    struct Session {
        child: Child,
        client: RunningService<RoleClient, ()>,
        /// Passes the server's output on to the client, line by line, and fails on a line that
        /// is not JSON: rmcp's client skips such a line, and an MCP client may not.
        output: JoinHandle<()>,
    }

    impl Session {
        async fn start(kubeconfig: &Path, lifecycle: ClientLifecycleMode) -> Self {
            let mut child = Command::new(env!("CARGO_BIN_EXE_zeaburctl"))
                .arg("mcp")
                .env("KUBECONFIG", kubeconfig)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .kill_on_drop(true)
                .spawn()
                .unwrap();
            let stdin = child.stdin.take().unwrap();
            let stdout = child.stdout.take().unwrap();
            let (mut to_client, from_server) = tokio::io::duplex(64 * 1024);
            let output = tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Some(line) = lines.next_line().await.unwrap() {
                    serde_json::from_str::<Value>(&line)
                        .unwrap_or_else(|e| panic!("stdout carried a non-JSON line {line:?}: {e}"));
                    let passed = to_client.write_all(format!("{line}\n").as_bytes()).await;
                    if passed.is_err() {
                        break;
                    }
                }
            });
            let client = ().serve_with_lifecycle((from_server, stdin), lifecycle).await.unwrap();
            Self {
                child,
                client,
                output,
            }
        }

        /// Calls `tool` and returns its result as JSON.
        async fn call(&self, tool: &str, arguments: Value) -> Value {
            let Value::Object(arguments) = arguments else {
                panic!("arguments are an object");
            };
            let result = self
                .client
                .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(arguments))
                .await
                .unwrap();
            serde_json::to_value(result).unwrap()
        }

        /// Closes the session; the server must exit with 0.
        async fn finish(mut self) {
            self.client.cancel().await.unwrap();
            let status = self.child.wait().await.unwrap();
            assert!(status.success(), "{status}");
            self.output.await.unwrap();
        }
    }

    #[tokio::test]
    #[ignore = "needs a disposable cluster"]
    async fn mcp_explains_a_crash_looping_service_in_both_revisions() {
        let sandbox = Sandbox::new(
            "zeabur-mcp",
            json!([
                {"apiGroups": ["", "apps", "batch", "networking.k8s.io", "events.k8s.io"],
                 "resources": ["*"], "verbs": ["get", "list"]}
            ]),
        )
        .await;
        let ns = sandbox.namespace();
        let line = format!("zeabur-mcp-crash-{ns}");

        create(
            &sandbox.admin_api::<Deployment>(),
            json!({"metadata": {"name": "crasher"},
                   "spec": {"replicas": 1, "selector": {"matchLabels": {"app": "crasher"}},
                            "template": {"metadata": {"labels": {"app": "crasher"}},
                                         "spec": {"containers": [{"name": "crasher",
                                             "image": "busybox:1.37",
                                             "command": ["sh", "-c", format!("echo {line}; exit 3")]}]}}}}),
        )
        .await;

        let kubeconfig = std::env::temp_dir().join(format!("{ns}.kubeconfig"));
        std::fs::write(&kubeconfig, person_kubeconfig(sandbox.token()).to_string()).unwrap();

        let eras = [
            (
                ClientLifecycleMode::Initialize,
                ProtocolVersion::V_2025_11_25,
            ),
            (
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                },
                ProtocolVersion::V_2026_07_28,
            ),
        ];
        for (era, version) in eras {
            let session = Session::start(&kubeconfig, era.clone()).await;
            let negotiated = session
                .client
                .peer_info()
                .map(|info| info.protocol_version.clone());
            assert_eq!(negotiated, Some(version), "{era:?}");

            // Wait for the kubelet to restart it once, and to report how the run ended.
            let deadline = Instant::now() + Duration::from_secs(180);
            let (service, pod) = loop {
                let result = session.call("get_project", json!({"project": ns})).await;
                assert_eq!(result["isError"], false, "{era:?}: {result}");
                let project = &result["structuredContent"];
                let service = project["services"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|s| s["name"] == "crasher")
                    .cloned();
                if let Some(service) = service
                    && service["restarts"].as_i64().unwrap_or(0) >= 1
                    && service["pods"][0]["containerStatuses"][0]["lastTermination"].is_object()
                {
                    let pod = service["pods"][0]["name"].as_str().unwrap().to_owned();
                    break (service, pod);
                }
                assert!(
                    Instant::now() < deadline,
                    "{era:?}: no restart reported; last seen {project}"
                );
                tokio::time::sleep(Duration::from_secs(2)).await;
            };
            let last = &service["pods"][0]["containerStatuses"][0]["lastTermination"];
            assert_eq!(last["exitCode"], 3, "{era:?}: {service}");
            assert_eq!(last["reason"], "Error", "{era:?}: {service}");

            // The run that printed `line` is the previous one while the container waits to restart,
            // and the current one while it stays terminated during a back-off; then the kubelet
            // has already removed the run before it ("unable to retrieve container logs"), which
            // kube-apiserver passes on as a `log-unavailable` problem; ask both until one has it.
            'found: loop {
                for previous in [true, false] {
                    let result = session
                        .call(
                            "pod_logs",
                            json!({"project": ns, "pod": pod, "previous": previous}),
                        )
                        .await;
                    let logs = &result["structuredContent"];
                    if result["isError"] == true {
                        assert_eq!(logs["kind"], "log-unavailable", "{era:?}: {result}");
                    } else if logs["lines"].as_array().unwrap().contains(&json!(line)) {
                        break 'found;
                    }
                    assert!(Instant::now() < deadline, "{era:?}: {logs}");
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            session.finish().await;
        }
        let _ignored = std::fs::remove_file(&kubeconfig);
    }
}
