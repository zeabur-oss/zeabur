//! The installer's baseline NetworkPolicies against the nodes, and the plan that brings them up
//! to date the way `zeabur-installer node sync` does.
//!
//! `zeabur-installer` puts a NetworkPolicy `zeabur-installer-baseline` (labelled
//! `app.kubernetes.io/managed-by: zeabur-installer`) in each namespace it manages. Its one ingress
//! rule lets in the namespace's pods, the installer's namespaces, and one `ipBlock` per node
//! address: every node's `InternalIP` and the first address of each of its `spec.podCIDRs` (where
//! Canal's VXLAN traffic comes from), as `/32` or `/128`, sorted. A node that joins later is
//! dropped by them until the blocks are rewritten.
//!
//! The plan rewrites only those blocks; the rest of each policy stays as it is. Like a member
//! plan, its token binds the person, the node addresses and each policy's `resourceVersion`;
//! apply computes the writes again, refuses when any of those moved, dry-runs every write, then
//! writes. Everything is read and written as the person.

use std::collections::{BTreeMap, BTreeSet};

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use zeabur_core::problem::{Denied, Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::api::core::v1::Node;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{Cluster, ClusterError, ClusterExt as _, ObjectTarget, WriteBody, WriteRequest};
use zeabur_modules::network_policy::NETWORK_POLICIES;

use crate::error::{ApiError, list_read, read_problem};
use crate::handlers::{AppState, Everyday, Person};
use crate::plan_token::{self as token, LIFETIME_SECONDS, TokenError};
use crate::projects::namespace::Permission;
use crate::projects::writes::token_problem;
use crate::views::UnreadableView;

/// The name of every baseline policy.
pub const BASELINE: &str = "zeabur-installer-baseline";
const MANAGED_BY: &str = "app.kubernetes.io/managed-by";
const INSTALLER: &str = "zeabur-installer";
const GROUP: &str = "networking.k8s.io";
const RESOURCE: &str = "networkpolicies";
/// Sealed plans of this module start with this, so no other plan's token opens as one.
const PREFIX: &str = "n1.";

/// A baseline policy and the node addresses it does not let in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BaselinePolicy {
    /// Its namespace.
    pub namespace: String,
    /// Node addresses it has no block for, sorted.
    pub missing: Vec<String>,
    /// Whether its blocks are exactly the nodes' addresses.
    pub current: bool,
}

/// A node some baseline policy drops traffic from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct NodeBehind {
    /// The node.
    pub name: String,
    /// Its addresses a policy does not let in.
    pub addresses: Vec<String>,
}

/// The baseline policies against the nodes, as far as the person may read them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodePolicies {
    /// The addresses every baseline policy should let in, sorted.
    pub addresses: Vec<String>,
    /// The baseline policies, by namespace; empty on a cluster the installer did not set up.
    pub policies: Vec<BaselinePolicy>,
    /// Nodes whose traffic a policy drops, by name.
    pub behind: Vec<NodeBehind>,
    /// Whether the person may patch the policies that are not current.
    pub can_update: Permission,
    /// The reads that were refused (`nodes`, `networkpolicies.networking.k8s.io`).
    pub unreadable: Vec<UnreadableView>,
}

/// One policy a plan rewrites.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PolicyWrite {
    /// Its namespace.
    pub namespace: String,
    /// Its name.
    pub name: String,
    /// The policy as it is, YAML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_yaml: Option<String>,
    /// The policy as it will be, YAML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_yaml: Option<String>,
}

/// A plan that brings the baseline policies up to date, and the token that applies it.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodePoliciesPlan {
    /// Send this to `apply` to carry out exactly this plan.
    pub token: String,
    /// When the token stops working, RFC 3339.
    pub expires_at: String,
    /// The addresses the policies will let in.
    pub addresses: Vec<String>,
    /// The policies it rewrites; empty when all are current.
    pub writes: Vec<PolicyWrite>,
}

/// The token of a plan to apply.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct NodePoliciesApply {
    /// The token `plan` returned.
    pub token: String,
}

