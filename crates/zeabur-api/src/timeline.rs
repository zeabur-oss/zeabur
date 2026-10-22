//! What changed, and what went wrong, in a service or a project, newest first: one timeline read
//! from what the cluster keeps, with no store of Zeabur's own.
//!
//! # Sources
//!
//! - **Releases**: a Deployment's `ReplicaSets` and a StatefulSet's or DaemonSet's
//!   `ControllerRevisions` [k8s-deployment-history], each with its creation time, images and
//!   `kubernetes.io/change-cause` (`zeabur_plan::releases`).
//! - **Writes**: the workload's `metadata.managedFields` [k8s-ssa-fields]. Each entry names a field
//!   manager (`zeabur` for Zeabur's plans, `helm`, `argocd-controller`, `kubectl-edit`,
//!   `kube-controller-manager` writing `scale` for a `HorizontalPodAutoscaler` [k8s-hpa]), the
//!   fields it owns and the time of its *last* write. So one manager is one entry at its latest
//!   write: earlier writes by the same manager are not kept by Kubernetes, and the person behind a
//!   manager is not recorded at all. Status writes are left out.
//! - **Warnings**: the namespace's `Warning` events about the workload, its pods and its
//!   `ReplicaSets`, folded by reason: a crash loop, an eviction, a pod that cannot be scheduled
//!   (with the scheduler's reason), an image that cannot be pulled, a failing probe, a pod the
//!   quota refused. Events last an hour by default [k8s-event-ttl].
//! - **Terminations**: the last termination of each container the pods report
//!   (`lastState.terminated`, `state.terminated`), out-of-memory kills included; a normal exit of a
//!   Job's container is left out.
//!
//! # Reverting
//!
//! A release that is running now carries the plan that returns to the release before it (the
//! `rollback` change). A write carries none: `managedFields` keep who owns a field, not the value
//! it held before, so its inverse cannot be computed.
//!
//! [k8s-deployment-history]: https://kubernetes.io/docs/concepts/workloads/controllers/deployment/#checking-rollout-history-of-a-deployment
//! [k8s-ssa-fields]: https://kubernetes.io/docs/reference/using-api/server-side-apply/#field-management
//! [k8s-hpa]: https://kubernetes.io/docs/tasks/run-application/horizontal-pod-autoscale/
//! [k8s-event-ttl]: https://kubernetes.io/docs/reference/command-line-tools-reference/kube-apiserver/

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::Path;
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_graph::{Snapshot, Workload};
use zeabur_kube::k8s_openapi::api::apps::v1::{ControllerRevision, ReplicaSet};
use zeabur_kube::k8s_openapi::api::core::v1::Pod;
use zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ManagedFieldsEntry;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{Cluster, ClusterExt as _};
use zeabur_model::{Message, Origin, WorkloadKind, WorkloadRef, id};
use zeabur_plan::{Change, PlanRequest, Release};

use crate::error::ApiError;
use crate::handlers::Person;
use crate::incidents::allowed;
use crate::projects::not_found;
use crate::projects::writes::releases::History;
use crate::views::UnreadableView;

/// How many entries a timeline holds at most.
pub(crate) const LIMIT: usize = 200;

/// What an entry records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    /// A new pod template: a release.
    Release,
    /// A field manager's last write to the workload.
    Write,
    /// `Warning` events of one reason.
    Warning,
    /// A container that stopped.
    Termination,
}

/// The tool behind a field manager, by its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ManagerTool {
    /// Zeabur's plans.
    Zeabur,
    /// Helm.
    Helm,
    /// Argo CD.
    ArgoCd,
    /// Flux.
    Flux,
    /// kubectl.
    Kubectl,
    /// A `HorizontalPodAutoscaler` (kube-controller-manager writing `scale`).
    Autoscaler,
    /// Another Kubernetes controller.
    Controller,
    /// Anything else.
    Other,
}

