//! The MCP server: the tools, each a few reads from [`ALLOWLIST`](crate::ALLOWLIST).
//!
//! The protocol is rmcp's, the official Rust SDK [rmcp]: it answers both the stateless
//! revision 2026-07-28 (`server/discover`, the version in every request's `_meta`) and clients
//! still on 2025-11-25 (`initialize`, `ping`) [mcp-versioning]. This module only declares the
//! tools.
//!
//! A tool's answer is its JSON as compact text and as `structuredContent`. A refusal or failure
//! is a result with `isError: true` whose content is the API's problem unchanged, so a model
//! sees kube-apiserver's answer, the right that was `denied` and the `fix` [mcp-tools]; an
//! unknown tool or malformed arguments are protocol errors, which rmcp answers.
//!
//! [rmcp]: https://github.com/modelcontextprotocol/rust-sdk
//! [mcp-versioning]: https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning
//! [mcp-tools]: https://modelcontextprotocol.io/specification/2026-07-28/server/tools

use std::sync::Arc;

use engawa_modules::prometheus_api::PrometheusRange;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Implementation, ServerCapabilities, ServerConfig};
use rmcp::schemars::JsonSchema;
use rmcp::{ErrorData, ServerHandler, tool, tool_handler, tool_router};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::Reader;
use crate::reads::{
    CAPABILITIES, CLUSTER, CLUSTER_GAPS, EVENTS, HISTORY, LOGS, OBJECT, PROJECT, PROJECTS, SERVICE,
    UPGRADES, USAGE, path_and_query,
};

/// Log lines a `pod_logs` call returns unless it asks for others.
const DEFAULT_TAIL_LINES: u32 = 100;
/// The most log lines one call returns, to keep a model's context for the answer.
const MAX_TAIL_LINES: u32 = 1000;
/// Events a `project_events` call returns unless it asks for others.
const DEFAULT_EVENTS: u32 = 50;

/// What the server tells a client about itself, for the model.
const INSTRUCTIONS: &str = "Read-only view of a Kubernetes cluster as the user's kubeconfig \
sees it, through Engawa. A project is a namespace; a service is a workload (Deployment, \
StatefulSet, DaemonSet, Job, CronJob). Start with list_projects, then get_project. To explain a \
failing service: get_service, project_events, then pod_logs with previous=true for the run that \
crashed. Diagnoses are message keys with parameters. Nothing here can change the cluster or show \
a Secret's values.";

/// The MCP server over a [`Reader`].
pub struct Server<R> {
    reader: Arc<R>,
    tool_router: ToolRouter<Self>,
}

impl<R> std::fmt::Debug for Server<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server").finish_non_exhaustive()
    }
}

impl<R> Clone for Server<R> {
    fn clone(&self) -> Self {
        Self {
            reader: Arc::clone(&self.reader),
            tool_router: self.tool_router.clone(),
        }
    }
}

/// A project.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct ProjectArgs {
    /// The project's name (its namespace).
    project: String,
}

/// A service of a project.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct ServiceArgs {
    /// The project's name (its namespace).
    project: String,
    /// The workload kind: `deployment`, `statefulset`, `daemonset`, `job` or `cronjob`.
    kind: String,
    /// The workload's name.
    name: String,
}

/// A project's events.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct EventsArgs {
    /// The project's name (its namespace).
    project: String,
    /// How many of the newest events (default 50, at most 500).
    limit: Option<u32>,
}

/// A container's log.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct LogsArgs {
    /// The project's name (its namespace).
    project: String,
    /// The pod's name, as `get_project` or `get_service` lists it.
    pod: String,
    /// The container; needed when the pod has several.
    container: Option<String>,
    /// Only the last lines (default 100, at most 1000).
    tail_lines: Option<u32>,
    /// The previous run of the container, the one that crashed before the last restart.
    #[serde(default)]
    previous: bool,
}

/// A service's history.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct HistoryArgs {
    /// The project's name (its namespace).
    project: String,
    /// The workload kind: `deployment`, `statefulset`, `daemonset`, `job` or `cronjob`.
    kind: String,
    /// The workload's name.
    name: String,
    /// How far back: `1h` (the default), `6h`, `24h` or `7d`.
    #[serde(default)]
    range: PrometheusRange,
}