/// What an apply wrote.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct NodePoliciesApplied {
    /// The namespaces whose policy was rewritten.
    pub namespaces: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Claims {
    person: String,
    groups: Vec<String>,
    addresses: Vec<String>,
    versions: BTreeMap<String, String>,
    expires_at: i64,
}

/// The addresses node traffic comes from: each node's `InternalIP`, and the first address of each
/// of its pod ranges. Sorted, without repeats.
fn node_addresses(node: &Node) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let internal = node
        .status
        .as_ref()
        .and_then(|s| s.addresses.as_ref())
        .into_iter()
        .flatten()
        .filter(|a| a.type_ == "InternalIP")
        .map(|a| a.address.clone());
    found.extend(internal);
    let ranges = node
        .spec
        .as_ref()
        .and_then(|s| s.pod_cidrs.as_ref())
        .into_iter()
        .flatten()
        .filter_map(|cidr| cidr.split_once('/').map(|(base, _)| base.to_owned()));
    found.extend(ranges);
    found
}

fn all_addresses(nodes: &[Node]) -> BTreeSet<String> {
    nodes.iter().flat_map(node_addresses).collect()
}

/// The addresses a policy's `ipBlock`s let in.
fn policy_sources(policy: &Value) -> BTreeSet<String> {
    policy
        .pointer("/spec/ingress/0/from")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|f| f.pointer("/ipBlock/cidr").and_then(Value::as_str))
        .filter_map(|c| c.split_once('/').map(|(a, _)| a.to_owned()))
        .collect()
}

fn is_baseline(policy: &Value) -> bool {
    policy.pointer("/metadata/name").and_then(Value::as_str) == Some(BASELINE)
        && policy
            .pointer("/metadata/labels")
            .and_then(|l| l.get(MANAGED_BY))
            .and_then(Value::as_str)
            == Some(INSTALLER)
}

fn text<'a>(object: &'a Value, path: &str) -> &'a str {
    object.pointer(path).and_then(Value::as_str).unwrap_or("")
}

/// The `from` of the first ingress rule with its `ipBlock`s replaced by `addresses`, after the
/// other sources, which stay in their order.
fn rewritten_from(policy: &Value, addresses: &BTreeSet<String>) -> Value {
    let mut from: Vec<Value> = policy
        .pointer("/spec/ingress/0/from")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|f| f.get("ipBlock").is_none())
        .cloned()
        .collect();
    from.extend(addresses.iter().map(|a| {
        let bits = if a.contains(':') { 128 } else { 32 };
        json!({"ipBlock": {"cidr": format!("{a}/{bits}")}})
    }));
    Value::Array(from)
}

/// The JSON Patch that rewrites `policy`'s blocks, and the policy as it will be; `None` when its
/// blocks are already `addresses`.
fn rewrite(policy: &Value, addresses: &BTreeSet<String>) -> Option<(Vec<Value>, Value)> {
    if policy_sources(policy) == *addresses {
        return None;
    }
    let from = rewritten_from(policy, addresses);
    let mut operations = Vec::new();
    if let Some(version) = policy
        .pointer("/metadata/resourceVersion")
        .and_then(Value::as_str)
    {
        operations
            .push(json!({"op": "test", "path": "/metadata/resourceVersion", "value": version}));
    }
    let mut after = policy.clone();
    let has_rule = policy
        .pointer("/spec/ingress")
        .and_then(Value::as_array)
        .is_some_and(|rules| !rules.is_empty());
    if has_rule {
        operations.push(json!({"op": "add", "path": "/spec/ingress/0/from", "value": from}));
        if let Some(rule) = after
            .pointer_mut("/spec/ingress/0")
            .and_then(Value::as_object_mut)
        {
            rule.insert("from".to_owned(), from);
        }
    } else {
        let rules = json!([{"from": from}]);
        operations.push(json!({"op": "add", "path": "/spec/ingress", "value": rules}));
        if let Some(spec) = after.get_mut("spec").and_then(Value::as_object_mut) {
            spec.insert("ingress".to_owned(), rules);
        }
    }
    Some((operations, after))
}