/// Who wrote, as far as `managedFields` say.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Writer {
    /// The field manager.
    pub manager: String,
    /// The tool it is.
    pub tool: ManagerTool,
    /// `Apply` or `Update`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    /// The subresource written, such as `scale`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subresource: Option<String>,
}

/// A release, as an entry names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseRef {
    /// Its number.
    pub revision: i64,
    /// The `ReplicaSet` or `ControllerRevision`.
    pub name: String,
    /// Its images.
    pub images: Vec<String>,
    /// `kubernetes.io/change-cause`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_cause: Option<String>,
    /// It runs now.
    pub current: bool,
}

/// One thing that happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEntry {
    /// When, RFC 3339: the release's creation, the write, the last event, the termination.
    pub at: String,
    /// What it records.
    pub kind: EntryKind,
    /// The workload.
    pub workload: WorkloadRef,
    /// What happened, in one sentence: `timeline.release`, `timeline.write.<tool>`,
    /// `timeline.warning.<crashLoop|evicted|unschedulable|imagePull|probe|quota|other>`,
    /// `timeline.termination.<oomKilled|exited>`.
    pub message: Message,
    /// Who wrote, for a write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer: Option<Writer>,
    /// The fields the manager owns, for a write.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    /// The release.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<ReleaseRef>,
    /// How many times, for warnings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<i32>,
    /// The objects and fields it rests on: the events, the pod's container status.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<Origin>,
    /// The changes that undo it, to send to `plan`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revert: Option<PlanRequest>,
}

/// A timeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Timeline {
    /// `projects/<project>/timeline` or `projects/<project>/services/<kind>/<name>/timeline`.
    pub id: String,
    /// Newest first, at most 200.
    pub items: Vec<TimelineEntry>,
    /// Older entries were left out.
    pub truncated: bool,
    /// Reads the person was refused; their entries are missing.
    pub unreadable: Vec<UnreadableView>,
}

fn tool(entry: &ManagedFieldsEntry) -> ManagerTool {
    let manager = entry.manager.as_deref().unwrap_or_default();
    match manager {
        m if m == zeabur_adapters::FIELD_MANAGER => ManagerTool::Zeabur,
        "helm" => ManagerTool::Helm,
        "helm-controller" | "kustomize-controller" => ManagerTool::Flux,
        m if m.starts_with("argocd") => ManagerTool::ArgoCd,
        m if m.starts_with("kubectl") => ManagerTool::Kubectl,
        "kube-controller-manager" if entry.subresource.as_deref() == Some("scale") => {
            ManagerTool::Autoscaler
        }
        "kube-controller-manager" | "kube-scheduler" | "kubelet" => ManagerTool::Controller,
        _ => ManagerTool::Other,
    }
}

const fn tool_key(tool: ManagerTool) -> &'static str {
    match tool {
        ManagerTool::Zeabur => "zeabur",
        ManagerTool::Helm => "helm",
        ManagerTool::ArgoCd => "argoCd",
        ManagerTool::Flux => "flux",
        ManagerTool::Kubectl => "kubectl",
        ManagerTool::Autoscaler => "autoscaler",
        ManagerTool::Controller => "controller",
        ManagerTool::Other => "other",
    }
}

/// The fields a `fieldsV1` trie owns, to two levels under `spec` and one under `metadata`.
fn owned_fields(fields: &Value) -> Vec<String> {
    let keys = |node: &Value| -> Vec<String> {
        node.as_object()
            .into_iter()
            .flatten()
            .filter_map(|(k, _)| k.strip_prefix("f:").map(str::to_owned))
            .collect()
    };
    let mut out = Vec::new();
    for top in ["metadata", "spec"] {
        let Some(node) = fields.get(format!("f:{top}")) else {
            continue;
        };
        for key in keys(node) {
            let child = node.get(format!("f:{key}")).unwrap_or(&Value::Null);
            let deeper = keys(child);
            if top == "spec" && !deeper.is_empty() && key != "template" {
                out.extend(deeper.into_iter().map(|d| format!("spec.{key}.{d}")));
            } else {
                out.push(format!("{top}.{key}"));
            }
        }
    }
    out
}

