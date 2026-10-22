//! The cluster: its version, nodes and capacity, upgrades, events and installed capabilities.
//!
//! # One section, one endpoint, one permission
//!
//! The cluster overview is made of sections that load on their own, so a person who may read
//! some of the cluster still sees those parts. Each endpoint reads only what its section needs,
//! and a refusal of its main read is its own `403`, naming the rule to ask for:
//!
//! | Endpoint | Reads (as the person) | Needs |
//! |---|---|---|
//! | `GET /cluster` | `/version` | nothing: every signed-in person (`system:public-info-viewer`) |
//! | `GET /cluster/upgrades` | `/version`; nodes and Plans when allowed | nothing; nodes and Plans add skew and progress |
//! | `GET /cluster/nodes` | nodes; pods, node metrics and the metrics store when allowed | `list nodes`; `list pods` (all namespaces) for requests; `list nodes.metrics.k8s.io` for usage; `get` on the metrics store's `services/proxy` for disks |
//! | `GET /cluster/health` | `/readyz?verbose`; the scheduler's and controller manager's leases when allowed | nothing: every signed-in person (`system:public-info-viewer`); `get leases.coordination.k8s.io` in `kube-system` adds the leases |
//! | `GET /cluster/events` | `events.k8s.io` events, all namespaces (`scope=cluster` keeps those about the cluster) | `list events.events.k8s.io` |
//! | `GET /cluster/alerts` | Falco's `ClusterPolicyReport`s (`zeabur_modules::falco`) | `list clusterpolicyreports.wgpolicyk8s.io`; refused or not installed is a state, not an error |
//! | `GET /cluster/config-reload` | Deployments' metadata, all namespaces; Reloader's Deployment | nothing: `unknown` when refused (see [`config_reload`]) |
//! | `GET /cluster/capabilities` | CRDs, APIServices, classes, `HelmChart`s, Helm release Secrets' labels, workloads | all of them: in practice a cluster administrator |
//!
//! The page shows the cluster itself, not its projects: Home lists those from `GET /projects`
//! (`list namespaces`). One ClusterRole covers the cluster page and Home except the components:
//!
//! ```yaml
//! apiVersion: rbac.authorization.k8s.io/v1
//! kind: ClusterRole
//! metadata: {name: zeabur-cluster-overview}
//! rules:
//!   - apiGroups: [""]
//!     resources: [nodes, namespaces, pods]
//!     verbs: [list]
//!   - apiGroups: [events.k8s.io]
//!     resources: [events]
//!     verbs: [list]
//!   - apiGroups: [metrics.k8s.io]
//!     resources: [nodes]
//!     verbs: [list]
//!   - apiGroups: [upgrade.cattle.io]
//!     resources: [plans]
//!     verbs: [list]
//! ```
//!
//! Disk use also needs the metrics store, which a Role in its own namespace grants (`get` on
//! `services/proxy`, see `crate::services::history` for who should have it).
//!
//! # Bounded reads
//!
//! The cluster-wide pod and event lists stop at 5,000 items and say so (`podScan: partial`,
//! `partial: true`), so a very large cluster gives a partial answer quickly rather than a slow
//! one (see `zeabur_kube`'s cluster-wide reads).

