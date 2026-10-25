//! Node maintenance: stop scheduling (cordon) and resume scheduling (uncordon).
//!
//! Moving the pods off a node and removing it are done with `kubectl drain` and `kubectl delete
//! node` (`docs/zh-TW/cluster/nodes.mdx`).
//!
//! A plan reads the node and says what it will do, with a token bound to the person, the node's
//! `uid` and the action. Apply patches `spec.unschedulable` (a dry run first). The node list also
//! counts each node's pods as `kubectl drain` sees them (`node_pods`), so a cordoned node says
//! whether replicas are still leaving or stay because their volume is only on this node.
//!
//! Everything is read and written as the person: RBAC decides whether they may patch the node
//! (`patch nodes`).

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use zeabur_core::problem::{Denied, Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::api::core::v1::Node;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{
    Cluster, ClusterError, ClusterExt as _, ObjectTarget, ResourceRef, WriteBody, WriteRequest,
};

use crate::error::{ApiError, get_read, read_problem};
use crate::handlers::{AppState, Person};
use crate::plan_token::{self as token, LIFETIME_SECONDS, TokenError};
use crate::projects::writes::token_problem;

/// Sealed plans of this module start with this, so no other plan's token opens as one.
const PREFIX: &str = "d1.";
const HOSTNAME: &str = "kubernetes.io/hostname";

/// What a plan changes on a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum NodeAction {
    /// Stop scheduling new pods on it (`spec.unschedulable: true`).
    Cordon,
    /// Schedule new pods on it again.
    Uncordon,
}

/// The action to plan.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct MaintenanceRequest {
    /// What to do.
    pub action: NodeAction,
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
}

/// The token of a plan.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct MaintenanceApply {
    /// The token `plan` returned.
    pub token: String,
}

/// The node after an apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct MaintenanceApplied {
    /// Whether the node is cordoned.
    pub cordoned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Claims {
    person: String,
    groups: Vec<String>,
    node: String,
    node_uid: String,
    action: NodeAction,
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

fn plan_key(state: &AppState) -> Result<std::sync::Arc<token::PlanKey>, ApiError> {
    state.plan_key.clone().ok_or_else(|| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "Zeabur could not make a token key when it started, so it cannot plan changes.",
        ))
    })
}

/// Works out what stopping or resuming scheduling does to the node, and a token to apply it;
/// writes nothing.
#[utoipa::path(post, path = "/api/v1/cluster/nodes/{node}/maintenance/plan", tag = "cluster",
    operation_id = "planNodeMaintenance", request_body = MaintenanceRequest,
    params(("node" = String, Path, description = "The node's name")),
    responses((status = 200, body = MaintenancePlan),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn plan(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path(name): Path<String>,
    Json(request): Json<MaintenanceRequest>,
) -> Result<Json<MaintenancePlan>, ApiError> {
    let sealing = plan_key(&state)?;
    let identity = cluster
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let node = read_node(cluster.as_ref(), &name).await?;
    let expires_at = (state.clock)().as_second().saturating_add(LIFETIME_SECONDS);
    let claims = Claims {
        person: identity.username,
        groups: identity.groups,
        node: name.clone(),
        node_uid: node.metadata.uid.clone().unwrap_or_default(),
        action: request.action,
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
        node: name,
        action: request.action,
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

/// Opens a token, checks that the same person sends it and that it has not expired, and reads its
/// node, refusing one that was replaced.
async fn opened(
    state: &AppState,
    cluster: &dyn Cluster,
    name: &str,
    token: &str,
) -> Result<(Claims, Node), ApiError> {
    let sealing = plan_key(state)?;
    let claims: Claims = token::open_as(&sealing, PREFIX, token).map_err(token_problem)?;
    if (state.clock)().as_second() >= claims.expires_at {
        return Err(token_problem(TokenError::Expired));
    }
    let identity = cluster
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

/// Carries out a plan by its token: patches `spec.unschedulable`.
#[utoipa::path(post, path = "/api/v1/cluster/nodes/{node}/maintenance/apply", tag = "cluster",
    operation_id = "applyNodeMaintenance", request_body = MaintenanceApply,
    params(("node" = String, Path, description = "The node's name")),
    responses((status = 200, body = MaintenanceApplied),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn apply(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path(name): Path<String>,
    Json(request): Json<MaintenanceApply>,
) -> Result<Json<MaintenanceApplied>, ApiError> {
    let (claims, node) = opened(&state, cluster.as_ref(), &name, &request.token).await?;
    let cordon = claims.action == NodeAction::Cordon;
    if let Some(patch) = unschedulable_patch(&node, cordon) {
        patch_node(cluster.as_ref(), &name, patch).await?;
    }
    Ok(Json(MaintenanceApplied { cordoned: cordon }))
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
    use zeabur_kube::k8s_openapi::api::core::v1::Node;

    use super::*;

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

    fn local_volume(name: &str, host: &str) -> Value {
        json!({"apiVersion": "v1", "kind": "PersistentVolume",
               "metadata": {"name": name},
               "spec": {"hostPath": {"path": "/opt/local-path"},
                        "nodeAffinity": {"required": {"nodeSelectorTerms": [
                            {"matchExpressions": [{"key": HOSTNAME, "operator": "In",
                                                   "values": [host]}]}]}}}})
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
}