/// One policy the plan rewrites.
#[derive(Debug, Clone)]
struct Rewrite {
    namespace: String,
    operations: Vec<Value>,
    before: Value,
    after: Value,
}

/// What a plan reads and writes.
#[derive(Debug, Clone)]
struct Computed {
    addresses: BTreeSet<String>,
    versions: BTreeMap<String, String>,
    rewrites: Vec<Rewrite>,
}

fn computed(nodes: &[Node], policies: &[Value]) -> Computed {
    let addresses = all_addresses(nodes);
    let mut versions = BTreeMap::new();
    let mut rewrites = Vec::new();
    for policy in policies.iter().filter(|p| is_baseline(p)) {
        let namespace = text(policy, "/metadata/namespace").to_owned();
        versions.insert(
            namespace.clone(),
            text(policy, "/metadata/resourceVersion").to_owned(),
        );
        if let Some((operations, after)) = rewrite(policy, &addresses) {
            rewrites.push(Rewrite {
                namespace,
                operations,
                before: policy.clone(),
                after,
            });
        }
    }
    rewrites.sort_by(|a, b| a.namespace.cmp(&b.namespace));
    Computed {
        addresses,
        versions,
        rewrites,
    }
}

/// The baseline policies against `nodes`.
fn compare(nodes: &[Node], policies: &[Value]) -> (Vec<BaselinePolicy>, Vec<NodeBehind>) {
    let addresses = all_addresses(nodes);
    let mut missing_anywhere = BTreeSet::new();
    let mut out: Vec<BaselinePolicy> = policies
        .iter()
        .filter(|p| is_baseline(p))
        .map(|policy| {
            let have = policy_sources(policy);
            let missing: Vec<String> = addresses.difference(&have).cloned().collect();
            missing_anywhere.extend(missing.iter().cloned());
            BaselinePolicy {
                namespace: text(policy, "/metadata/namespace").to_owned(),
                missing,
                current: rewrite(policy, &addresses).is_none(),
            }
        })
        .collect();
    out.sort_by(|a, b| a.namespace.cmp(&b.namespace));
    let mut behind: Vec<NodeBehind> = nodes
        .iter()
        .filter_map(|node| {
            let addresses: Vec<String> = node_addresses(node)
                .into_iter()
                .filter(|a| missing_anywhere.contains(a))
                .collect();
            (!addresses.is_empty()).then(|| NodeBehind {
                name: node.metadata.name.clone().unwrap_or_default(),
                addresses,
            })
        })
        .collect();
    behind.sort_by(|a, b| a.name.cmp(&b.name));
    (out, behind)
}