fn writes(workload: Workload<'_>) -> Vec<TimelineEntry> {
    let me = workload.reference();
    workload
        .meta()
        .managed_fields
        .iter()
        .flatten()
        .filter(|entry| entry.subresource.as_deref() != Some("status"))
        .filter_map(|entry| {
            let at = entry.time.as_ref()?.0;
            let manager = entry.manager.clone()?;
            let tool = tool(entry);
            let fields = entry
                .fields_v1
                .as_ref()
                .map_or_else(Vec::new, |f| owned_fields(&f.0));
            Some(TimelineEntry {
                at: at.to_string(),
                kind: EntryKind::Write,
                workload: me.clone(),
                message: Message::new(&format!("timeline.write.{}", tool_key(tool)))
                    .with("manager", manager.clone())
                    .with("fields", fields.join(", ")),
                writer: Some(Writer {
                    manager,
                    tool,
                    operation: entry.operation.clone(),
                    subresource: entry.subresource.clone(),
                }),
                fields,
                release: None,
                count: None,
                evidence: vec![Origin::field(
                    me.kind.as_str(),
                    &me.name,
                    "metadata.managedFields".to_owned(),
                )],
                revert: None,
            })
        })
        .collect()
}

fn releases(workload: &WorkloadRef, releases: &[Release]) -> Vec<TimelineEntry> {
    releases
        .iter()
        .enumerate()
        .filter_map(|(index, release)| {
            let before = releases.get(index + 1);
            let revert = before
                .filter(|_| release.current)
                .map(|before| PlanRequest {
                    changes: vec![Change::Rollback {
                        workload: workload.clone(),
                        revision: before.revision,
                    }],
                });
            let mut message = Message::new("timeline.release")
                .with("revision", release.revision.to_string())
                .with("images", release.images.join(", "));
            if let Some(cause) = &release.change_cause {
                message = message.with("changeCause", cause.clone());
            }
            Some(TimelineEntry {
                at: release.created.clone()?,
                kind: EntryKind::Release,
                workload: workload.clone(),
                message,
                writer: None,
                fields: Vec::new(),
                release: Some(ReleaseRef {
                    revision: release.revision,
                    name: release.name.clone(),
                    images: release.images.clone(),
                    change_cause: release.change_cause.clone(),
                    current: release.current,
                }),
                count: None,
                evidence: vec![Origin::object(
                    if workload.kind == WorkloadKind::Deployment {
                        "ReplicaSet"
                    } else {
                        "ControllerRevision"
                    },
                    &release.name,
                )],
                revert,
            })
        })
        .collect()
}

/// Whether `pod` is named as a pod of `workload` or of one of its releases: the pods of earlier
/// releases are gone, but their events remain.
fn names_pod_of(workload: &WorkloadRef, releases: &[Release], pod: &str) -> bool {
    let suffix_of = |prefix: &str| {
        pod.strip_prefix(prefix)
            .and_then(|rest| rest.strip_prefix('-'))
    };
    let generated = |rest: &str| rest.len() == 5 && rest.chars().all(|c| c.is_ascii_alphanumeric());
    match workload.kind {
        WorkloadKind::Deployment => releases
            .iter()
            .any(|r| suffix_of(&r.name).is_some_and(generated)),
        WorkloadKind::StatefulSet => suffix_of(&workload.name)
            .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())),
        WorkloadKind::DaemonSet | WorkloadKind::Job => {
            suffix_of(&workload.name).is_some_and(generated)
        }
        WorkloadKind::CronJob => suffix_of(&workload.name).is_some_and(|rest| {
            rest.split_once('-').is_some_and(|(job, pod)| {
                !job.is_empty() && job.chars().all(|c| c.is_ascii_digit()) && generated(pod)
            })
        }),
    }
}

