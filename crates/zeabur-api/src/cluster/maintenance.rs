//! Node maintenance: stop scheduling (cordon), resume scheduling (uncordon), and maintenance.
//!
//! Maintenance cordons the node and evicts what can move, as `kubectl drain --ignore-daemonsets
//! --delete-emptydir-data` does [kubectl-drain].
//!
//! # The plan
//!
//! A plan reads the node and the pods bound to it and says what maintenance does to each pod:
//!
//! | Fate | Pods | Evicted |
//! |---|---|---|
//! | `moves` | pods with a controller | yes; the controller starts a replacement elsewhere |
//! | `stays` | a DaemonSet's pods, static pods' mirrors | no, as `--ignore-daemonsets` |
//! | `deleted` | pods without a controller | only when the person confirms, as `--force` |
//! | `stuck` | pods with a claim bound to a volume kept on this node | only when the person opts in |
//!
//! Finished pods (`Succeeded`, `Failed`) hold nothing and are left out. A moving pod also says
//! which scratch space (`emptyDir`) it loses and which `PodDisruptionBudget` covers it. A volume is
//! kept on this node when its PersistentVolume's node affinity requires this node's hostname and
//! nothing else, as local-path and `local` volumes do. The plan checks whether the other ready,
//! schedulable nodes have room for what the moving pods request (requests only: taints and
//! affinity are not weighed), and says when Zeabur itself runs on the node, when no other node
//! takes new pods, and when the node runs the control plane.
//!
//! # Apply and the passes
//!
//! The token binds the person, the node's `uid`, the action and the pods the plan may evict, by
//! `uid`. Apply patches `spec.unschedulable` (a dry run first) and, for maintenance, makes the
//! first eviction pass. A pass evicts each of those pods still on the node through the Eviction
//! API, which respects `PodDisruptionBudget`s: a budget that allows no disruption now is answered
//! `429` and the pod is reported waiting on it. Zeabur's own pods are evicted only once no other
//! pod is leaving or waiting, since moving Zeabur ends the person's session (sessions live in its
//! memory) and with it the passes. The browser repeats passes while it stays open
//! (`evict`, with the same token, for an hour); nothing runs on the server between passes. A
//! person who leaves leaves the node cordoned, and a new plan shows what is still on it.
//!
//! Everything is read and written as the person: RBAC decides whether they may patch the node
//! (`patch nodes`) and evict pods (`create pods/eviction`).
//!
//! [kubectl-drain]: https://kubernetes.io/docs/reference/kubectl/generated/kubectl_drain/

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use zeabur_core::names::LABEL_PROJECT;
use zeabur_core::problem::{Denied, Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::api::core::v1::{Namespace, Node, PersistentVolumeClaim, Pod};
use zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{
    Cluster, ClusterError, ClusterExt as _, ObjectTarget, Preconditions, ResourceRef, WriteBody,
    WriteRequest,
};
use zeabur_model::ProjectDetails;

use crate::cluster::WIDE_LIMIT;
use crate::cluster::capacity::{Amount, spec_amounts};
use crate::error::{ApiError, get_read, read_problem};
use crate::handlers::{AppState, Everyday, Person};
use crate::plan_token::{self as token, LIFETIME_SECONDS, TokenError};
use crate::projects::writes::preview::node_room;
use crate::projects::writes::token_problem;
use crate::views::{Resources, UnreadableView};

/// Sealed plans of this module start with this, so no other plan's token opens as one.
const PREFIX: &str = "d1.";
/// How long after its plan expires a maintenance token still makes eviction passes.
const PASSES_SECONDS: i64 = 3_600;
const HOSTNAME: &str = "kubernetes.io/hostname";
const MIRROR: &str = "kubernetes.io/config.mirror";
const TEMPLATE_HASH: &str = "pod-template-hash";
/// The chart's `app.kubernetes.io/name` for Zeabur's own pods.
const ZEABUR_APP: &str = "zeabur";

/// What a plan changes on a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum NodeAction {
    /// Stop scheduling new pods on it (`spec.unschedulable: true`).
    Cordon,
    /// Schedule new pods on it again.
    Uncordon,
    /// Cordon it, then evict the pods that can move.
    Drain,
}

/// The action to plan.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct MaintenanceRequest {
    /// What to do.
    pub action: NodeAction,
}

/// What maintenance does to a pod.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum PodFate {
    /// Evicted; its controller starts a replacement on another node.
    Moves,
    /// Left on the node: a DaemonSet's pod or a static pod.
    Stays,
    /// Has no controller: evicting deletes it and nothing replaces it.
    Deleted,
    /// Uses a volume kept on this node: a replacement elsewhere would wait for it.
    Stuck,
}

/// Why a pod stays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum StayReason {
    /// A DaemonSet runs one on every node.
    DaemonSet,
    /// The kubelet runs it from a file on the node (a mirror pod).
    Static,
}

/// The workload a pod belongs to, as the interface links it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct PodWorkload {
    /// `Deployment`, `StatefulSet`, `DaemonSet`, `Job`, or the controller's own kind.
    pub kind: String,
    /// Its name.
    pub name: String,
}

/// A pod on the node, and what maintenance does to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DrainPod {
    /// Its namespace.
    pub namespace: String,
    /// Its name.
    pub name: String,
    /// What happens to it.
    pub fate: PodFate,
    /// Why it stays, when it does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stays: Option<StayReason>,
    /// The project it is in, when its namespace is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// The project's display name, when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    /// Its workload, when its controller says which.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workload: Option<PodWorkload>,
    /// Its `emptyDir` volumes, emptied when it is evicted.
    pub scratch: Vec<String>,
    /// Its claims bound to a volume kept on this node.
    pub local_claims: Vec<String>,
    /// The `PodDisruptionBudget` that covers it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget: Option<String>,
    /// That budget allows fewer disruptions now than this node has pods under it, so some of
    /// their evictions wait.
    pub budget_blocks: bool,
    /// One of Zeabur's own pods.
    pub zeabur: bool,
    /// Already being deleted.
    pub terminating: bool,
    /// What it requests.
    pub requests: Resources,
}

/// Whether the other nodes have room for the pods that move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum CapacityStatus {
    /// Every moving pod fits on another ready, schedulable node.
    Fits,
    /// Some would wait (Pending) for room.
    Short,
    /// The other nodes' pods could not all be read.
    Unknown,
}

/// The moving pods against the room on the other nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DrainCapacity {
    /// The verdict.
    pub status: CapacityStatus,
    /// What the moving pods request, with their count.
    pub needs: Resources,
    /// What the other ready, schedulable nodes have left, with how many pods they take; absent
    /// when unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room: Option<Resources>,
    /// The moving pods that fit on no node, as `namespace/name`.
    pub unplaced: Vec<String>,
}

/// What a plan changes, and the token that applies it.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MaintenancePlan {
    /// Send this to `apply` to carry out exactly this plan.
    pub token: String,
    /// When the token stops working, RFC 3339.
    pub expires_at: String,
    /// The node.
    pub node: String,
    /// The action.
    pub action: NodeAction,
    /// Whether the node is cordoned now.
    pub cordoned: bool,
    /// Its roles (`control-plane`, `etcd`).
    pub roles: Vec<String>,
    /// How many other nodes are ready and take new pods.
    pub other_schedulable: usize,
    /// Zeabur itself runs on this node.
    pub zeabur_here: bool,
    /// The pods on the node, those that move first, then by namespace and name.
    pub pods: Vec<DrainPod>,
    /// Whether the moving pods fit elsewhere; maintenance only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity: Option<DrainCapacity>,
    /// Reads that were refused and what they leave unchecked (`persistentvolumes`,
    /// `poddisruptionbudgets.policy`, `pods`).
    pub unreadable: Vec<UnreadableView>,
}