fn patch_right(namespace: &str) -> Denied {
    Denied {
        verb: "patch".to_owned(),
        group: GROUP.to_owned(),
        resource: RESOURCE.to_owned(),
        name: Some(BASELINE.to_owned()),
        namespace: Some(namespace.to_owned()),
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

async fn read_policies(cluster: &dyn Cluster) -> Result<Vec<Value>, ClusterError> {
    let objects = cluster
        .list_objects(&NETWORK_POLICIES.resource(), None)
        .await?
        .unwrap_or_default();
    Ok(objects
        .iter()
        .filter_map(|o| serde_json::to_value(o).ok())
        .collect())
}

/// Whether the installer's baseline network policies let every node's traffic in. Read as the
/// person; a read they may not make is listed in `unreadable`.
#[utoipa::path(get, path = "/api/v1/cluster/node-policies", tag = "cluster",
    operation_id = "getNodePolicies",
    responses((status = 200, body = NodePolicies), (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(Person(cluster): Person) -> Result<Json<NodePolicies>, ApiError> {
    let (nodes, policies) =
        futures_util::join!(cluster.list::<Node>(None), read_policies(cluster.as_ref()));
    let mut unreadable = Vec::new();
    let nodes = readable(nodes, "nodes", &mut unreadable)?;
    let policies = readable(policies, &format!("{RESOURCE}.{GROUP}"), &mut unreadable)?;
    let (Some(nodes), Some(policies)) = (nodes, policies) else {
        return Ok(Json(NodePolicies {
            addresses: Vec::new(),
            policies: Vec::new(),
            behind: Vec::new(),
            can_update: Permission::Unknown,
            unreadable,
        }));
    };
    let (baseline, behind) = compare(&nodes, &policies);
    let mut can_update = Permission::Allowed;
    for policy in baseline.iter().filter(|p| !p.current) {
        let answer = match cluster.review_access(&patch_right(&policy.namespace)).await {
            Ok(true) => Permission::Allowed,
            Ok(false) => Permission::Denied,
            Err(_) => Permission::Unknown,
        };
        can_update = match (can_update, answer) {
            (Permission::Denied, _) | (_, Permission::Denied) => Permission::Denied,
            (Permission::Unknown, _) | (_, Permission::Unknown) => Permission::Unknown,
            _ => Permission::Allowed,
        };
    }
    Ok(Json(NodePolicies {
        addresses: all_addresses(&nodes).into_iter().collect(),
        policies: baseline,
        behind,
        can_update,
        unreadable,
    }))
}

async fn compute(cluster: &dyn Cluster) -> Result<Computed, ApiError> {
    let nodes = cluster
        .list::<Node>(None)
        .await
        .map_err(|e| read_problem(e, "the nodes", list_read("", "nodes", None)))?;
    let policies = read_policies(cluster)
        .await
        .map_err(|e| read_problem(e, "the network policies", list_read(GROUP, RESOURCE, None)))?;
    Ok(computed(&nodes, &policies))
}

fn target(namespace: &str) -> ObjectTarget {
    ObjectTarget {
        api_version: format!("{GROUP}/v1"),
        kind: "NetworkPolicy".to_owned(),
        plural: RESOURCE.to_owned(),
        namespace: namespace.to_owned(),
        name: BASELINE.to_owned(),
    }
}

fn yaml(object: &Value) -> Option<String> {
    zeabur_graph::manifest::to_yaml(&zeabur_graph::manifest::clean(object)).ok()
}

fn plan_key(state: &AppState) -> Result<std::sync::Arc<token::PlanKey>, ApiError> {
    state.plan_key.clone().ok_or_else(|| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "Zeabur could not make a token key when it started, so it cannot plan changes.",
        ))
    })
}

/// Works out how the baseline network policies change to let every node in, and a token to apply
/// it; writes nothing.
#[utoipa::path(post, path = "/api/v1/cluster/node-policies/plan", tag = "cluster",
    operation_id = "planNodePolicies",
    responses((status = 200, body = NodePoliciesPlan),
              (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn plan(
    State(state): State<AppState>,
    Person(cluster): Person,
    Everyday(everyday): Everyday,
) -> Result<Json<NodePoliciesPlan>, ApiError> {
    let sealing = plan_key(&state)?;
    let identity = everyday
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let computed = compute(cluster.as_ref()).await?;
    let now = (state.clock)().as_second();
    let expires_at = now.saturating_add(LIFETIME_SECONDS);
    let addresses: Vec<String> = computed.addresses.iter().cloned().collect();
    let claims = Claims {
        person: identity.username,
        groups: identity.groups,
        addresses: addresses.clone(),
        versions: computed.versions.clone(),
        expires_at,
    };
    let token = token::sign_as(&sealing, PREFIX, &claims).map_err(|_random| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "The system random number generator failed, so Zeabur could not seal the plan.",
        ))
    })?;
    Ok(Json(NodePoliciesPlan {
        token,
        expires_at: Timestamp::from_second(expires_at)
            .unwrap_or(Timestamp::UNIX_EPOCH)
            .to_string(),
        addresses,
        writes: computed
            .rewrites
            .iter()
            .map(|r| PolicyWrite {
                namespace: r.namespace.clone(),
                name: BASELINE.to_owned(),
                before_yaml: yaml(&r.before),
                after_yaml: yaml(&r.after),
            })
            .collect(),
    }))
}