fn warning_variant(event: &zeabur_gaps::Event) -> &'static str {
    let note = event.note.to_ascii_lowercase();
    match event.reason.as_str() {
        "BackOff" if note.contains("image") => "imagePull",
        "BackOff" | "CrashLoopBackOff" => "crashLoop",
        "Evicted" | "Preempted" => "evicted",
        "FailedScheduling" => "unschedulable",
        "Failed" | "ErrImagePull" | "ImagePullBackOff" | "InspectFailed" => "imagePull",
        "Unhealthy" => "probe",
        "FailedCreate" => "quota",
        _ => "other",
    }
}

fn warnings(
    workload: &WorkloadRef,
    releases: &[Release],
    events: &[zeabur_gaps::Event],
) -> Vec<TimelineEntry> {
    let about = |event: &zeabur_gaps::Event| {
        event
            .regarding
            .as_ref()
            .is_some_and(|r| match r.kind.as_str() {
                "Pod" => names_pod_of(workload, releases, &r.name),
                "ReplicaSet" => releases.iter().any(|release| release.name == r.name),
                kind => {
                    WorkloadKind::parse(kind).is_some_and(|k| k == workload.kind)
                        && r.name == workload.name
                }
            })
    };
    let mut folded: BTreeMap<&str, (Timestamp, i32, &zeabur_gaps::Event, Vec<Origin>)> =
        BTreeMap::new();
    for event in events.iter().filter(|e| e.kind == "Warning" && about(e)) {
        let Some(at) = event.last_seen else {
            continue;
        };
        let entry = folded
            .entry(event.reason.as_str())
            .or_insert((at, 0, event, Vec::new()));
        entry.1 = entry.1.saturating_add(event.count);
        entry.3.push(event.origin.clone());
        if at >= entry.0 {
            entry.0 = at;
            entry.2 = event;
        }
    }
    folded
        .into_values()
        .map(|(at, count, latest, evidence)| {
            let object = latest
                .regarding
                .as_ref()
                .map_or_else(String::new, |r| format!("{}/{}", r.kind, r.name));
            TimelineEntry {
                at: at.to_string(),
                kind: EntryKind::Warning,
                workload: workload.clone(),
                message: Message::new(&format!("timeline.warning.{}", warning_variant(latest)))
                    .with("reason", latest.reason.clone())
                    .with("note", latest.note.clone())
                    .with("count", count.to_string())
                    .with("object", object),
                writer: None,
                fields: Vec::new(),
                release: None,
                count: Some(count),
                evidence,
                revert: None,
            }
        })
        .collect()
}

fn terminations(workload: &WorkloadRef, pods: &[&Pod]) -> Vec<TimelineEntry> {
    let mut out = Vec::new();
    for pod in pods {
        let name = pod.metadata.name.as_deref().unwrap_or_default();
        let statuses = pod.status.as_ref().into_iter().flat_map(|s| {
            s.container_statuses
                .iter()
                .flatten()
                .map(|c| ("containerStatuses", c))
                .chain(
                    s.init_container_statuses
                        .iter()
                        .flatten()
                        .map(|c| ("initContainerStatuses", c)),
                )
        });
        for (list, status) in statuses {
            let ended = [
                ("lastState", status.last_state.as_ref()),
                ("state", status.state.as_ref()),
            ];
            for (field, state) in ended {
                let Some(terminated) = state.and_then(|s| s.terminated.as_ref()) else {
                    continue;
                };
                if terminated.exit_code == 0 {
                    continue;
                }
                let Some(at) = terminated.finished_at.as_ref() else {
                    continue;
                };
                let reason = terminated.reason.clone().unwrap_or_default();
                let variant = if reason == "OOMKilled" {
                    "oomKilled"
                } else {
                    "exited"
                };
                out.push(TimelineEntry {
                    at: at.0.to_string(),
                    kind: EntryKind::Termination,
                    workload: workload.clone(),
                    message: Message::new(&format!("timeline.termination.{variant}"))
                        .with("pod", name)
                        .with("container", status.name.clone())
                        .with("reason", reason)
                        .with("exitCode", terminated.exit_code.to_string()),
                    writer: None,
                    fields: Vec::new(),
                    release: None,
                    count: None,
                    evidence: vec![Origin::field(
                        "Pod",
                        name,
                        format!("status.{list}[name={}].{field}.terminated", status.name),
                    )],
                    revert: None,
                });
            }
        }
    }
    out
}