/// The token of a plan, and what the person chose to evict besides the pods that move.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceApply {
    /// The token `plan` returned.
    pub token: String,
    /// Also evict the pods whose volume is kept on this node.
    #[serde(default)]
    pub include_stuck: bool,
    /// Also evict (delete) the pods without a controller.
    #[serde(default)]
    pub include_deleted: bool,
}

/// What a pass found or did for one pod.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum EvictOutcome {
    /// No longer on the node.
    Gone,
    /// Evicted; its containers are stopping.
    Leaving,
    /// A `PodDisruptionBudget` does not allow it now (`429`).
    Waiting,
    /// The person may not evict it (`403`).
    Refused,
    /// The eviction failed otherwise.
    Failed,
    /// One of Zeabur's own pods, left until the others are off the node: moving it ends the
    /// person's session, and with it the passes.
    Last,
}

/// One pod of a pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EvictedPod {
    /// Its namespace.
    pub namespace: String,
    /// Its name.
    pub name: String,
    /// What happened.
    pub outcome: EvictOutcome,
    /// One of Zeabur's own pods: once it is leaving, the passes after it fail until Zeabur is up
    /// again on another node.
    pub zeabur: bool,
    /// The budget it waits on, when one was found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget: Option<String>,
    /// How long kube-apiserver asked to wait before trying again.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_seconds: Option<u32>,
    /// kube-apiserver's message, for a refusal or a failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// The node after an apply or a pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DrainProgress {
    /// Whether the node is cordoned.
    pub cordoned: bool,
    /// The pods being evicted, in the plan's order; empty for stop and resume scheduling.
    pub pods: Vec<EvictedPod>,
    /// How many of them are still on the node.
    pub remaining: usize,
    /// None of them is left.
    pub done: bool,
}

/// A pod the token lets apply evict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Planned {
    namespace: String,
    name: String,
    uid: String,
    fate: PodFate,
    /// One of Zeabur's own pods, evicted last.
    #[serde(default)]
    zeabur: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Claims {
    person: String,
    groups: Vec<String>,
    node: String,
    node_uid: String,
    action: NodeAction,
    pods: Vec<Planned>,
    expires_at: i64,
}

fn text<'a>(object: &'a Value, path: &str) -> &'a str {
    object.pointer(path).and_then(Value::as_str).unwrap_or("")
}

fn is_cordoned(node: &Node) -> bool {
    node.spec
        .as_ref()
        .and_then(|s| s.unschedulable)
        .unwrap_or(false)
}

fn is_ready(node: &Node) -> bool {
    node.status
        .as_ref()
        .and_then(|s| s.conditions.as_ref())
        .is_some_and(|c| c.iter().any(|c| c.type_ == "Ready" && c.status == "True"))
}

fn is_finished(pod: &Pod) -> bool {
    matches!(
        pod.status.as_ref().and_then(|s| s.phase.as_deref()),
        Some("Succeeded" | "Failed")
    )
}

/// The node's hostname, as volumes' node affinity names it.
fn hostname(node: &Node) -> String {
    node.metadata
        .labels
        .as_ref()
        .and_then(|l| l.get(HOSTNAME))
        .or(node.metadata.name.as_ref())
        .cloned()
        .unwrap_or_default()
}

/// Whether a PersistentVolume can only be used on the node called `host`: every term of its
/// required node affinity asks for that hostname and no other.
fn kept_on(volume: &Value, host: &str) -> bool {
    let terms = volume
        .pointer("/spec/nodeAffinity/required/nodeSelectorTerms")
        .and_then(Value::as_array);
    let Some(terms) = terms.filter(|t| !t.is_empty()) else {
        return false;
    };
    terms.iter().all(|term| {
        term.get("matchExpressions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|e| {
                let values: Vec<&str> = e
                    .get("values")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                text(e, "/key") == HOSTNAME && text(e, "/operator") == "In" && values == [host]
            })
    })
}

/// The workload a pod's controller stands for: a ReplicaSet's Deployment (its name less the
/// `pod-template-hash`), or the controller itself.
fn workload_of(pod: &Pod) -> Option<PodWorkload> {
    let owner = pod
        .metadata
        .owner_references
        .as_ref()?
        .iter()
        .find(|o| o.controller == Some(true))?;
    let hash = pod
        .metadata
        .labels
        .as_ref()
        .and_then(|l| l.get(TEMPLATE_HASH));
    if owner.kind == "ReplicaSet"
        && let Some(deployment) = hash.and_then(|h| owner.name.strip_suffix(&format!("-{h}")))
    {
        return Some(PodWorkload {
            kind: "Deployment".to_owned(),
            name: deployment.to_owned(),
        });
    }
    Some(PodWorkload {
        kind: owner.kind.clone(),
        name: owner.name.clone(),
    })
}

fn labels_of(pod: &Pod) -> BTreeMap<String, String> {
    pod.metadata.labels.clone().unwrap_or_default()
}

/// What the plan reads besides the node and its pods.
#[derive(Debug, Default)]
struct Context {
    /// Claims by `namespace/name`: the volume each is bound to.
    claims: BTreeMap<String, String>,
    /// PersistentVolumes by name.
    volumes: BTreeMap<String, Value>,
    /// `PodDisruptionBudget`s.
    budgets: Vec<Value>,
    /// Namespaces, for which are projects.
    namespaces: Vec<Namespace>,
    /// The namespace Zeabur runs in, when known.
    zeabur_namespace: Option<String>,
}

fn budget_for<'a>(budgets: &'a [Value], pod: &Pod) -> Option<&'a Value> {
    let namespace = pod.metadata.namespace.as_deref().unwrap_or_default();
    let labels = labels_of(pod);
    budgets.iter().find(|budget| {
        text(budget, "/metadata/namespace") == namespace
            && budget
                .pointer("/spec/selector")
                .and_then(|s| serde_json::from_value::<LabelSelector>(s.clone()).ok())
                .is_some_and(|s| zeabur_adapters::selector::matches(&s, &labels))
    })
}

/// Marks the pods whose budget allows fewer disruptions now than this node has pods under it to
/// evict: kubectl drain waits on the rest.
fn held_by_budgets(out: &mut [DrainPod], budgets: &[Value]) {
    let under = |namespace: &str, budget: &str| {
        out.iter()
            .filter(|p| p.fate != PodFate::Stays && p.namespace == namespace)
            .filter(|p| p.budget.as_deref() == Some(budget))
            .count()
    };
    let blocked: Vec<bool> = out
        .iter()
        .map(|pod| {
            let Some(name) = pod.budget.as_deref() else {
                return false;
            };
            let allowed = budgets
                .iter()
                .find(|b| {
                    text(b, "/metadata/namespace") == pod.namespace
                        && text(b, "/metadata/name") == name
                })
                .and_then(|b| b.pointer("/status/disruptionsAllowed"))
                .and_then(Value::as_i64);
            allowed
                .is_some_and(|n| i64::try_from(under(&pod.namespace, name)).unwrap_or(i64::MAX) > n)
        })
        .collect();
    for (pod, blocks) in out.iter_mut().zip(blocked) {
        pod.budget_blocks = blocks;
    }
}

