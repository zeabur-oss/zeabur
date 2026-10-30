//! Everything about one service that someone helping with it would ask for, as Markdown to copy.
//!
//! One text for an issue or a message: its problems, the way to it hop by hop, its recent events,
//! the end of the log of each container's previous run, and its objects.
//!
//! # What it never holds
//!
//! - A Secret: neither its values nor the object (a Secret's values never reach Zeabur's reads
//!   except through `reveal`).
//! - A variable's value: every `env[].value` in the objects shows `(hidden)`, so a password typed
//!   into a literal variable is not pasted along; names and where each value comes from stay. A
//!   variable read from a Secret has no value in the object to begin with.
//! - The `kubectl.kubernetes.io/last-applied-configuration` annotation, `managedFields` and
//!   `status`, as the YAML view leaves them out (`zeabur_graph::manifest`).
//!
//! Logs are included as the containers wrote them: a program that prints a secret prints it here
//! too. They are bounded: the last 50 lines of each container that restarted, in at most three
//! pods, each line cut to 500 characters.
//!
//! Sentences are their keys with their values, as the API gives them (`diagnosis.crashLoop.cause.
//! exitCode`, `exitCode=1`): the text reads the same whoever's language made it.

use axum::Json;
use axum::extract::{Path, State};
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_graph::manifest::{HIDDEN, to_yaml};
use zeabur_kube::{Cluster, LogOptions};
use zeabur_model::{Message, Origin, Service, WorkloadKind, id};

use crate::error::ApiError;
use crate::handlers::{AppState, Person};
use crate::projects::{ProjectRead, not_found};
use crate::services::trace::{ServiceTrace, of_service};
use crate::views::UnreadableView;

const LOG_LINES: i64 = 50;
const LOG_PODS: usize = 3;
const LINE_CHARS: usize = 500;
const EVENTS: usize = 20;

/// The Markdown text, line by line.
#[derive(Debug, Default)]
struct Doc(String);

impl Doc {
    fn line(&mut self, text: impl AsRef<str>) {
        self.0.push_str(text.as_ref());
        self.0.push('\n');
    }

    fn push_str(&mut self, text: &str) {
        self.0.push_str(text);
    }
}

/// A service's evidence, as Markdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceBundle {
    /// `projects/<project>/services/<kind>/<name>/evidence`.
    pub id: String,
    /// When it was read, RFC 3339.
    pub generated_at: String,
    /// The text to copy.
    pub markdown: String,
    /// Reads the person was refused; the text names them too.
    pub unreadable: Vec<UnreadableView>,
}

fn word<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn sentence(message: &Message) -> String {
    let params: Vec<String> = message
        .params
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    if params.is_empty() {
        format!("`{}`", message.key)
    } else {
        format!("`{}` ({})", message.key, params.join(", "))
    }
}

fn origin(origin: &Origin) -> String {
    let path = origin
        .field_path
        .as_ref()
        .map_or_else(String::new, |path| format!(" `{path}`"));
    let value = origin
        .value
        .as_ref()
        .map_or_else(String::new, |value| format!(" = `{value}`"));
    format!("{}/{}{path}{value}", origin.kind, origin.name)
}

/// A code fence longer than any run of backticks in `text`.
fn fenced(text: &str, language: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    format!("{fence}{language}\n{}\n{fence}\n", text.trim_end())
}

/// `object` with every `env[].value` hidden, in every container list of its pod template.
fn hide_values(mut object: Value) -> Value {
    for pointer in [
        "/spec/template/spec",
        "/spec/jobTemplate/spec/template/spec",
    ] {
        let Some(spec) = object.pointer_mut(pointer) else {
            continue;
        };
        for list in ["containers", "initContainers", "ephemeralContainers"] {
            for container in spec
                .get_mut(list)
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                for variable in container
                    .get_mut("env")
                    .and_then(Value::as_array_mut)
                    .into_iter()
                    .flatten()
                {
                    if let Some(value) = variable.get_mut("value") {
                        *value = Value::String(HIDDEN.to_owned());
                    }
                }
            }
        }
    }
    object
}

fn problems(out: &mut Doc, service: &Service) {
    out.push_str("\n## Problems\n\n");
    if service.diagnoses.is_empty() {
        out.push_str("None found.\n");
    }
    for d in &service.diagnoses {
        out.line(format!(
            "- **{}**{}{}: where the cause lies: {}{}; who can fix it: {}",
            word(&d.kind),
            if d.container.is_empty() {
                String::new()
            } else {
                format!(" in container `{}`", d.container)
            },
            if d.pods.is_empty() {
                String::new()
            } else {
                format!(" (pods {})", d.pods.join(", "))
            },
            word(&d.fault.origin),
            d.fault
                .component
                .as_ref()
                .map_or_else(String::new, |c| format!(" ({c})")),
            word(&d.fault.fixer),
        ));
        out.line(format!("  - cause: {}", sentence(&d.cause)));
        out.line(format!("  - why: {}", sentence(&d.why)));
        out.line(format!("  - fix: {}", sentence(&d.fix)));
        if let Some(incident) = &d.incident {
            out.line(format!("  - part of incident `{incident}`"));
        }
        for e in &d.evidence {
            out.line(format!("  - evidence: {}", origin(e)));
        }
    }
}