/// What the timeline of one namespace is read from.
pub(crate) struct Sources<'a> {
    pub(crate) snapshot: &'a Snapshot,
    pub(crate) events: &'a [zeabur_gaps::Event],
    pub(crate) history: &'a History,
}

/// The entries of `workload`, unsorted.
pub(crate) fn of_workload(sources: &Sources<'_>, workload: Workload<'_>) -> Vec<TimelineEntry> {
    let me = workload.reference();
    let held = if matches!(me.kind, WorkloadKind::Job | WorkloadKind::CronJob) {
        Vec::new()
    } else {
        sources.history.of(workload, &sources.snapshot.pods)
    };
    let pods: Vec<&Pod> = sources
        .snapshot
        .pods
        .iter()
        .filter(|p| {
            zeabur_graph::pods_of(sources.snapshot, workload)
                .iter()
                .any(|s| Some(s.name.as_str()) == p.metadata.name.as_deref())
        })
        .collect();
    let mut out = releases(&me, &held);
    out.extend(writes(workload));
    out.extend(warnings(&me, &held, sources.events));
    out.extend(terminations(&me, &pods));
    out
}

/// Newest first, then by kind, at most [`LIMIT`]; whether some were left out.
pub(crate) fn newest(mut items: Vec<TimelineEntry>) -> (Vec<TimelineEntry>, bool) {
    items.sort_by(|a, b| {
        let time = |e: &TimelineEntry| e.at.parse::<Timestamp>().ok();
        time(b)
            .cmp(&time(a))
            .then(a.kind.cmp(&b.kind))
            .then(a.workload.cmp(&b.workload))
    });
    let truncated = items.len() > LIMIT;
    items.truncate(LIMIT);
    (items, truncated)
}

pub(crate) async fn history(
    cluster: &dyn Cluster,
    project: &str,
    unreadable: &mut Vec<UnreadableView>,
) -> Result<History, ApiError> {
    let (replica_sets, revisions) = futures_util::join!(
        cluster.list::<ReplicaSet>(Some(project)),
        cluster.list::<ControllerRevision>(Some(project)),
    );
    Ok(History::new(
        allowed(replica_sets, "replicasets.apps", unreadable)?,
        allowed(revisions, "controllerrevisions.apps", unreadable)?,
    ))
}