/// What maintenance does to each pod on `node`, those that move first.
fn classify(node: &Node, pods: &[Pod], context: &Context) -> Vec<DrainPod> {
    let host = hostname(node);
    let mut out: Vec<DrainPod> = pods
        .iter()
        .filter(|pod| !is_finished(pod))
        .map(|pod| {
            let namespace = pod.metadata.namespace.clone().unwrap_or_default();
            let mirror = pod
                .metadata
                .annotations
                .as_ref()
                .is_some_and(|a| a.contains_key(MIRROR));
            // A mirror pod's owner is the Node; the pod's own name says more.
            let workload = (!mirror).then(|| workload_of(pod)).flatten();
            let volumes = pod.spec.as_ref().and_then(|s| s.volumes.as_ref());
            let scratch: Vec<String> = volumes
                .into_iter()
                .flatten()
                .filter(|v| v.empty_dir.is_some())
                .map(|v| v.name.clone())
                .collect();
            let local_claims: Vec<String> = volumes
                .into_iter()
                .flatten()
                .filter_map(|v| v.persistent_volume_claim.as_ref())
                .map(|c| c.claim_name.clone())
                .filter(|claim| {
                    context
                        .claims
                        .get(&format!("{namespace}/{claim}"))
                        .and_then(|volume| context.volumes.get(volume))
                        .is_some_and(|volume| kept_on(volume, &host))
                })
                .collect();
            let (fate, stays) = if mirror {
                (PodFate::Stays, Some(StayReason::Static))
            } else if workload.as_ref().is_some_and(|w| w.kind == "DaemonSet") {
                (PodFate::Stays, Some(StayReason::DaemonSet))
            } else if !local_claims.is_empty() {
                (PodFate::Stuck, None)
            } else if workload.is_none() {
                (PodFate::Deleted, None)
            } else {
                (PodFate::Moves, None)
            };
            let budget = budget_for(&context.budgets, pod);
            let project = context
                .namespaces
                .iter()
                .find(|n| n.metadata.name.as_deref() == Some(namespace.as_str()))
                .filter(|n| {
                    n.metadata
                        .labels
                        .as_ref()
                        .is_some_and(|l| l.contains_key(LABEL_PROJECT))
                });
            let requests = pod.spec.as_ref().map_or_default(spec_amounts).0;
            DrainPod {
                name: pod.metadata.name.clone().unwrap_or_default(),
                fate,
                stays,
                project: project.map(|_| namespace.clone()),
                project_name: project.and_then(|n| {
                    ProjectDetails::from_annotations(n.metadata.annotations.as_ref()).display_name
                }),
                workload,
                scratch,
                local_claims,
                budget: budget.map(|b| text(b, "/metadata/name").to_owned()),
                budget_blocks: false,
                zeabur: context.zeabur_namespace.as_deref() == Some(namespace.as_str())
                    && labels_of(pod)
                        .get("app.kubernetes.io/name")
                        .map(String::as_str)
                        == Some(ZEABUR_APP),
                terminating: pod.metadata.deletion_timestamp.is_some(),
                requests: Resources {
                    cpu_millicores: requests.cpu,
                    memory_bytes: requests.memory,
                    pods: None,
                },
                namespace,
            }
        })
        .collect();
    held_by_budgets(&mut out, &context.budgets);
    let rank = |fate: PodFate| match fate {
        PodFate::Moves => 0,
        PodFate::Stuck => 1,
        PodFate::Deleted => 2,
        PodFate::Stays => 3,
    };
    out.sort_by(|a, b| {
        rank(a.fate)
            .cmp(&rank(b.fate))
            .then_with(|| a.namespace.cmp(&b.namespace))
            .then_with(|| a.name.cmp(&b.name))
    });
    out
}

/// Places the moving pods on `room` (each node's CPU, memory and pods left), largest memory
/// first, each on the node with the least memory left that still takes it.
///
/// Requests only: taints, tolerations, node selectors, affinity, topology spread and host ports
/// are not weighed, so a pod counted as placed can still stay Pending, and a budget can slow the
/// moves but not change where they land.
fn place(moving: &[&DrainPod], room: Option<Vec<(Amount, u64)>>) -> DrainCapacity {
    let mut needs = Resources {
        pods: Some(0),
        ..Resources::default()
    };
    for pod in moving {
        needs.cpu_millicores = needs
            .cpu_millicores
            .saturating_add(pod.requests.cpu_millicores);
        needs.memory_bytes = needs.memory_bytes.saturating_add(pod.requests.memory_bytes);
        needs.pods = needs.pods.map(|n| n.saturating_add(1));
    }
    let Some(mut room) = room else {
        return DrainCapacity {
            status: CapacityStatus::Unknown,
            needs,
            room: None,
            unplaced: Vec::new(),
        };
    };
    let total = room.iter().fold(
        Resources {
            pods: Some(0),
            ..Resources::default()
        },
        |sum, (amount, pods)| Resources {
            cpu_millicores: sum.cpu_millicores.saturating_add(amount.cpu),
            memory_bytes: sum.memory_bytes.saturating_add(amount.memory),
            pods: sum.pods.map(|n| n.saturating_add(*pods)),
        },
    );
    let mut order: Vec<&&DrainPod> = moving.iter().collect();
    order.sort_by(|a, b| {
        b.requests
            .memory_bytes
            .cmp(&a.requests.memory_bytes)
            .then_with(|| b.requests.cpu_millicores.cmp(&a.requests.cpu_millicores))
    });
    let mut unplaced = Vec::new();
    for pod in order {
        let fits = room
            .iter_mut()
            .filter(|(amount, pods)| {
                *pods > 0
                    && amount.cpu >= pod.requests.cpu_millicores
                    && amount.memory >= pod.requests.memory_bytes
            })
            .min_by_key(|(amount, _)| amount.memory);
        match fits {
            Some((amount, pods)) => {
                amount.cpu -= pod.requests.cpu_millicores;
                amount.memory -= pod.requests.memory_bytes;
                *pods -= 1;
            }
            None => unplaced.push(format!("{}/{}", pod.namespace, pod.name)),
        }
    }
    unplaced.sort();
    DrainCapacity {
        status: if unplaced.is_empty() {
            CapacityStatus::Fits
        } else {
            CapacityStatus::Short
        },
        needs,
        room: Some(total),
        unplaced,
    }
}

fn readable<T>(
    result: Result<T, ClusterError>,
    resource: &str,
    unreadable: &mut Vec<UnreadableView>,
) -> Result<Option<T>, ApiError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) => {
            let refusal = error
                .into_unreadable(resource)
                .map_err(|e| ApiError::from(e.into_problem(resource)))?;
            unreadable.push(refusal.into());
            Ok(None)
        }
    }
}

fn values(objects: Option<Vec<zeabur_kube::DynamicObject>>) -> Vec<Value> {
    objects
        .unwrap_or_default()
        .iter()
        .filter_map(|o| serde_json::to_value(o).ok())
        .collect()
}

async fn read_node(cluster: &dyn Cluster, name: &str) -> Result<Node, ApiError> {
    let resource = ResourceRef::new("", "v1", "nodes", "Node");
    let object = cluster
        .find_object(&resource, None, name)
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("the node {name}"),
                get_read("", "nodes", None, name),
            )
        })?
        .ok_or_else(|| {
            ApiError::from(Problem::new(
                Kind::NotFound,
                format!("There is no node {name}."),
            ))
        })?;
    serde_json::to_value(object)
        .and_then(serde_json::from_value)
        .map_err(|e| ApiError::from(ClusterError::Decode(e.to_string()).into_problem("a node")))
}

async fn read_pods(cluster: &dyn Cluster, node: &str) -> Result<Vec<Pod>, ApiError> {
    cluster.node_pods(node).await.map_err(|e| {
        read_problem(
            e,
            &format!("the pods on {node}"),
            crate::error::list_read("", "pods", None),
        )
    })
}