async fn send(cluster: &dyn Cluster, rewrite: &Rewrite, dry_run: bool) -> Result<(), ApiError> {
    let write = WriteRequest {
        target: target(&rewrite.namespace),
        body: WriteBody::JsonPatch(Value::Array(rewrite.operations.clone())),
        dry_run,
    };
    cluster.write(&write).await.map(|_| ()).map_err(|error| {
        read_problem(
            error,
            &format!("NetworkPolicy {BASELINE} in {}", rewrite.namespace),
            patch_right(&rewrite.namespace),
        )
    })
}

/// Carries out a plan by its token: checks it, dry-runs every write, then writes.
#[utoipa::path(post, path = "/api/v1/cluster/node-policies/apply", tag = "cluster",
    operation_id = "applyNodePolicies", request_body = NodePoliciesApply,
    responses((status = 200, body = NodePoliciesApplied),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn apply(
    State(state): State<AppState>,
    Person(cluster): Person,
    Everyday(everyday): Everyday,
    Json(request): Json<NodePoliciesApply>,
) -> Result<Json<NodePoliciesApplied>, ApiError> {
    let sealing = plan_key(&state)?;
    let claims: Claims = token::open_as(&sealing, PREFIX, &request.token).map_err(token_problem)?;
    if (state.clock)().as_second() >= claims.expires_at {
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
    let computed = compute(cluster.as_ref()).await?;
    let addresses: Vec<String> = computed.addresses.iter().cloned().collect();
    if addresses != claims.addresses || computed.versions != claims.versions {
        return Err(ApiError::from(Problem::new(
            Kind::Conflict,
            "The nodes or the network policies changed after the plan was made.",
        )));
    }
    for rewrite in &computed.rewrites {
        send(cluster.as_ref(), rewrite, true).await?;
    }
    for rewrite in &computed.rewrites {
        send(cluster.as_ref(), rewrite, false).await?;
    }
    Ok(Json(NodePoliciesApplied {
        namespaces: computed.rewrites.into_iter().map(|r| r.namespace).collect(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(name: &str, ip: &str, ranges: &[&str]) -> Node {
        serde_json::from_value(json!({
            "metadata": {"name": name},
            "spec": {"podCIDRs": ranges},
            "status": {"addresses": [{"type": "ExternalIP", "address": "203.0.113.9"},
                                     {"type": "InternalIP", "address": ip},
                                     {"type": "Hostname", "address": name}]}
        }))
        .unwrap()
    }

    /// A policy as `zeabur-installer apply` writes it, with these blocks.
    fn policy(namespace: &str, cidrs: &[&str]) -> Value {
        let mut from = vec![
            json!({"podSelector": {}}),
            json!({"namespaceSelector": {"matchLabels": {MANAGED_BY: INSTALLER}}}),
        ];
        from.extend(cidrs.iter().map(|c| json!({"ipBlock": {"cidr": c}})));
        json!({
            "apiVersion": "networking.k8s.io/v1",
            "kind": "NetworkPolicy",
            "metadata": {"name": BASELINE, "namespace": namespace, "resourceVersion": "7",
                         "labels": {MANAGED_BY: INSTALLER}},
            "spec": {"podSelector": {}, "policyTypes": ["Ingress"], "ingress": [{"from": from}]},
        })
    }

    fn nodes() -> Vec<Node> {
        vec![
            node("server-1", "10.0.0.5", &["10.42.0.0/24", "fd42::/64"]),
            node("worker-1", "10.0.0.6", &["10.42.1.0/24"]),
        ]
    }

    #[test]
    fn node_addresses_are_internal_ips_and_the_first_address_of_each_pod_range_sorted() {
        let all: Vec<String> = all_addresses(&nodes()).into_iter().collect();
        assert_eq!(
            all,
            ["10.0.0.5", "10.0.0.6", "10.42.0.0", "10.42.1.0", "fd42::"]
        );
    }

    #[test]
    fn the_blocks_are_rewritten_as_node_sync_writes_them_and_nothing_else_changes() {
        let mut before = policy("observability", &["10.0.0.5/32", "10.42.0.0/32"]);
        before["spec"]["egress"] = json!([{"to": [{"podSelector": {}}]}]);
        before["metadata"]["annotations"] = json!({"note": "kept"});
        let (operations, after) = rewrite(&before, &all_addresses(&nodes())).unwrap();
        assert_eq!(
            operations[0],
            json!({"op": "test", "path": "/metadata/resourceVersion", "value": "7"})
        );
        let expected = json!([
            {"podSelector": {}},
            {"namespaceSelector": {"matchLabels": {MANAGED_BY: INSTALLER}}},
            {"ipBlock": {"cidr": "10.0.0.5/32"}},
            {"ipBlock": {"cidr": "10.0.0.6/32"}},
            {"ipBlock": {"cidr": "10.42.0.0/32"}},
            {"ipBlock": {"cidr": "10.42.1.0/32"}},
            {"ipBlock": {"cidr": "fd42::/128"}},
        ]);
        assert_eq!(
            operations[1],
            json!({"op": "add", "path": "/spec/ingress/0/from", "value": expected})
        );
        assert_eq!(after["spec"]["ingress"][0]["from"], expected);
        let mut unchanged = after;
        unchanged["spec"]["ingress"][0]["from"] = before["spec"]["ingress"][0]["from"].clone();
        assert_eq!(unchanged, before);
    }

    #[test]
    fn a_current_policy_is_left_alone_and_a_stale_block_is_removed() {
        let current = policy(
            "zeabur",
            &[
                "10.0.0.5/32",
                "10.0.0.6/32",
                "10.42.0.0/32",
                "10.42.1.0/32",
                "fd42::/128",
            ],
        );
        assert!(rewrite(&current, &all_addresses(&nodes())).is_none());
        let with_gone_node = policy(
            "zeabur",
            &[
                "10.0.0.5/32",
                "10.0.0.6/32",
                "10.0.0.9/32",
                "10.42.0.0/32",
                "10.42.1.0/32",
                "fd42::/128",
            ],
        );
        let (_, after) = rewrite(&with_gone_node, &all_addresses(&nodes())).unwrap();
        assert!(!after.to_string().contains("10.0.0.9"));
    }

    #[test]
    fn only_the_installers_baseline_policies_count() {
        let mut other = policy("shop", &[]);
        other["metadata"]["name"] = json!("allow-web");
        let mut unlabelled = policy("lab", &[]);
        unlabelled["metadata"]["labels"] = json!({});
        let behind = policy("zeabur", &["10.0.0.5/32", "10.42.0.0/32", "fd42::/128"]);
        let (policies, nodes_behind) = compare(&nodes(), &[other, unlabelled, behind]);
        assert_eq!(
            policies,
            [BaselinePolicy {
                namespace: "zeabur".to_owned(),
                missing: vec!["10.0.0.6".to_owned(), "10.42.1.0".to_owned()],
                current: false,
            }]
        );
        assert_eq!(
            nodes_behind,
            [NodeBehind {
                name: "worker-1".to_owned(),
                addresses: vec!["10.0.0.6".to_owned(), "10.42.1.0".to_owned()],
            }]
        );
    }

    #[test]
    fn a_policy_without_an_ingress_rule_gets_one() {
        let mut bare = policy("zeabur", &[]);
        bare["spec"]["ingress"] = json!([]);
        let (operations, after) = rewrite(&bare, &all_addresses(&nodes())).unwrap();
        assert_eq!(operations[1]["path"], "/spec/ingress");
        assert_eq!(
            after["spec"]["ingress"][0]["from"][0]["ipBlock"]["cidr"],
            "10.0.0.5/32"
        );
    }
}