fn routes(out: &mut Doc, trace: &ServiceTrace) {
    out.push_str("\n## The way to it\n");
    if trace.routes.is_empty() {
        out.push_str("\nNo domain or private address.\n");
    }
    for route in &trace.routes {
        out.line(format!("\n### {} `{}` via {}: {}\n\n| Hop | Status | Expected | Saw | What was seen |\n|---|---|---|---|---|",
            word(&route.trace.kind),
            route.trace.address,
            origin(&route.trace.via),
            word(&route.trace.status),
        ));
        for hop in &route.trace.hops {
            let cell = |s: Option<&String>| s.map_or_else(String::new, |s| s.replace('|', "\\|"));
            out.line(format!(
                "| {} | {} | {} | {} | {} |",
                word(&hop.step),
                word(&hop.status),
                cell(hop.expected.as_ref()),
                cell(hop.saw.as_ref()),
                sentence(&hop.message).replace('|', "\\|"),
            ));
        }
    }
}

fn events(out: &mut Doc, read: &ProjectRead, service: &Service) {
    let pods: Vec<&str> = service.pods.iter().map(|p| p.name.as_str()).collect();
    let mut mine: Vec<&zeabur_gaps::Event> = read
        .events
        .iter()
        .filter(|e| {
            e.regarding.as_ref().is_some_and(|r| {
                (r.kind == "Pod" && pods.contains(&r.name.as_str()))
                    || (r.name == service.workload.name
                        && WorkloadKind::parse(&r.kind).is_some_and(|k| k == service.workload.kind))
            })
        })
        .collect();
    mine.sort_by_key(|e| std::cmp::Reverse(e.last_seen));
    out.push_str("\n## Recent events\n\n");
    if mine.is_empty() {
        out.push_str("None kept (events last about an hour).\n");
    }
    for e in mine.into_iter().take(EVENTS) {
        out.line(format!(
            "- {} {} {} on {} ×{}: {}",
            e.last_seen.map_or_else(String::new, |t| t.to_string()),
            e.kind,
            e.reason,
            e.regarding.as_ref().map_or_else(String::new, origin),
            e.count,
            e.note.replace('\n', " "),
        ));
    }
}

async fn logs(out: &mut Doc, cluster: &dyn Cluster, project: &str, service: &Service) {
    out.push_str("\n## Previous run logs\n");
    let restarted: Vec<(&str, &str)> = service
        .pods
        .iter()
        .flat_map(|pod| {
            pod.container_statuses
                .iter()
                .filter(|c| c.restarts > 0)
                .map(move |c| (pod.name.as_str(), c.name.as_str()))
        })
        .collect();
    let mut pods_seen: Vec<&str> = Vec::new();
    let mut any = false;
    for (pod, container) in restarted {
        if !pods_seen.contains(&pod) {
            if pods_seen.len() == LOG_PODS {
                continue;
            }
            pods_seen.push(pod);
        }
        any = true;
        let options = LogOptions {
            container: Some(container.to_owned()),
            tail_lines: Some(LOG_LINES),
            previous: true,
            ..LogOptions::default()
        };
        out.line(format!(
            "\n### `{pod}` / `{container}`, the last {LOG_LINES} lines\n"
        ));
        match cluster.pod_logs(project, pod, &options).await {
            Ok(text) => {
                let cut: Vec<String> = text
                    .lines()
                    .map(|l| l.chars().take(LINE_CHARS).collect())
                    .collect();
                out.push_str(&fenced(&cut.join("\n"), "text"));
            }
            Err(error) => {
                out.line(format!("Not read: {error}"));
            }
        }
    }
    if !any {
        out.push_str("\nNo container restarted.\n");
    }
}