/// What changed in a project and what went wrong, newest first: releases, each field manager's
/// last write, warnings folded by reason and containers that stopped, for every workload.
#[utoipa::path(get, path = "/api/v1/projects/{project}/timeline", tag = "projects",
    operation_id = "projectTimeline",
    params(("project" = String, Path, description = "The project's namespace")),
    responses((status = 200, body = Timeline), (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn project(
    Person(cluster): Person,
    Path(project): Path<String>,
) -> Result<Json<Timeline>, ApiError> {
    let cluster = cluster.as_ref();
    let read = crate::projects::read(cluster, &project).await?;
    let mut unreadable = read.document.unreadable.clone();
    let history = history(cluster, &project, &mut unreadable).await?;
    let sources = Sources {
        snapshot: &read.snapshot,
        events: &read.events,
        history: &history,
    };
    let items = zeabur_graph::workloads(&read.snapshot)
        .into_iter()
        .flat_map(|w| of_workload(&sources, w))
        .collect();
    let (items, truncated) = newest(items);
    Ok(Json(Timeline {
        id: format!("{}/timeline", id::project(&project)),
        items,
        truncated,
        unreadable,
    }))
}

/// What changed in one service and what went wrong, newest first, each release that runs now with
/// the plan that returns to the one before it.
#[utoipa::path(get, path = "/api/v1/projects/{project}/services/{kind}/{name}/timeline", tag = "services",
    operation_id = "serviceTimeline",
    params(("project" = String, Path),
           ("kind" = String, Path, description = "deployment, statefulset, daemonset, job or cronjob; any case"),
           ("name" = String, Path)),
    responses((status = 200, body = Timeline), (status = 404, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn service(
    Person(cluster): Person,
    Path((project, kind, name)): Path<(String, String, String)>,
) -> Result<Json<Timeline>, ApiError> {
    let Some(workload_kind) = WorkloadKind::parse(&kind) else {
        return Err(not_found(&project, &kind, &name));
    };
    let cluster = cluster.as_ref();
    let read = crate::projects::read(cluster, &project).await?;
    let workload = zeabur_graph::workloads(&read.snapshot)
        .into_iter()
        .find(|w| w.kind() == workload_kind && w.name() == name)
        .ok_or_else(|| not_found(&project, workload_kind.as_str(), &name))?;
    let mut unreadable = read.document.unreadable.clone();
    let history = history(cluster, &project, &mut unreadable).await?;
    let sources = Sources {
        snapshot: &read.snapshot,
        events: &read.events,
        history: &history,
    };
    let (items, truncated) = newest(of_workload(&sources, workload));
    Ok(Json(Timeline {
        id: format!("{}/timeline", id::service(&project, workload_kind, &name)),
        items,
        truncated,
        unreadable,
    }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn release(name: &str, revision: i64, current: bool) -> Release {
        serde_json::from_value(
            json!({"revision": revision, "name": name, "images": ["app:1"],
            "created": "2026-10-01T00:00:00Z", "current": current,
            "pods": {"ready": 0, "total": 0}, "template": {}, "templateYaml": ""}),
        )
        .unwrap()
    }

    #[test]
    fn a_pod_of_an_earlier_release_still_counts() {
        let web = WorkloadRef {
            kind: WorkloadKind::Deployment,
            name: "web".to_owned(),
        };
        let held = [release("web-7d9f", 2, true), release("web-5c4b", 1, false)];
        assert!(names_pod_of(&web, &held, "web-5c4b-x8k2p"));
        assert!(!names_pod_of(&web, &held, "web-api-5c4b-x8k2p"));
        let db = WorkloadRef {
            kind: WorkloadKind::StatefulSet,
            name: "db".to_owned(),
        };
        assert!(names_pod_of(&db, &[], "db-0"));
        assert!(!names_pod_of(&db, &[], "db-backup-0"));
    }

    #[test]
    fn only_the_running_release_reverts_to_the_one_before() {
        let web = WorkloadRef {
            kind: WorkloadKind::Deployment,
            name: "web".to_owned(),
        };
        let held = [release("web-7d9f", 2, true), release("web-5c4b", 1, false)];
        let entries = releases(&web, &held);
        assert_eq!(
            entries[0].revert,
            Some(PlanRequest {
                changes: vec![Change::Rollback {
                    workload: web,
                    revision: 1
                }],
            })
        );
        assert_eq!(entries[1].revert, None);
    }

    #[test]
    fn a_manager_is_named_by_its_tool_and_owned_fields() {
        let entry: ManagedFieldsEntry = serde_json::from_value(json!({
            "manager": "kube-controller-manager", "operation": "Update", "subresource": "scale",
            "time": "2026-10-01T00:00:00Z", "fieldsV1": {"f:spec": {"f:replicas": {}}}}))
        .unwrap();
        assert_eq!(tool(&entry), ManagerTool::Autoscaler);
        let fields = json!({"f:metadata": {"f:labels": {"f:app": {}}},
            "f:spec": {"f:replicas": {}, "f:template": {"f:spec": {}},
                "f:strategy": {"f:type": {}}}});
        assert_eq!(
            owned_fields(&fields),
            [
                "metadata.labels",
                "spec.replicas",
                "spec.strategy.type",
                "spec.template"
            ]
        );
    }
}