pub(crate) mod capacity;
pub mod config_reload;
pub(crate) mod disks;
pub(crate) mod health;
pub mod install;
pub mod modules;

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{RawQuery, State};
use axum::http::HeaderMap;
use capacity::Amount;
use zeabur_catalog::versions::{self, Inputs, PlanStatus};
use zeabur_core::problem::{Denied, Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::api::core::v1::Node;
use zeabur_kube::k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{ClusterError, ClusterExt as _, DynamicObject};
use zeabur_model::Origin;
use zeabur_modules::Reading;

use crate::error::{ApiError, list_read, list_read_of_key};
use crate::handlers::{AppState, Person};
use crate::projects::events::{EventKey, event_key, event_view, last_seen, newest_first};
use crate::query::{EventQuery, LimitParams};
use crate::settings::cluster_name;
use crate::views::{
    CapabilityList, Cluster, ClusterEventList, ClusterHealth, DiskUsage, MetricsState,
    NodeCondition, NodeList, NodeTotals, NodeView, PodScan, Resources, Taint, UnreadableView,
    UpgradeExtensions, Upgrades,
};

/// The most pods or events a cluster-wide read takes.
pub(crate) const WIDE_LIMIT: usize = 5_000;

/// The distribution a kube-apiserver version names (`v1.36.5+k3s1`), as the API names it.
fn distribution(version: &str) -> Option<String> {
    if version.contains("+k3s") {
        Some("k3s".to_owned())
    } else if version.contains("+rke2") {
        Some("rke2".to_owned())
    } else {
        None
    }
}

/// A forbidden answer told as the rule to ask for, naming the refused `read`; other problems as
/// they are.
fn refused(error: ClusterError, what: &str, rule: &str, read: Denied) -> ApiError {
    let mut problem = error.into_problem(what);
    if problem.kind == Kind::Forbidden {
        problem.deny(read);
        format!("You do not have permission to read {what}.").clone_into(&mut problem.detail);
        problem.fix_for_occasion(
            format!(
                "Ask a cluster administrator for {rule}, for example with a ClusterRole granting \
                 it and a ClusterRoleBinding to you."
            ),
            zeabur_core::message::Message::new("problem.forbidden.fix.askForRule")
                .with("rule", rule),
        );
    }
    ApiError::from(problem)
}

/// A refusal of an optional read as an unreadable entry; any other error as it is.
fn optional<T>(
    result: Result<T, ClusterError>,
    resource: &str,
    unreadable: &mut Vec<UnreadableView>,
) -> Result<Option<T>, ClusterError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) => {
            unreadable.push(error.into_unreadable(resource)?.into());
            Ok(None)
        }
    }
}