fn objects(out: &mut Doc, read: &ProjectRead, service: &Service) {
    out.push_str("\n## Objects\n\nVariable values show `(hidden)`; Secrets are not included.\n");
    let mut found: Vec<Value> = zeabur_graph::workloads(&read.snapshot)
        .into_iter()
        .filter(|w| w.kind() == service.workload.kind && w.name() == service.workload.name)
        .filter_map(|w| match w {
            zeabur_graph::Workload::Deployment(o) => serde_json::to_value(o).ok(),
            zeabur_graph::Workload::StatefulSet(o) => serde_json::to_value(o).ok(),
            zeabur_graph::Workload::DaemonSet(o) => serde_json::to_value(o).ok(),
            zeabur_graph::Workload::Job(o) => serde_json::to_value(o).ok(),
            zeabur_graph::Workload::CronJob(o) => serde_json::to_value(o).ok(),
        })
        .collect();
    found.extend(
        read.snapshot
            .services
            .iter()
            .filter(|s| {
                s.metadata
                    .name
                    .as_ref()
                    .is_some_and(|n| service.endpoint_names.contains(n))
            })
            .filter_map(|s| serde_json::to_value(s).ok()),
    );
    for object in found {
        let kind = object
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let name = object
            .pointer("/metadata/name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let yaml = to_yaml(&hide_values(object)).unwrap_or_default();
        out.line(format!("\n### {kind} `{name}`\n"));
        out.push_str(&fenced(&yaml, "yaml"));
    }
}

/// The evidence bundle of `service`.
pub(crate) async fn bundle(
    cluster: &dyn Cluster,
    read: &ProjectRead,
    service: &Service,
    trace: &ServiceTrace,
    generated_at: &str,
) -> String {
    let project = &read.document.project.namespace;
    let mut out = Doc(format!(
        "# {} `{}` in project `{}`\n\nRead {generated_at} by Zeabur {}. Status: {}{}.\n",
        word(&service.workload.kind),
        service.workload.name,
        project,
        env!("CARGO_PKG_VERSION"),
        word(&service.status.word),
        service
            .workload
            .replicas
            .as_ref()
            .map_or_else(String::new, |r| format!(
                ", {} of {} ready",
                r.ready, r.desired
            )),
    ));
    problems(&mut out, service);
    routes(&mut out, trace);
    events(&mut out, read, service);
    logs(&mut out, cluster, project, service).await;
    objects(&mut out, read, service);
    if !trace.unreadable.is_empty() {
        out.push_str("\n## Not readable with your rights\n\n");
        for u in &trace.unreadable {
            out.line(format!("- `{}`: {}", u.resource, u.kubernetes.message));
        }
    }
    out.0
}

/// Everything about one service someone helping would ask for, as Markdown to copy: its
/// problems, the way to it hop by hop, recent events, the end of each restarted container's
/// previous log, and its objects with variable values hidden. Never a Secret.
#[utoipa::path(get, path = "/api/v1/projects/{project}/services/{kind}/{name}/evidence", tag = "services",
    operation_id = "getServiceEvidence",
    params(("project" = String, Path),
           ("kind" = String, Path, description = "deployment, statefulset, daemonset, job or cronjob; any case"),
           ("name" = String, Path)),
    responses((status = 200, body = EvidenceBundle), (status = 404, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn evidence(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path((project, kind, name)): Path<(String, String, String)>,
) -> Result<Json<EvidenceBundle>, ApiError> {
    let Some(workload_kind) = WorkloadKind::parse(&kind) else {
        return Err(not_found(&project, &kind, &name));
    };
    let cluster = cluster.as_ref();
    let read = crate::projects::read(cluster, &project).await?;
    let service = read
        .document
        .project
        .services
        .iter()
        .find(|s| s.workload.kind == workload_kind && s.workload.name == name)
        .ok_or_else(|| not_found(&project, workload_kind.as_str(), &name))?;
    let trace = of_service(&state, cluster, &read, service).await?;
    let generated_at =
        zeabur_kube::k8s_openapi::jiff::Timestamp::from_second((state.clock)().as_second())
            .unwrap_or(zeabur_kube::k8s_openapi::jiff::Timestamp::UNIX_EPOCH)
            .to_string();
    let markdown = bundle(cluster, &read, service, &trace, &generated_at).await;
    Ok(Json(EvidenceBundle {
        id: format!("{}/evidence", id::service(&project, workload_kind, &name)),
        generated_at,
        markdown,
        unreadable: trace.unreadable,
    }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_fence_outgrows_the_backticks_inside() {
        assert_eq!(fenced("a", "text"), "```text\na\n```\n");
        assert_eq!(fenced("```\nx", ""), "````\n```\nx\n````\n");
    }

    #[test]
    fn every_literal_variable_value_is_hidden() {
        let object = json!({"spec": {"template": {"spec": {
            "containers": [{"name": "app", "env": [
                {"name": "PASSWORD", "value": "hunter2"},
                {"name": "TOKEN", "valueFrom": {"secretKeyRef": {"name": "s", "key": "t"}}}]}],
            "initContainers": [{"name": "init", "env": [{"name": "A", "value": "b"}]}]}}}});
        let hidden = hide_values(object).to_string();
        assert!(!hidden.contains("hunter2"));
        assert!(!hidden.contains("\"b\""));
        assert!(hidden.contains("secretKeyRef"));
    }
}