/// One object of a project.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct ObjectArgs {
    /// The project's name (its namespace).
    project: String,
    /// The API group, `core` for the empty group (Pods, Secrets, ConfigMaps, Services).
    group: String,
    /// The API version, such as `v1`.
    version: String,
    /// The resource, lower-case plural, such as `deployments` or `secrets`.
    resource: String,
    /// The object's name.
    name: String,
}

#[tool_router]
impl<R: Reader> Server<R> {
    /// A server answering from `reader`.
    pub fn new(reader: R) -> Self {
        Self {
            reader: Arc::new(reader),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "The cluster: Kubernetes version and distribution, upgrades (end of life, \
version skew, newer versions), what the cluster is missing, and installed components when the \
user may read them all."
    )]
    async fn cluster_summary(&self) -> Result<CallToolResult, ErrorData> {
        let cluster = match self.read(CLUSTER, &[], &[]).await {
            Ok(cluster) => cluster,
            Err(problem) => return Ok(CallToolResult::structured_error(problem)),
        };
        let mut summary = Map::from_iter([("cluster".to_owned(), cluster)]);
        let mut unavailable = Map::new();
        for (key, template) in [
            ("upgrades", UPGRADES),
            ("gaps", CLUSTER_GAPS),
            ("capabilities", CAPABILITIES),
        ] {
            match self.read(template, &[], &[]).await {
                Ok(value) => summary.insert(key.to_owned(), value),
                Err(problem) => unavailable.insert(key.to_owned(), problem),
            };
        }
        if !unavailable.is_empty() {
            summary.insert("unavailable".to_owned(), Value::Object(unavailable));
        }
        Ok(CallToolResult::structured(Value::Object(summary)))
    }

    #[tool(
        description = "The projects (namespaces) the user can see, with each adopted project's \
health and its services' health."
    )]
    async fn list_projects(&self) -> Result<CallToolResult, ErrorData> {
        Ok(answer(self.read(PROJECTS, &[], &[]).await))
    }

    #[tool(
        description = "One project's services: health, replicas, restarts, each pod's container \
states with the last termination (exit code, reason), and diagnoses of what is failing."
    )]
    async fn get_project(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(answer(
            self.read(PROJECT, &[&args.project], &[])
                .await
                .map(|d| project(&d)),
        ))
    }

    #[tool(
        description = "One service in full: its containers (image, ports, env, mounts, probes), \
pods with container states and last termination, endpoints, routes and diagnoses."
    )]
    async fn get_service(
        &self,
        Parameters(args): Parameters<ServiceArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(answer(
            self.read(SERVICE, &[&args.project, &args.kind, &args.name], &[])
                .await,
        ))
    }

    #[tool(description = "A project's newest events (Kubernetes events), newest first.")]
    async fn project_events(
        &self,
        Parameters(args): Parameters<EventsArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let limit = args.limit.unwrap_or(DEFAULT_EVENTS);
        Ok(answer(
            self.read(EVENTS, &[&args.project], &[("limit", limit.to_string())])
                .await,
        ))
    }

    #[tool(
        description = "The last lines of a container's log. previous=true reads the run before \
the last restart, which shows why a crash-looping container exited."
    )]
    async fn pod_logs(
        &self,
        Parameters(args): Parameters<LogsArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let tail_lines = args
            .tail_lines
            .unwrap_or(DEFAULT_TAIL_LINES)
            .min(MAX_TAIL_LINES);
        let mut query = vec![("tailLines", tail_lines.to_string())];
        if let Some(container) = args.container {
            query.push(("container", container));
        }
        if args.previous {
            query.push(("previous", "true".to_owned()));
        }
        Ok(answer(
            self.read(LOGS, &[&args.project, &args.pod], &query).await,
        ))
    }

    #[tool(
        description = "One object of a project as YAML, without status. Secret values are \
hidden; this server cannot show them."
    )]
    async fn object_yaml(
        &self,
        Parameters(args): Parameters<ObjectArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(answer(
            self.read(
                OBJECT,
                &[
                    &args.project,
                    &args.group,
                    &args.version,
                    &args.resource,
                    &args.name,
                ],
                &[],
            )
            .await,
        ))
    }

    #[tool(
        description = "What each pod of a project uses now, from metrics-server: CPU in \
millicores and memory (the working set) in bytes, per pod and container. state says when \
metrics-server is not installed, not readable or not answering; a running pod missing from pods \
has no sample yet. Compare with the requests and limits get_service shows."
    )]
    async fn project_usage(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(answer(self.read(USAGE, &[&args.project], &[]).await))
    }

    #[tool(
        description = "What a service's pods used over the last 1h, 6h, 24h or 7d, from the \
cluster's metrics store: CPU (millicores) and memory (bytes) per pod at each of times, null \
where a pod had no sample, and restarts (oomKilled when it ran out of memory; absent when the \
store cannot tell). state says when there is no store or it may not be read."
    )]
    async fn service_history(
        &self,
        Parameters(args): Parameters<HistoryArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(answer(
            self.read(
                HISTORY,
                &[&args.project, &args.kind, &args.name],
                &[("range", args.range.as_str().to_owned())],
            )
            .await,
        ))
    }

    /// `GET`s `template` with `values` in place of its parameters.
    async fn read(
        &self,
        template: &'static str,
        values: &[&str],
        query: &[(&str, String)],
    ) -> Result<Value, Value> {
        let path = path_and_query(template, values, query)?;
        self.reader.get(&path).await
    }
}