/// The cluster: its name and where that comes from, kube-apiserver's version and distribution.
/// Readable by every signed-in person; the name may come from the nodes when they may list them.
/// `can` is one `SelfSubjectRulesReview` in `kube-system`.
#[utoipa::path(get, path = "/api/v1/cluster", tag = "cluster", operation_id = "getCluster",
    responses((status = 200, body = Cluster), (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    State(state): State<AppState>,
    Person(cluster): Person,
    headers: HeaderMap,
) -> Result<Json<Cluster>, ApiError> {
    let (version, rules) =
        futures_util::future::join(cluster.version(), cluster.access_rules("kube-system")).await;
    let version = version.map_err(|e| ApiError::from(e.into_problem("the cluster's version")))?;
    let named = cluster_name::resolve(cluster.as_ref(), &state.settings, &headers).await;
    Ok(Json(Cluster {
        name: named.name,
        name_source: named.source,
        name_provider: named.provider,
        distribution: distribution(&version),
        kubernetes_version: version,
        domains: state.settings.domains.clone(),
        can: crate::projects::rights::ClusterRights::of(rules.as_ref().ok()),
    }))
}

/// The API server's readiness checks, of the replica that answered, with only the failing ones
/// named, and the scheduler's and controller manager's leader leases when the person may read
/// them (see [`mod@health`]). `403` when `/readyz` is refused.
#[utoipa::path(get, path = "/api/v1/cluster/health", tag = "cluster", operation_id = "getClusterHealth",
    responses((status = 200, body = ClusterHealth), (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn health(
    State(state): State<AppState>,
    Person(cluster): Person,
) -> Result<Json<ClusterHealth>, ApiError> {
    Ok(Json(health::read(cluster.as_ref(), (state.clock)()).await?))
}

/// Which capabilities the cluster has, by Zeabur Catalog, with the version of
/// each installed component.
///
/// Detection reads cluster-wide objects (CRDs, ingress classes, workloads in every namespace) as
/// the person. If any of those reads is refused, the answer would show installed components as
/// missing, so the endpoint answers `403` instead: managing the cluster's components is for
/// people who can see the whole cluster, and the interface hides it for everyone else.
#[utoipa::path(get, path = "/api/v1/cluster/capabilities", tag = "cluster", operation_id = "listCapabilities",
    responses((status = 200, body = CapabilityList), (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn capabilities(
    State(state): State<AppState>,
    Person(cluster): Person,
) -> Result<Json<CapabilityList>, ApiError> {
    let facts = cluster
        .cluster_facts()
        .await
        .map_err(|e| ApiError::from(e.into_problem("the cluster's installed components")))?;
    if let Some(refused) = facts.unreadable.first() {
        let mut problem = Problem {
            kubernetes: Some(refused.status.clone()),
            ..Problem::new(
                Kind::Forbidden,
                "Managing the cluster's components needs cluster administrator rights: Zeabur \
                 reads the whole cluster to see what is installed.",
            )
        };
        problem.deny(list_read_of_key(&refused.resource));
        return Err(ApiError::from(problem));
    }
    let mut capabilities = zeabur_catalog::detect(&state.catalogue, &facts);
    capabilities.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(CapabilityList {
        catalogue_commit: zeabur_catalog::CATALOGUE_COMMIT.to_owned(),
        items: capabilities,
    }))
}

fn quantity(map: Option<&BTreeMap<String, Quantity>>, key: &str) -> Option<String> {
    map.and_then(|m| m.get(key)).map(|q| q.0.clone())
}

fn resources(map: Option<&BTreeMap<String, Quantity>>) -> Resources {
    Resources {
        cpu_millicores: quantity(map, "cpu")
            .and_then(|q| capacity::millicores(&q))
            .unwrap_or(0),
        memory_bytes: quantity(map, "memory")
            .and_then(|q| capacity::bytes(&q))
            .unwrap_or(0),
        pods: quantity(map, "pods").and_then(|q| q.trim().parse().ok()),
    }
}

/// Roles from `node-role.kubernetes.io/<role>` label keys and the legacy `kubernetes.io/role`
/// value, as `kubectl get nodes` prints them [k8s-printers]; `master` is the old name of
/// `control-plane`.
///
/// [k8s-printers]: https://github.com/kubernetes/kubernetes/blob/master/pkg/printers/internalversion/printers.go
fn roles(labels: Option<&BTreeMap<String, String>>) -> Vec<String> {
    let mut roles: Vec<String> = labels
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| {
            key.strip_prefix("node-role.kubernetes.io/")
                .filter(|r| !r.is_empty())
                .map(str::to_owned)
                .or_else(|| (key == "kubernetes.io/role").then(|| value.clone()))
        })
        .map(|r| {
            if r == "master" {
                "control-plane".to_owned()
            } else {
                r
            }
        })
        .collect();
    roles.sort();
    roles.dedup();
    roles
}

/// A node, without the pods' sums.
fn node_view(node: &Node) -> NodeView {
    let name = node.metadata.name.clone().unwrap_or_default();
    let status = node.status.as_ref();
    let conditions = status.and_then(|s| s.conditions.as_ref());
    let ready = conditions
        .and_then(|c| c.iter().find(|c| c.type_ == "Ready"))
        .map_or_else(|| "Unknown".to_owned(), |c| c.status.clone());
    // Ready must be True; the pressures and NetworkUnavailable must not be
    // (https://kubernetes.io/docs/reference/node/node-status/).
    let problems = conditions
        .into_iter()
        .flatten()
        .filter(|c| {
            if c.type_ == "Ready" {
                c.status != "True"
            } else {
                matches!(
                    c.type_.as_str(),
                    "MemoryPressure" | "DiskPressure" | "PIDPressure" | "NetworkUnavailable"
                ) && c.status == "True"
            }
        })
        .map(|c| NodeCondition {
            r#type: c.type_.clone(),
            status: c.status.clone(),
            reason: c.reason.clone(),
            message: c.message.clone(),
        })
        .collect();
    let info = status.and_then(|s| s.node_info.as_ref());
    let field = |pick: fn(&zeabur_kube::k8s_openapi::api::core::v1::NodeSystemInfo) -> &String| {
        info.map_or_default(|i| pick(i).clone())
    };
    let capacity_map = status.and_then(|s| s.capacity.as_ref());
    let allocatable_map = status.and_then(|s| s.allocatable.as_ref()).or(capacity_map);
    let spec = node.spec.as_ref();
    NodeView {
        id: format!("cluster/nodes/{name}"),
        origin: Origin::complete("v1", "Node", None, &name),
        name,
        roles: roles(node.metadata.labels.as_ref()),
        ready,
        problems,
        cordoned: spec.and_then(|s| s.unschedulable).unwrap_or(false),
        taints: spec
            .and_then(|s| s.taints.as_ref())
            .into_iter()
            .flatten()
            .map(|t| Taint {
                key: t.key.clone(),
                value: t.value.clone().filter(|v| !v.is_empty()),
                effect: t.effect.clone(),
            })
            .collect(),
        kubelet_version: field(|i| &i.kubelet_version),
        os_image: field(|i| &i.os_image),
        kernel_version: field(|i| &i.kernel_version),
        architecture: field(|i| &i.architecture),
        container_runtime: field(|i| &i.container_runtime_version),
        allocatable: resources(allocatable_map),
        capacity: resources(capacity_map),
        requests: None,
        limits: None,
        usage: None,
        disk: None,
        created: node
            .metadata
            .creation_timestamp
            .as_ref()
            .map(|t| t.0.to_string()),
    }
}

const fn plus(a: Option<Resources>, b: Option<Resources>) -> Option<Resources> {
    match (a, b) {
        (Some(a), Some(b)) => Some(Resources {
            cpu_millicores: a.cpu_millicores.saturating_add(b.cpu_millicores),
            memory_bytes: a.memory_bytes.saturating_add(b.memory_bytes),
            pods: match (a.pods, b.pods) {
                (Some(x), Some(y)) => Some(x.saturating_add(y)),
                (x, None) => x,
                (None, y) => y,
            },
        }),
        (x, None) => x,
        (None, y) => y,
    }
}

const fn disk_plus(a: Option<DiskUsage>, b: Option<DiskUsage>) -> Option<DiskUsage> {
    match (a, b) {
        (Some(a), Some(b)) => Some(DiskUsage {
            capacity_bytes: a.capacity_bytes.saturating_add(b.capacity_bytes),
            used_bytes: a.used_bytes.saturating_add(b.used_bytes),
            available_bytes: a.available_bytes.saturating_add(b.available_bytes),
        }),
        (x, None) => x,
        (None, y) => y,
    }
}

/// Each node's usage from metrics-server's quantities.
fn usage_by_node(usage: Vec<(String, String, String)>) -> BTreeMap<String, Resources> {
    usage
        .into_iter()
        .map(|(node, cpu, memory)| {
            (
                node,
                Resources {
                    cpu_millicores: capacity::millicores(&cpu).unwrap_or(0),
                    memory_bytes: capacity::bytes(&memory).unwrap_or(0),
                    pods: None,
                },
            )
        })
        .collect()
}

/// metrics-server's node metrics, through its module.
async fn node_metrics(
    cluster: &dyn zeabur_kube::Cluster,
) -> Result<Reading<UnreadableView>, ClusterError> {
    modules::read(cluster, &zeabur_modules::metrics_server::NODE_METRICS).await
}

/// Each node's usage as `(node, cpu, memory)` quantities.
fn node_usage(objects: &[DynamicObject]) -> Vec<(String, String, String)> {
    zeabur_modules::metrics_server::node_usage(objects)
        .into_iter()
        .map(|u| (u.node, u.cpu, u.memory))
        .collect()
}

/// The nodes: roles, health, versions, and what their pods request against what is allocatable.
///
/// Requests and limits are summed per node from every unfinished pod, the way
/// `kubectl describe node` sums them (see `capacity`); usage is added only when metrics-server
/// is installed and readable, and is approximate ("meant only for autoscaling purposes"
/// [metrics-server]). Each node's disk comes from the metrics store (see `disks`), and `disk`
/// says when it could not. `403` when nodes cannot be listed; a refusal of pods, metrics or the
/// store leaves those parts out and says so.
///
/// [metrics-server]: https://github.com/kubernetes-sigs/metrics-server
#[utoipa::path(get, path = "/api/v1/cluster/nodes", tag = "cluster", operation_id = "listNodes",
    responses((status = 200, body = NodeList), (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn nodes(
    State(state): State<AppState>,
    Person(cluster): Person,
) -> Result<Json<NodeList>, ApiError> {
    let (nodes, pods, usage, asked) = futures_util::join!(
        cluster.list::<Node>(None),
        cluster.cluster_pods(WIDE_LIMIT),
        node_metrics(cluster.as_ref()),
        disks::ask(cluster.as_ref(), &state.settings, disks::NODE_FILESYSTEMS)
    );
    let nodes = nodes.map_err(|e| {
        refused(
            e,
            "the cluster's nodes",
            "`list` on `nodes` (cluster-wide)",
            list_read("", "nodes", None),
        )
    })?;
    let mut unreadable = Vec::new();
    let pods = optional(pods, "pods", &mut unreadable)
        .map_err(|e| ApiError::from(e.into_problem("the cluster's pods")))?;
    let (usage, metrics) = match usage {
        Ok(Reading::Objects(objects)) => (Some(node_usage(&objects)), MetricsState::Available),
        Ok(Reading::NotInstalled) => (None, MetricsState::NotInstalled),
        Ok(Reading::Refused(refusal)) => {
            unreadable.push(refusal);
            (None, MetricsState::Unreadable)
        }
        Err(_) => (None, MetricsState::Unavailable),
    };

    let mut sums: BTreeMap<String, (Amount, Amount, u64)> = BTreeMap::new();
    let mut unscheduled = 0;
    if let Some(pods) = &pods {
        for pod in &pods.pods {
            let Some(node) = &pod.node_name else {
                unscheduled += 1;
                continue;
            };
            let (requests, limits) = capacity::pod_amounts(pod);
            let entry = sums.entry(node.clone()).or_default();
            entry.0.add(requests);
            entry.1.add(limits);
            entry.2 += 1;
        }
    }
    let usage = usage.map(usage_by_node);

    let mut items: Vec<NodeView> = nodes.iter().map(node_view).collect();
    let disk = asked.disks(|samples| disks::node_disks(samples, &nodes));
    unreadable.extend(asked.unreadable);
    let mut totals = NodeTotals::default();
    for item in &mut items {
        item.disk = disk.disks.get(&item.name).copied();
        totals.disk = disk_plus(totals.disk, item.disk);
        if pods.is_some() {
            let (requests, limits, count) = sums.get(&item.name).copied().unwrap_or_default();
            item.requests = Some(Resources {
                cpu_millicores: requests.cpu,
                memory_bytes: requests.memory,
                pods: Some(count),
            });
            item.limits = Some(Resources {
                cpu_millicores: limits.cpu,
                memory_bytes: limits.memory,
                pods: None,
            });
        }
        item.usage = usage.as_ref().and_then(|u| u.get(&item.name).copied());
        totals.allocatable =
            plus(Some(totals.allocatable), Some(item.allocatable)).unwrap_or_default();
        totals.requests = plus(totals.requests, item.requests);
        totals.limits = plus(totals.limits, item.limits);
        totals.usage = plus(totals.usage, item.usage);
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    let pod_scan = match &pods {
        None => PodScan::Unreadable,
        Some(p) if p.truncated => PodScan::Partial,
        Some(_) => PodScan::Complete,
    };
    let pods_scanned = pods.as_ref().map_or(0, |p| p.pods.len());
    Ok(Json(NodeList {
        items,
        totals,
        pod_scan,
        pods_scanned,
        unscheduled_pods: unscheduled,
        metrics,
        disk: disk.state,
        unreadable,
    }))
}

/// The day of `now` in UTC, `YYYY-MM-DD`.
fn day(now: Timestamp) -> String {
    now.to_string().get(..10).unwrap_or_default().to_owned()
}

/// The cluster's version and what it calls for: end of life from the bundled table, version
/// skew between kube-apiserver and the kubelets, and newer patches and minors from
/// system-upgrade-controller's Plans.
///
/// Zeabur makes no outbound request: without Plans, newer versions are not reported (see
/// [`zeabur_catalog::versions`]). Readable by everyone signed in; nodes and Plans the person
/// may not list are named in `unreadable`, so the interface can say which permission is missing.
#[utoipa::path(get, path = "/api/v1/cluster/upgrades", tag = "cluster", operation_id = "getUpgrades",
    responses((status = 200, body = Upgrades), (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn upgrades(
    State(state): State<AppState>,
    Person(cluster): Person,
) -> Result<Json<Upgrades>, ApiError> {
    let (version, nodes, plans) = futures_util::join!(
        cluster.version(),
        cluster.list::<Node>(None),
        upgrade_plans(cluster.as_ref())
    );
    let version = version.map_err(|e| ApiError::from(e.into_problem("the cluster's version")))?;
    let mut unreadable = Vec::new();
    let nodes = optional(nodes, "nodes", &mut unreadable)
        .map_err(|e| ApiError::from(e.into_problem("the cluster's nodes")))?
        .unwrap_or_default();
    let mut extensions = UpgradeExtensions::default();
    // Plans refused: `unreadable` names the missing permission. Not installed (no CRD): none.
    let statuses =
        match plans.map_err(|e| ApiError::from(e.into_problem("the cluster's upgrade plans")))? {
            Reading::Objects(objects) => plan_statuses(&objects, &mut extensions),
            Reading::NotInstalled => Vec::new(),
            Reading::Refused(refusal) => {
                unreadable.push(refusal);
                Vec::new()
            }
        };
    let kubelets: Vec<(String, String)> = nodes
        .iter()
        .filter_map(|n| {
            Some((
                n.metadata.name.clone()?,
                n.status
                    .as_ref()?
                    .node_info
                    .as_ref()?
                    .kubelet_version
                    .clone(),
            ))
        })
        .collect();
    let today = day((state.clock)());
    let assessment = versions::assess(&Inputs {
        current: &version,
        today: &today,
        kubelets: &kubelets,
        plans: &statuses,
    });
    Ok(Json(Upgrades {
        id: "cluster/upgrades".to_owned(),
        distribution: distribution(&version),
        kubernetes_version: version,
        minor: assessment.minor,
        release: assessment.release,
        data_as_of: versions::DATA_AS_OF.to_owned(),
        severity: assessment
            .states
            .iter()
            .map(|s| s.severity())
            .max()
            .unwrap_or(versions::Severity::None),
        states: assessment.states,
        patch: assessment.patch,
        next_minor: assessment.next_minor,
        skew: assessment.skew,
        upgrading: assessment.upgrading,
        unreadable,
        extensions,
    }))
}

/// system-upgrade-controller's Plans, through its module.
async fn upgrade_plans(
    cluster: &dyn zeabur_kube::Cluster,
) -> Result<Reading<UnreadableView>, ClusterError> {
    modules::read(cluster, &zeabur_modules::system_upgrade_controller::PLANS).await
}

/// The Plans as the version assessment reads them, and as the module's extension.
fn plan_statuses(objects: &[DynamicObject], extensions: &mut UpgradeExtensions) -> Vec<PlanStatus> {
    let plans = zeabur_modules::system_upgrade_controller::plans(objects);
    let statuses = plans
        .iter()
        .map(|p| PlanStatus {
            latest_version: p.latest_version.clone(),
            applying: p.applying.clone(),
        })
        .collect();
    extensions.system_upgrade_controller =
        Some(zeabur_modules::system_upgrade_controller::Upgrades { plans });
    statuses
}

/// The parameters of the cluster's events (for the description; parsed by [`EventQuery`]).
#[expect(
    dead_code,
    reason = "describes the parameters in the OpenAPI document; parsed by hand"
)]
#[derive(Debug, Default, utoipa::IntoParams)]
#[into_params(parameter_in = Query, rename_all = "camelCase")]
pub(crate) struct ClusterEventParams {
    /// Only events of this type: `Normal` or `Warning` (any case).
    r#type: Option<String>,
    /// Only events that last happened within this many seconds.
    since_seconds: Option<i64>,
    /// `cluster`: only events about the cluster itself (no namespace, a Node, or a `kube-`
    /// namespace); `all`, the default: every event.
    scope: Option<String>,
}

/// Whether an event is about the cluster itself rather than a project: it has no namespace, it
/// regards a Node, or it lives in a `kube-` namespace (`kube-system`, `kube-public`,
/// `kube-node-lease`), where Kubernetes and the distribution run their own components.
fn about_the_cluster(event: &zeabur_kube::k8s_openapi::api::events::v1::Event) -> bool {
    let namespace = event.metadata.namespace.as_deref().unwrap_or_default();
    namespace.is_empty()
        || namespace.starts_with("kube-")
        || event
            .regarding
            .as_ref()
            .and_then(|r| r.kind.as_deref())
            .is_some_and(|kind| kind == "Node")
}

/// Events of every namespace, the newest `limit` of them (100 by default), optionally only
/// warnings, only recent ones and only those about the cluster itself: the cluster overview asks
/// for the last hour's warnings with `scope=cluster`, leaving projects' events to their pages.
///
/// At most 5,000 events are read (`partial` says when there were more); `totalSize` counts every
/// event read that matched, also those past the limit.
#[utoipa::path(get, path = "/api/v1/cluster/events", tag = "cluster", operation_id = "listClusterEvents",
    params(ClusterEventParams, LimitParams),
    responses((status = 200, body = ClusterEventList), (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn events(
    Person(cluster): Person,
    RawQuery(query): RawQuery,
) -> Result<Json<ClusterEventList>, ApiError> {
    let query = EventQuery::parse(query.as_deref(), &["type", "sinceSeconds", "scope"])?;
    let kind = query.filters.get("type").map(|t| t.to_ascii_lowercase());
    let cluster_only = match query.filters.get("scope").map(String::as_str) {
        None | Some("all") => false,
        Some("cluster") => true,
        Some(other) => {
            return Err(ApiError::from(Problem::new(
                Kind::BadRequest,
                format!("`scope` must be `cluster` or `all`, not {other:?}."),
            )));
        }
    };
    let since = match query.filters.get("sinceSeconds") {
        None => None,
        Some(text) => Some(text.parse::<i64>().ok().filter(|&s| s > 0).ok_or_else(|| {
            ApiError::from(Problem::new(
                Kind::BadRequest,
                format!("`sinceSeconds` must be a positive whole number, not {text:?}."),
            ))
        })?),
    };
    let read = cluster.cluster_events(WIDE_LIMIT).await.map_err(|e| {
        refused(
            e,
            "the cluster's events",
            "`list` on `events` in the `events.k8s.io` group (cluster-wide)",
            list_read("events.k8s.io", "events", None),
        )
    })?;
    let cutoff = since.map(|s| Timestamp::now().as_second().saturating_sub(s));
    let mut keyed: Vec<(EventKey, crate::views::EventView)> = read
        .events
        .iter()
        .filter(|e| {
            kind.as_ref().is_none_or(|k| {
                e.type_
                    .as_deref()
                    .is_some_and(|t| t.eq_ignore_ascii_case(k))
            })
        })
        .filter(|e| {
            cutoff.is_none_or(|cutoff| last_seen(e).is_some_and(|t| t.as_second() >= cutoff))
        })
        .filter(|e| !cluster_only || about_the_cluster(e))
        .map(|e| {
            let namespace = e.metadata.namespace.as_deref().unwrap_or_default();
            let (time, name) = event_key(e);
            (
                (time, format!("{namespace}/{name}")),
                event_view(namespace, e),
            )
        })
        .collect();
    keyed.sort_by(|a, b| newest_first(&a.0, &b.0));
    let total_size = keyed.len();
    Ok(Json(ClusterEventList {
        items: keyed
            .into_iter()
            .take(query.limit)
            .map(|(_, view)| view)
            .collect(),
        total_size,
        partial: read.truncated,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_come_from_labels_and_master_is_control_plane() {
        let labels = BTreeMap::from([
            (
                "node-role.kubernetes.io/master".to_owned(),
                "true".to_owned(),
            ),
            ("node-role.kubernetes.io/etcd".to_owned(), "true".to_owned()),
            (
                "node-role.kubernetes.io/control-plane".to_owned(),
                String::new(),
            ),
            ("kubernetes.io/hostname".to_owned(), "a".to_owned()),
        ]);
        assert_eq!(roles(Some(&labels)), ["control-plane", "etcd"]);
        assert_eq!(roles(None), Vec::<String>::new());
    }
}