/// The claims, volumes, budgets and namespaces the pods need, as far as the person may read
/// them.
async fn context(
    cluster: &dyn Cluster,
    pods: &[Pod],
    zeabur_namespace: Option<&str>,
    unreadable: &mut Vec<UnreadableView>,
) -> Result<Context, ApiError> {
    let has_claims = pods.iter().any(|p| {
        p.spec
            .as_ref()
            .and_then(|s| s.volumes.as_ref())
            .is_some_and(|v| v.iter().any(|v| v.persistent_volume_claim.is_some()))
    });
    let budgets_ref = ResourceRef::new(
        "policy",
        "v1",
        "poddisruptionbudgets",
        "PodDisruptionBudget",
    );
    let volumes_ref = ResourceRef::new("", "v1", "persistentvolumes", "PersistentVolume");
    let (namespaces, budgets) = futures_util::join!(
        cluster.namespaces(),
        cluster.list_objects(&budgets_ref, None)
    );
    let mut context = Context {
        namespaces: readable(namespaces, "namespaces", unreadable)?.unwrap_or_default(),
        budgets: values(readable(budgets, "poddisruptionbudgets.policy", unreadable)?.flatten()),
        zeabur_namespace: zeabur_namespace.map(str::to_owned),
        ..Context::default()
    };
    if has_claims {
        let (claims, volumes) = futures_util::join!(
            cluster.list::<PersistentVolumeClaim>(None),
            cluster.list_objects(&volumes_ref, None)
        );
        context.claims = readable(claims, "persistentvolumeclaims", unreadable)?
            .unwrap_or_default()
            .into_iter()
            .filter_map(|c| {
                let key = format!(
                    "{}/{}",
                    c.metadata.namespace.as_deref()?,
                    c.metadata.name.as_deref()?
                );
                Some((key, c.spec?.volume_name?))
            })
            .collect();
        context.volumes = values(readable(volumes, "persistentvolumes", unreadable)?.flatten())
            .into_iter()
            .map(|v| (text(&v, "/metadata/name").to_owned(), v))
            .collect();
    }
    Ok(context)
}

fn plan_key(state: &AppState) -> Result<std::sync::Arc<token::PlanKey>, ApiError> {
    state.plan_key.clone().ok_or_else(|| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "Zeabur could not make a token key when it started, so it cannot plan changes.",
        ))
    })
}