#[tool_handler]
#[expect(
    clippy::unused_async_trait_impl,
    reason = "rmcp's `tool_handler` writes `async` methods that need no `.await`"
)]
impl<R: Reader> ServerHandler for Server<R> {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("engawa", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

/// A read's value as a tool result, or its problem as an error result.
fn answer(read: Result<Value, Value>) -> CallToolResult {
    match read {
        Ok(value) => CallToolResult::structured(value),
        Err(problem) => CallToolResult::structured_error(problem),
    }
}

/// The parts of a project document that say how its services are doing.
///
/// The document also holds every resource, relation and connection, which a question about
/// health does not need; `get_service` has a service in full.
fn project(document: &Value) -> Value {
    let mut project = Map::new();
    project.insert(
        "project".to_owned(),
        document.get("namespace").cloned().unwrap_or(Value::Null),
    );
    project.insert(
        "services".to_owned(),
        array(document, "services").iter().map(service).collect(),
    );
    if let Some(unreadable) = document.get("unreadable") {
        project.insert("unreadable".to_owned(), unreadable.clone());
    }
    Value::Object(project)
}

/// One service, as [`project`] keeps it: its pods' states, and its diagnoses without evidence.
fn service(service: &Value) -> Value {
    let pods: Vec<Value> = array(service, "pods")
        .iter()
        .map(|pod| {
            pick(
                pod,
                &[
                    "name",
                    "phase",
                    "ready",
                    "restarts",
                    "reason",
                    "containerStatuses",
                ],
            )
        })
        .collect();
    let restarts: i64 = pods
        .iter()
        .filter_map(|pod| pod.get("restarts").and_then(Value::as_i64))
        .sum();
    let diagnoses: Vec<Value> = array(service, "diagnoses")
        .iter()
        .map(|d| {
            pick(
                d,
                &["kind", "container", "cause", "fix", "lastSeen", "pods"],
            )
        })
        .collect();
    let mut kept = pick(service, &["kind", "name", "status", "replicas", "image"]);
    if let Value::Object(kept) = &mut kept {
        kept.insert("restarts".to_owned(), restarts.into());
        kept.insert("pods".to_owned(), Value::Array(pods));
        kept.insert("diagnoses".to_owned(), Value::Array(diagnoses));
    }
    kept
}

/// The array `value[key]`, or none.
fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// The members of `value` named in `keys` that are present and not null.
fn pick(value: &Value, keys: &[&str]) -> Value {
    Value::Object(
        keys.iter()
            .filter_map(|&key| {
                value
                    .get(key)
                    .filter(|v| !v.is_null())
                    .map(|v| (key.to_owned(), v.clone()))
            })
            .collect(),
    )
}