/// Works out what stopping scheduling, resuming it or maintenance does to the node and its pods,
/// and a token to apply it; writes nothing.
#[utoipa::path(post, path = "/api/v1/cluster/nodes/{node}/maintenance/plan", tag = "cluster",
    operation_id = "planNodeMaintenance", request_body = MaintenanceRequest,
    params(("node" = String, Path, description = "The node's name")),
    responses((status = 200, body = MaintenancePlan),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn plan(
    State(state): State<AppState>,
    Person(cluster): Person,
    Everyday(everyday): Everyday,
    Path(name): Path<String>,
    Json(request): Json<MaintenanceRequest>,
) -> Result<Json<MaintenancePlan>, ApiError> {
    let sealing = plan_key(&state)?;
    let identity = everyday
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let node = read_node(cluster.as_ref(), &name).await?;
    let pods = read_pods(cluster.as_ref(), &name).await?;
    let mut unreadable = Vec::new();
    let context = context(
        cluster.as_ref(),
        &pods,
        state.settings.namespace.as_deref(),
        &mut unreadable,
    )
    .await?;
    let classified = classify(&node, &pods, &context);
    let (nodes, cluster_pods) =
        futures_util::join!(cluster.list::<Node>(None), cluster.cluster_pods(WIDE_LIMIT));
    let others: Vec<Node> = readable(nodes, "nodes", &mut unreadable)?
        .unwrap_or_default()
        .into_iter()
        .filter(|n| n.metadata.name.as_deref() != Some(name.as_str()))
        .collect();
    let other_schedulable = others
        .iter()
        .filter(|n| is_ready(n) && !is_cordoned(n))
        .count();
    let capacity = (request.action == NodeAction::Drain).then(|| {
        let moving: Vec<&DrainPod> = classified
            .iter()
            .filter(|p| p.fate == PodFate::Moves && !p.terminating)
            .collect();
        let room = match cluster_pods {
            Ok(read) if !read.truncated => Some(node_room(&others, &read.pods)),
            Ok(_) | Err(_) => None,
        };
        place(&moving, room)
    });
    let now = (state.clock)().as_second();
    let expires_at = now.saturating_add(LIFETIME_SECONDS);
    let evictable = if request.action == NodeAction::Drain {
        classified
            .iter()
            .filter(|p| p.fate != PodFate::Stays)
            .map(|p| {
                let uid = pods
                    .iter()
                    .find(|pod| {
                        pod.metadata.namespace.as_deref() == Some(p.namespace.as_str())
                            && pod.metadata.name.as_deref() == Some(p.name.as_str())
                    })
                    .and_then(|pod| pod.metadata.uid.clone())
                    .unwrap_or_default();
                Planned {
                    namespace: p.namespace.clone(),
                    name: p.name.clone(),
                    uid,
                    fate: p.fate,
                    zeabur: p.zeabur,
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    let claims = Claims {
        person: identity.username,
        groups: identity.groups,
        node: name.clone(),
        node_uid: node.metadata.uid.clone().unwrap_or_default(),
        action: request.action,
        pods: evictable,
        expires_at,
    };
    let token = token::sign_as(&sealing, PREFIX, &claims).map_err(|_random| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "The system random number generator failed, so Zeabur could not seal the plan.",
        ))
    })?;
    Ok(Json(MaintenancePlan {
        token,
        expires_at: Timestamp::from_second(expires_at)
            .unwrap_or(Timestamp::UNIX_EPOCH)
            .to_string(),
        cordoned: is_cordoned(&node),
        roles: crate::cluster::roles(node.metadata.labels.as_ref()),
        node: name,
        action: request.action,
        other_schedulable,
        zeabur_here: classified.iter().any(|p| p.zeabur),
        pods: classified,
        capacity,
        unreadable,
    }))
}

fn node_target(name: &str) -> ObjectTarget {
    ObjectTarget {
        api_version: "v1".to_owned(),
        kind: "Node".to_owned(),
        plural: "nodes".to_owned(),
        namespace: String::new(),
        name: name.to_owned(),
    }
}

/// The JSON Patch that sets the node's `spec.unschedulable` to `cordon`, refused when the node
/// was replaced; `None` when it already is.
fn unschedulable_patch(node: &Node, cordon: bool) -> Option<Value> {
    if is_cordoned(node) == cordon {
        return None;
    }
    let uid = node.metadata.uid.clone().unwrap_or_default();
    let change = if cordon {
        json!({"op": "add", "path": "/spec/unschedulable", "value": true})
    } else {
        json!({"op": "remove", "path": "/spec/unschedulable"})
    };
    Some(json!([{"op": "test", "path": "/metadata/uid", "value": uid}, change]))
}

async fn patch_node(cluster: &dyn Cluster, name: &str, patch: Value) -> Result<(), ApiError> {
    for dry_run in [true, false] {
        let write = WriteRequest {
            target: node_target(name),
            body: WriteBody::JsonPatch(patch.clone()),
            dry_run,
        };
        cluster.write(&write).await.map_err(|error| {
            read_problem(
                error,
                &format!("the node {name}"),
                Denied {
                    verb: "patch".to_owned(),
                    group: String::new(),
                    resource: "nodes".to_owned(),
                    name: Some(name.to_owned()),
                    namespace: None,
                },
            )
        })?;
    }
    Ok(())
}

/// Opens a token, checks that the same person sends it and that `deadline` (from its claims) has
/// not passed, and reads its node, refusing one that was replaced.
async fn opened(
    state: &AppState,
    cluster: &dyn Cluster,
    everyday: &dyn Cluster,
    name: &str,
    token: &str,
    deadline: fn(&Claims) -> i64,
) -> Result<(Claims, Node), ApiError> {
    let sealing = plan_key(state)?;
    let claims: Claims = token::open_as(&sealing, PREFIX, token).map_err(token_problem)?;
    if (state.clock)().as_second() >= deadline(&claims) {
        return Err(token_problem(TokenError::Expired));
    }
    let identity = everyday
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    if (identity.username.as_str(), identity.groups.as_slice())
        != (claims.person.as_str(), claims.groups.as_slice())
    {
        return Err(ApiError::from(Problem::new(
            Kind::Forbidden,
            "Someone else made this plan, or you made it with other group memberships.",
        )));
    }
    if claims.node != name {
        return Err(ApiError::from(Problem::new(
            Kind::BadRequest,
            format!("This plan is for the node {}, not {name}.", claims.node),
        )));
    }
    let node = read_node(cluster, name).await?;
    if node.metadata.uid.as_deref().unwrap_or_default() != claims.node_uid {
        return Err(ApiError::from(Problem::new(
            Kind::Conflict,
            format!("The node {name} was replaced after the plan was made."),
        )));
    }
    Ok((claims, node))
}

/// The pods of `claims` a pass evicts.
fn chosen(claims: &Claims, request: &MaintenanceApply) -> Vec<Planned> {
    claims
        .pods
        .iter()
        .filter(|p| match p.fate {
            PodFate::Moves => true,
            PodFate::Stuck => request.include_stuck,
            PodFate::Deleted => request.include_deleted,
            PodFate::Stays => false,
        })
        .cloned()
        .collect()
}

/// Carries out a plan by its token: patches `spec.unschedulable` and, for maintenance, makes the
/// first eviction pass.
#[utoipa::path(post, path = "/api/v1/cluster/nodes/{node}/maintenance/apply", tag = "cluster",
    operation_id = "applyNodeMaintenance", request_body = MaintenanceApply,
    params(("node" = String, Path, description = "The node's name")),
    responses((status = 200, body = DrainProgress),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn apply(
    State(state): State<AppState>,
    Person(cluster): Person,
    Everyday(everyday): Everyday,
    Path(name): Path<String>,
    Json(request): Json<MaintenanceApply>,
) -> Result<Json<DrainProgress>, ApiError> {
    let (claims, node) = opened(
        &state,
        cluster.as_ref(),
        everyday.as_ref(),
        &name,
        &request.token,
        |c| c.expires_at,
    )
    .await?;
    let cordon = claims.action != NodeAction::Uncordon;
    if let Some(patch) = unschedulable_patch(&node, cordon) {
        patch_node(cluster.as_ref(), &name, patch).await?;
    }
    if claims.action != NodeAction::Drain {
        return Ok(Json(DrainProgress {
            cordoned: cordon,
            pods: Vec::new(),
            remaining: 0,
            done: true,
        }));
    }
    let pods = chosen(&claims, &request);
    Ok(Json(pass(cluster.as_ref(), &name, &pods).await?))
}

/// Evicts the pods of a maintenance plan that are still on the node, once; the browser calls it
/// again while it waits. Accepted for an hour after the plan expires, while the node stays
/// cordoned.
#[utoipa::path(post, path = "/api/v1/cluster/nodes/{node}/maintenance/evict", tag = "cluster",
    operation_id = "evictNodePods", request_body = MaintenanceApply,
    params(("node" = String, Path, description = "The node's name")),
    responses((status = 200, body = DrainProgress),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn evict(
    State(state): State<AppState>,
    Person(cluster): Person,
    Everyday(everyday): Everyday,
    Path(name): Path<String>,
    Json(request): Json<MaintenanceApply>,
) -> Result<Json<DrainProgress>, ApiError> {
    let (claims, node) = opened(
        &state,
        cluster.as_ref(),
        everyday.as_ref(),
        &name,
        &request.token,
        |c| c.expires_at.saturating_add(PASSES_SECONDS),
    )
    .await?;
    if claims.action != NodeAction::Drain {
        return Err(ApiError::from(Problem::new(
            Kind::BadRequest,
            "This plan does not evict pods.",
        )));
    }
    if !is_cordoned(&node) {
        return Err(ApiError::from(Problem::new(
            Kind::Conflict,
            format!(
                "The node {name} takes new pods again, so the moves stopped. Review maintenance again."
            ),
        )));
    }
    let pods = chosen(&claims, &request);
    Ok(Json(pass(cluster.as_ref(), &name, &pods).await?))
}

fn pod_target(pod: &Planned) -> ObjectTarget {
    ObjectTarget {
        api_version: "v1".to_owned(),
        kind: "Pod".to_owned(),
        plural: "pods".to_owned(),
        namespace: pod.namespace.clone(),
        name: pod.name.clone(),
    }
}

/// The budget in `namespace` that covers the pod with `labels`, when the person may read them.
async fn waiting_on(
    cluster: &dyn Cluster,
    namespace: &str,
    labels: &BTreeMap<String, String>,
) -> Option<String> {
    let resource = ResourceRef::new(
        "policy",
        "v1",
        "poddisruptionbudgets",
        "PodDisruptionBudget",
    );
    let budgets = values(
        cluster
            .list_objects(&resource, Some(namespace))
            .await
            .ok()?,
    );
    let pod = Pod {
        metadata: zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
            namespace: Some(namespace.to_owned()),
            labels: Some(labels.clone()),
            ..Default::default()
        },
        ..Pod::default()
    };
    budget_for(&budgets, &pod).map(|b| text(b, "/metadata/name").to_owned())
}

/// One eviction pass: each of `planned` still on `node` is evicted unless it is already leaving.
/// A pod replaced by another of the same name (another `uid`) counts as gone.
async fn pass(
    cluster: &dyn Cluster,
    node: &str,
    planned: &[Planned],
) -> Result<DrainProgress, ApiError> {
    let on_node = read_pods(cluster, node).await?;
    let cordoned = read_node(cluster, node)
        .await
        .is_ok_and(|n| is_cordoned(&n));
    let found: BTreeMap<&str, &Pod> = on_node
        .iter()
        .filter(|p| !is_finished(p))
        .filter_map(|p| Some((p.metadata.uid.as_deref()?, p)))
        .collect();
    let mut pods = Vec::new();
    let mut order: Vec<(usize, &Planned)> = planned.iter().enumerate().collect();
    order.sort_by_key(|(_, p)| p.zeabur);
    for (index, target) in order {
        let in_flight = pods.iter().any(|(_, p): &(usize, EvictedPod)| {
            matches!(p.outcome, EvictOutcome::Leaving | EvictOutcome::Waiting)
        });
        let mut evicted = EvictedPod {
            namespace: target.namespace.clone(),
            name: target.name.clone(),
            outcome: EvictOutcome::Gone,
            zeabur: target.zeabur,
            budget: None,
            retry_after_seconds: None,
            detail: None,
        };
        let Some(pod) = found.get(target.uid.as_str()) else {
            pods.push((index, evicted));
            continue;
        };
        if pod.metadata.deletion_timestamp.is_some() {
            evicted.outcome = EvictOutcome::Leaving;
            pods.push((index, evicted));
            continue;
        }
        if target.zeabur && in_flight {
            evicted.outcome = EvictOutcome::Last;
            pods.push((index, evicted));
            continue;
        }
        let write = WriteRequest {
            target: pod_target(target),
            body: WriteBody::Evict(Preconditions {
                uid: Some(target.uid.clone()),
                resource_version: None,
                grace_period_seconds: None,
            }),
            dry_run: false,
        };
        evicted.outcome = match cluster.write(&write).await {
            Ok(_) => EvictOutcome::Leaving,
            Err(ClusterError::Status(status)) => match status.code {
                404 | 409 => EvictOutcome::Gone,
                429 => {
                    evicted.retry_after_seconds = status.retry_after_seconds;
                    evicted.budget = waiting_on(cluster, &target.namespace, &labels_of(pod)).await;
                    EvictOutcome::Waiting
                }
                401 | 403 => {
                    evicted.detail = Some(status.message);
                    EvictOutcome::Refused
                }
                _ => {
                    evicted.detail = Some(status.message);
                    EvictOutcome::Failed
                }
            },
            Err(error) => {
                evicted.detail = Some(error.to_string());
                EvictOutcome::Failed
            }
        };
        pods.push((index, evicted));
    }
    pods.sort_by_key(|(index, _)| *index);
    let pods: Vec<EvictedPod> = pods.into_iter().map(|(_, pod)| pod).collect();
    let remaining = pods
        .iter()
        .filter(|p| p.outcome != EvictOutcome::Gone)
        .count();
    Ok(DrainProgress {
        cordoned,
        pods,
        remaining,
        done: remaining == 0,
    })
}

/// A node's pods as maintenance sees them.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NodePods {
    /// Would move, including those leaving.
    pub(crate) moving: u64,
    /// Being deleted.
    pub(crate) leaving: u64,
    /// Kept by a volume only this node can use; maintenance leaves them unless asked.
    pub(crate) pinned: u64,
}

/// Each node's pods as maintenance sees them, by node name. PersistentVolumes are listed once,
/// and only when a pod has a claim.
pub(crate) async fn node_pods(
    cluster: &dyn Cluster,
    nodes: &[Node],
    pods: &[zeabur_kube::ClusterPod],
) -> BTreeMap<String, NodePods> {
    let volumes = if pods.iter().any(|p| !p.claims.is_empty()) {
        bound_volumes(cluster).await
    } else {
        BTreeMap::new()
    };
    let hosts: BTreeMap<&str, String> = nodes
        .iter()
        .filter_map(|n| Some((n.metadata.name.as_deref()?, hostname(n))))
        .collect();
    let mut out: BTreeMap<String, NodePods> = BTreeMap::new();
    for pod in pods.iter().filter(|p| movable(p)) {
        let Some(node) = &pod.node_name else {
            continue;
        };
        let host = hosts
            .get(node.as_str())
            .map_or(node.as_str(), String::as_str);
        let entry = out.entry(node.clone()).or_default();
        if !pod.terminating && pinned(pod, host, &volumes) {
            entry.pinned += 1;
        } else {
            entry.moving += 1;
            entry.leaving += u64::from(pod.terminating);
        }
    }
    out
}

/// Whether maintenance would move a pod listed across the cluster: every unfinished pod but a
/// DaemonSet's and a static pod's mirror.
pub(crate) fn movable(pod: &zeabur_kube::ClusterPod) -> bool {
    !pod.mirror
        && pod.controller.as_deref() != Some("DaemonSet")
        && !matches!(pod.phase.as_deref(), Some("Succeeded" | "Failed"))
}

/// The cluster's PersistentVolumes by the claim bound to them (`namespace/name`, from
/// `spec.claimRef`), in one list; empty when the person may not list them, so no pod counts as
/// pinned.
async fn bound_volumes(cluster: &dyn Cluster) -> BTreeMap<String, Value> {
    let resource = ResourceRef::new("", "v1", "persistentvolumes", "PersistentVolume");
    let listed = cluster.list_objects(&resource, None).await.ok().flatten();
    values(listed)
        .into_iter()
        .filter_map(|volume| {
            let namespace = volume.pointer("/spec/claimRef/namespace")?.as_str()?;
            let claim = volume.pointer("/spec/claimRef/name")?.as_str()?;
            Some((format!("{namespace}/{claim}"), volume))
        })
        .collect()
}

/// Whether a pod listed across the cluster has a claim bound to a volume only the node called
/// `host` can use: maintenance leaves it there unless the person opts in.
fn pinned(pod: &zeabur_kube::ClusterPod, host: &str, volumes: &BTreeMap<String, Value>) -> bool {
    pod.claims.iter().any(|claim| {
        volumes
            .get(&format!("{}/{claim}", pod.namespace))
            .is_some_and(|volume| kept_on(volume, host))
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use zeabur_core::problem::KubernetesStatus;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{DynamicObject, NamespaceObjects, ServedResource};

    use super::*;

    fn names(pods: &[DrainPod], fate: PodFate) -> BTreeSet<String> {
        pods.iter()
            .filter(|p| p.fate == fate)
            .map(|p| format!("{}/{}", p.namespace, p.name))
            .collect()
    }

    fn node(name: &str) -> Node {
        serde_json::from_value(json!({
            "metadata": {"name": name, "uid": format!("uid-{name}"),
                         "labels": {HOSTNAME: name}},
            "spec": {},
            "status": {"conditions": [{"type": "Ready", "status": "True"}],
                       "allocatable": {"cpu": "2", "memory": "4Gi", "pods": "110"}}
        }))
        .unwrap()
    }

    fn pod(namespace: &str, name: &str, owner: Option<(&str, &str)>, extra: &Value) -> Pod {
        let owners = owner.map_or_else(
            || json!([]),
            |(kind, owner)| {
                json!([{"apiVersion": "apps/v1", "kind": kind, "name": owner,
                                   "uid": "o", "controller": true}])
            },
        );
        let mut value = json!({
            "metadata": {"name": name, "namespace": namespace, "uid": format!("{namespace}-{name}"),
                         "labels": {"app": name.split('-').next().unwrap(), TEMPLATE_HASH: "7c9d8"},
                         "ownerReferences": owners},
            "spec": {"nodeName": "vps-3", "containers": [{"name": "main", "image": "x",
                "resources": {"requests": {"cpu": "500m", "memory": "1Gi"}}}]},
            "status": {"phase": "Running"}
        });
        merge(&mut value, extra);
        serde_json::from_value(value).unwrap()
    }

    fn merge(into: &mut Value, from: &Value) {
        match (into, from) {
            (Value::Object(a), Value::Object(b)) => {
                for (k, v) in b {
                    merge(a.entry(k.clone()).or_insert(Value::Null), v);
                }
            }
            (slot, value) => *slot = value.clone(),
        }
    }

    fn local_volume(name: &str, host: &str) -> Value {
        json!({"apiVersion": "v1", "kind": "PersistentVolume",
               "metadata": {"name": name},
               "spec": {"hostPath": {"path": "/opt/local-path"},
                        "nodeAffinity": {"required": {"nodeSelectorTerms": [
                            {"matchExpressions": [{"key": HOSTNAME, "operator": "In",
                                                   "values": [host]}]}]}}}})
    }

    fn pods() -> Vec<Pod> {
        vec![
            pod(
                "shop",
                "web-7c9d8-abcde",
                Some(("ReplicaSet", "web-7c9d8")),
                &json!({}),
            ),
            pod(
                "shop",
                "cache-7c9d8-xyz",
                Some(("ReplicaSet", "cache-7c9d8")),
                &json!({"spec": {"volumes": [{"name": "tmp", "emptyDir": {}}]}}),
            ),
            pod(
                "shop",
                "db-0",
                Some(("StatefulSet", "db")),
                &json!({"spec": {"volumes": [{"name": "data",
                    "persistentVolumeClaim": {"claimName": "data-db-0"}}]}}),
            ),
            pod(
                "shop",
                "queue-0",
                Some(("StatefulSet", "queue")),
                &json!({"spec": {"volumes": [{"name": "data",
                    "persistentVolumeClaim": {"claimName": "data-queue-0"}}]}}),
            ),
            pod(
                "kube-system",
                "canal-x",
                Some(("DaemonSet", "canal")),
                &json!({}),
            ),
            pod(
                "kube-system",
                "kube-apiserver-vps-3",
                None,
                &json!({"metadata": {"annotations": {MIRROR: "a"}}}),
            ),
            pod("shop", "debug", None, &json!({})),
            pod(
                "shop",
                "migrate-x",
                Some(("Job", "migrate")),
                &json!({"status": {"phase": "Succeeded"}}),
            ),
        ]
    }

    fn context() -> Context {
        Context {
            claims: BTreeMap::from([
                ("shop/data-db-0".to_owned(), "pvc-local".to_owned()),
                ("shop/data-queue-0".to_owned(), "pvc-elsewhere".to_owned()),
            ]),
            volumes: BTreeMap::from([
                ("pvc-local".to_owned(), local_volume("pvc-local", "vps-3")),
                (
                    "pvc-elsewhere".to_owned(),
                    local_volume("pvc-elsewhere", "vps-1"),
                ),
            ]),
            budgets: vec![json!({"metadata": {"name": "web", "namespace": "shop"},
                "spec": {"selector": {"matchLabels": {"app": "web"}}},
                "status": {"disruptionsAllowed": 0}})],
            namespaces: vec![
                serde_json::from_value(json!({"metadata": {"name": "shop",
                "labels": {LABEL_PROJECT: "true"},
                "annotations": {"zeabur.com/display-name": "Shop"}}}))
                .unwrap(),
            ],
            zeabur_namespace: Some("zeabur".to_owned()),
        }
    }

    #[test]
    fn pods_are_classified_as_kubectl_drain_treats_them() {
        let classified = classify(&node("vps-3"), &pods(), &context());
        let set = |names: &[&str]| {
            names
                .iter()
                .map(|n| (*n).to_owned())
                .collect::<BTreeSet<_>>()
        };
        assert_eq!(
            names(&classified, PodFate::Moves),
            set(&[
                "shop/cache-7c9d8-xyz",
                "shop/queue-0",
                "shop/web-7c9d8-abcde"
            ])
        );
        assert_eq!(
            names(&classified, PodFate::Stays),
            set(&["kube-system/canal-x", "kube-system/kube-apiserver-vps-3"])
        );
        assert_eq!(names(&classified, PodFate::Stuck), set(&["shop/db-0"]));
        assert_eq!(names(&classified, PodFate::Deleted), set(&["shop/debug"]));
        assert!(
            !classified.iter().any(|p| p.name == "migrate-x"),
            "finished pods are left out"
        );
        assert_eq!(classified.first().map(|p| p.fate), Some(PodFate::Moves));
    }

    #[test]
    fn a_pod_says_what_it_loses_who_owns_it_and_what_holds_it() {
        let classified = classify(&node("vps-3"), &pods(), &context());
        let find = |name: &str| classified.iter().find(|p| p.name == name).unwrap();
        let web = find("web-7c9d8-abcde");
        assert_eq!(
            web.workload,
            Some(PodWorkload {
                kind: "Deployment".to_owned(),
                name: "web".to_owned()
            })
        );
        assert_eq!(
            (web.project.as_deref(), web.project_name.as_deref()),
            (Some("shop"), Some("Shop"))
        );
        assert_eq!(
            (web.budget.as_deref(), web.budget_blocks),
            (Some("web"), true)
        );
        assert_eq!(find("cache-7c9d8-xyz").scratch, ["tmp"]);
        assert_eq!(find("db-0").local_claims, ["data-db-0"]);
        assert_eq!(find("canal-x").stays, Some(StayReason::DaemonSet));
        assert_eq!(find("kube-apiserver-vps-3").stays, Some(StayReason::Static));
        assert_eq!(find("kube-apiserver-vps-3").workload, None);
        assert_eq!(find("canal-x").project, None);
        assert_eq!(web.requests.memory_bytes, 1 << 30);
    }

    #[test]
    fn a_budget_holds_the_drain_when_the_node_has_more_of_its_pods_than_it_allows() {
        let replica = |name: &str| {
            pod(
                "shop",
                name,
                Some(("ReplicaSet", "web-7c9d8")),
                &json!({"metadata": {"labels": {"app": "web"}}}),
            )
        };
        let mut context = context();
        context.budgets[0]["status"]["disruptionsAllowed"] = json!(1);
        let blocks = |pods: &[Pod]| {
            classify(&node("vps-3"), pods, &context)
                .iter()
                .all(|p| p.budget_blocks)
        };
        assert!(!blocks(&[replica("web-7c9d8-a")]));
        assert!(blocks(&[replica("web-7c9d8-a"), replica("web-7c9d8-b")]));
    }

    #[test]
    fn a_volume_is_kept_on_a_node_only_when_its_affinity_names_that_node_alone() {
        assert!(kept_on(&local_volume("a", "vps-3"), "vps-3"));
        assert!(!kept_on(&local_volume("a", "vps-1"), "vps-3"));
        let mut two = local_volume("a", "vps-3");
        two["spec"]["nodeAffinity"]["required"]["nodeSelectorTerms"][0]["matchExpressions"][0]["values"] =
            json!(["vps-3", "vps-1"]);
        assert!(!kept_on(&two, "vps-3"));
        assert!(!kept_on(&json!({"spec": {"csi": {}}}), "vps-3"));
    }

    #[test]
    fn zeabur_is_found_by_its_namespace_and_chart_label() {
        let mut ours = pod(
            "zeabur",
            "zeabur-5f6-abc",
            Some(("ReplicaSet", "zeabur-5f6")),
            &json!({}),
        );
        ours.metadata
            .labels
            .get_or_insert_default()
            .insert("app.kubernetes.io/name".to_owned(), ZEABUR_APP.to_owned());
        let classified = classify(&node("vps-3"), &[ours.clone()], &context());
        assert!(classified.iter().all(|p| p.zeabur));
        ours.metadata.namespace = Some("elsewhere".to_owned());
        let classified = classify(&node("vps-3"), &[ours], &context());
        assert!(!classified.iter().any(|p| p.zeabur));
    }

    fn moving(name: &str, cpu: u64, memory: u64) -> DrainPod {
        DrainPod {
            namespace: "shop".to_owned(),
            name: name.to_owned(),
            fate: PodFate::Moves,
            stays: None,
            project: None,
            project_name: None,
            workload: None,
            scratch: Vec::new(),
            local_claims: Vec::new(),
            budget: None,
            budget_blocks: false,
            zeabur: false,
            terminating: false,
            requests: Resources {
                cpu_millicores: cpu,
                memory_bytes: memory,
                pods: None,
            },
        }
    }

    #[test]
    fn the_moving_pods_are_placed_largest_first_and_what_fits_nowhere_is_named() {
        let (a, b, c) = (
            moving("a", 500, 3),
            moving("b", 500, 2),
            moving("c", 500, 2),
        );
        let room = |m1: u64, m2: u64| {
            Some(vec![
                (
                    Amount {
                        cpu: 2000,
                        memory: m1,
                    },
                    10,
                ),
                (
                    Amount {
                        cpu: 2000,
                        memory: m2,
                    },
                    10,
                ),
            ])
        };
        let fits = place(&[&a, &b, &c], room(4, 3));
        assert_eq!(fits.status, CapacityStatus::Fits);
        assert_eq!(fits.needs.memory_bytes, 7);
        assert_eq!(fits.room.map(|r| r.memory_bytes), Some(7));
        let short = place(&[&a, &b, &c], room(4, 2));
        assert_eq!(short.status, CapacityStatus::Short);
        assert_eq!(short.unplaced, ["shop/c"]);
        let no_slots = place(
            &[&a],
            Some(vec![(
                Amount {
                    cpu: 9000,
                    memory: 9,
                },
                0,
            )]),
        );
        assert_eq!(no_slots.unplaced, ["shop/a"]);
        assert_eq!(place(&[&a], None).status, CapacityStatus::Unknown);
    }

    fn cluster(pods: Vec<Pod>) -> MemoryCluster {
        let mut cordoned = node("vps-3");
        cordoned.spec.get_or_insert_default().unschedulable = Some(true);
        MemoryCluster::new(State {
            nodes: vec![cordoned, node("vps-1")],
            objects: BTreeMap::from([(
                "shop".to_owned(),
                NamespaceObjects {
                    namespace: "shop".to_owned(),
                    pods,
                    ..NamespaceObjects::default()
                },
            )]),
            ..State::default()
        })
    }

    fn planned(name: &str) -> Planned {
        Planned {
            namespace: "shop".to_owned(),
            name: name.to_owned(),
            uid: format!("shop-{name}"),
            fate: PodFate::Moves,
            zeabur: false,
        }
    }

    #[tokio::test]
    async fn a_pass_evicts_what_is_left_and_reports_what_a_budget_holds() {
        let web = pod(
            "shop",
            "web-1",
            Some(("ReplicaSet", "web-7c9d8")),
            &json!({}),
        );
        let api = pod(
            "shop",
            "api-1",
            Some(("ReplicaSet", "api-7c9d8")),
            &json!({}),
        );
        let leaving = pod(
            "shop",
            "old-1",
            Some(("ReplicaSet", "old-7c9d8")),
            &json!({"metadata": {"deletionTimestamp": "2026-10-23T10:00:00Z"}}),
        );
        let cluster = cluster(vec![web, api, leaving]);
        cluster.update(|s| {
            s.eviction_refusals.insert(
                "shop/api-1".to_owned(),
                KubernetesStatus {
                    retry_after_seconds: Some(5),
                    ..KubernetesStatus::new(
                        429,
                        "TooManyRequests",
                        "Cannot evict pod as it would violate the pod's disruption budget.",
                    )
                },
            );
            s.served.push(ServedResource::readable(
                "policy",
                "v1",
                "poddisruptionbudgets",
                "PodDisruptionBudget",
                true,
            ));
            s.dynamic.push(
                serde_json::from_value::<DynamicObject>(json!({
                    "apiVersion": "policy/v1", "kind": "PodDisruptionBudget",
                    "metadata": {"name": "api-budget", "namespace": "shop"},
                    "spec": {"selector": {"matchLabels": {"app": "api"}}}}))
                .unwrap(),
            );
        });
        let targets = [
            planned("web-1"),
            planned("api-1"),
            planned("old-1"),
            planned("gone-1"),
        ];
        let progress = pass(&cluster, "vps-3", &targets).await.unwrap();
        let outcomes: Vec<(&str, EvictOutcome)> = progress
            .pods
            .iter()
            .map(|p| (p.name.as_str(), p.outcome))
            .collect();
        assert_eq!(
            outcomes,
            [
                ("web-1", EvictOutcome::Leaving),
                ("api-1", EvictOutcome::Waiting),
                ("old-1", EvictOutcome::Leaving),
                ("gone-1", EvictOutcome::Gone),
            ]
        );
        let api = progress.pods.iter().find(|p| p.name == "api-1").unwrap();
        assert_eq!(
            (api.budget.as_deref(), api.retry_after_seconds),
            (Some("api-budget"), Some(5))
        );
        assert_eq!(
            (progress.remaining, progress.done, progress.cordoned),
            (3, false, true)
        );
        let evictions: Vec<String> = cluster
            .inspect(|s| s.writes.iter().map(|w| w.target.name.clone()).collect())
            .unwrap();
        assert_eq!(evictions, ["web-1"], "a leaving pod is not evicted again");
    }

    #[tokio::test]
    async fn zeabur_itself_moves_only_once_the_other_pods_are_off_the_node() {
        let web = pod(
            "shop",
            "web-1",
            Some(("ReplicaSet", "web-7c9d8")),
            &json!({}),
        );
        let zeabur = pod(
            "shop",
            "zeabur-1",
            Some(("ReplicaSet", "zeabur-7c9d8")),
            &json!({}),
        );
        let cluster = cluster(vec![web, zeabur]);
        let ours = Planned {
            zeabur: true,
            ..planned("zeabur-1")
        };
        let targets = [ours.clone(), planned("web-1")];
        let first = pass(&cluster, "vps-3", &targets).await.unwrap();
        let outcomes: Vec<(&str, EvictOutcome)> = first
            .pods
            .iter()
            .map(|p| (p.name.as_str(), p.outcome))
            .collect();
        assert_eq!(
            outcomes,
            [
                ("zeabur-1", EvictOutcome::Last),
                ("web-1", EvictOutcome::Leaving)
            ]
        );
        let evicted = |cluster: &MemoryCluster| -> Vec<String> {
            cluster
                .inspect(|s| s.writes.iter().map(|w| w.target.name.clone()).collect())
                .unwrap()
        };
        assert_eq!(evicted(&cluster), ["web-1"]);

        let second = pass(&cluster, "vps-3", &[ours, planned("gone-1")])
            .await
            .unwrap();
        assert_eq!(
            second.pods.first().map(|p| p.outcome),
            Some(EvictOutcome::Leaving)
        );
        assert_eq!(evicted(&cluster), ["web-1", "zeabur-1"]);
    }

    #[tokio::test]
    async fn a_pass_is_done_once_the_planned_pods_are_off_the_node() {
        let replaced = pod(
            "shop",
            "web-1",
            Some(("ReplicaSet", "web-7c9d8")),
            &json!({
            "metadata": {"uid": "another"}}),
        );
        let cluster = cluster(vec![replaced]);
        let progress = pass(&cluster, "vps-3", &[planned("web-1")]).await.unwrap();
        assert_eq!(
            progress.pods.first().map(|p| p.outcome),
            Some(EvictOutcome::Gone)
        );
        assert!(progress.done);
    }

    #[tokio::test]
    async fn a_refused_eviction_is_reported_as_refused() {
        let web = pod(
            "shop",
            "web-1",
            Some(("ReplicaSet", "web-7c9d8")),
            &json!({}),
        );
        let cluster = cluster(vec![web]);
        cluster.update(|s| {
            s.forbidden_writes.insert("Pod/web-1".to_owned());
        });
        let progress = pass(&cluster, "vps-3", &[planned("web-1")]).await.unwrap();
        assert_eq!(
            progress.pods.first().map(|p| p.outcome),
            Some(EvictOutcome::Refused)
        );
        assert!(!progress.done);
    }

    #[test]
    fn cordoning_tests_the_nodes_uid_and_resuming_removes_the_field() {
        let mut n = node("vps-3");
        let patch = unschedulable_patch(&n, true).unwrap();
        assert_eq!(
            patch,
            json!([{"op": "test", "path": "/metadata/uid", "value": "uid-vps-3"},
                   {"op": "add", "path": "/spec/unschedulable", "value": true}])
        );
        assert_eq!(unschedulable_patch(&n, false), None);
        n.spec.get_or_insert_default().unschedulable = Some(true);
        assert_eq!(unschedulable_patch(&n, true), None);
        assert_eq!(
            unschedulable_patch(&n, false).unwrap()[1],
            json!({"op": "remove", "path": "/spec/unschedulable"})
        );
    }

    #[test]
    fn only_the_opted_in_pods_are_evicted() {
        let claims = Claims {
            person: String::new(),
            groups: Vec::new(),
            node: "vps-3".to_owned(),
            node_uid: String::new(),
            action: NodeAction::Drain,
            pods: [PodFate::Moves, PodFate::Stuck, PodFate::Deleted]
                .into_iter()
                .map(|fate| Planned {
                    fate,
                    ..planned(&format!("{fate:?}"))
                })
                .collect(),
            expires_at: 0,
        };
        let pick = |stuck, deleted| {
            chosen(
                &claims,
                &MaintenanceApply {
                    token: String::new(),
                    include_stuck: stuck,
                    include_deleted: deleted,
                },
            )
            .into_iter()
            .map(|p| p.fate)
            .collect::<Vec<_>>()
        };
        assert_eq!(pick(false, false), [PodFate::Moves]);
        assert_eq!(
            pick(true, true),
            [PodFate::Moves, PodFate::Stuck, PodFate::Deleted]
        );
    }
}
