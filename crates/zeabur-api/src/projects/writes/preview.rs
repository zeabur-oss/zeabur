//! What a plan does once applied, beyond the fields it writes: which workloads roll and whether
//! that leaves a moment with no pod, whether the new pods fit, what data goes, and which fields
//! it removes.
//!
//! # Rollouts
//!
//! A change to a workload's pod template replaces its pods; a change to `replicas` adds or removes
//! some. How pods are replaced is the workload's strategy [k8s-deployment-strategy]
//! [k8s-statefulset-updates] [k8s-daemonset-updates]:
//!
//! - a Deployment's `Recreate` stops every pod before starting new ones: a moment with none;
//! - a Deployment's `RollingUpdate` keeps `replicas - maxUnavailable` pods (25% each by default,
//!   `maxSurge` rounded up and `maxUnavailable` down) and adds up to `maxSurge` new ones first;
//!   with no surge and every pod allowed to be unavailable, there is a moment with none;
//! - a StatefulSet replaces one pod at a time, stopping it first: with one replica, a moment with
//!   none. With `OnDelete` the pods change only when they are deleted;
//! - a DaemonSet replaces the pod of each node, stopping it first unless `maxSurge` is set.
//!
//! A ConfigMap or Secret change reaches containers that read it into variables only when they
//! restart (the plan's `restartNeeded` warning); those workloads are listed as `pendingRestart`.
//!
//! # Fit
//!
//! The pods a plan adds (the surge of a rolling Deployment, new replicas) and the growth of each
//! replaced pod's requests are checked against the namespace's `ResourceQuotas` (`hard - used`)
//! [k8s-quota] and against what the ready, schedulable nodes have left (`allocatable` minus the
//! requests of their pods, summed as `kubectl describe node` sums them, see
//! `crate::cluster::capacity`) [k8s-scheduling]. Only requests are compared: node selectors,
//! affinity, taints and volume topology can still keep a pod off a node with room, so `fits`
//! means the requests fit, and a blocker is certain.
//!
//! # Data
//!
//! A deleted claim's data may go with it (the plan's `claimDataMayBeLost` warning names the reclaim
//! policy). A pod's `emptyDir` volumes live only as long as the pod [k8s-emptydir], so a workload
//! that rolls or is deleted loses them. A StatefulSet whose
//! `persistentVolumeClaimRetentionPolicy` says `Delete` takes its claims with it when it is
//! deleted or scaled down [k8s-statefulset-pvc-retention].
//!
//! # Removed fields
//!
//! Every field a plan removes is listed, with whether it was left out of an edited YAML (an edit
//! replaces the object, so what it leaves out goes) or asked for. A form's change removes only
//! what it names.
//!
//! [k8s-deployment-strategy]: https://kubernetes.io/docs/concepts/workloads/controllers/deployment/#strategy
//! [k8s-statefulset-updates]: https://kubernetes.io/docs/concepts/workloads/controllers/statefulset/#update-strategies
//! [k8s-daemonset-updates]: https://kubernetes.io/docs/tasks/manage-daemon/update-daemon-set/
//! [k8s-quota]: https://kubernetes.io/docs/concepts/policy/resource-quotas/
//! [k8s-scheduling]: https://kubernetes.io/docs/concepts/scheduling-eviction/kube-scheduler/
//! [k8s-emptydir]: https://kubernetes.io/docs/concepts/storage/volumes/#emptydir
//! [k8s-statefulset-pvc-retention]: https://kubernetes.io/docs/concepts/workloads/controllers/statefulset/#persistentvolumeclaim-retention

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_kube::k8s_openapi::api::core::v1::{Node, PodSpec, ResourceQuota};
use zeabur_kube::{Cluster, ClusterExt as _, ClusterPod, NamespaceObjects};
use zeabur_model::{WorkloadKind, WorkloadRef};
use zeabur_plan::{Change, ObjectName, Plan, PlanRequest, PlannedObject, Warning, Write};

use crate::cluster::WIDE_LIMIT;
use crate::cluster::capacity::{Amount, bytes, millicores, pod_amounts, spec_amounts};
use crate::error::ApiError;
use crate::incidents::allowed;
use crate::views::UnreadableView;

/// What happens to a workload's pods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum RolloutAction {
    /// Its pods are replaced: the pod template changes.
    Roll,
    /// Pods are added or removed: `replicas` changes.
    Scale,
    /// It is created: its pods start.
    Start,
    /// It is deleted: its pods stop.
    Stop,
    /// It reads a changed ConfigMap or Secret into variables and is not restarted: the running
    /// pods keep the old values until they restart.
    PendingRestart,
}

/// One workload whose pods a plan changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Rollout {
    /// The workload.
    pub workload: WorkloadRef,
    /// What happens to its pods.
    pub actions: Vec<RolloutAction>,
    /// Replicas before, for a scale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_replicas: Option<i32>,
    /// Replicas after.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicas: Option<i32>,
    /// `RollingUpdate`, `Recreate` or `OnDelete`, for a roll.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,
    /// There is a moment with no pod serving (on a node, for a DaemonSet).
    pub downtime: bool,
    /// The pod template fields that change, at most ten.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
}

/// Whether the plan's new pods fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum FitStatus {
    /// The plan adds no pod and grows none.
    NotNeeded,
    /// Their requests fit the quotas and the nodes' room.
    Fits,
    /// A quota or the nodes' room keeps at least one from being created or placed.
    Blocked,
    /// Quotas or nodes could not be read (see `unreadable`), or the read stopped early.
    Unknown,
}

/// What keeps a pod from being created or placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum BlockerKind {
    /// A `ResourceQuota` of the namespace: kube-apiserver refuses the pod.
    Quota,
    /// No ready, schedulable node has room: the pod stays `Pending`.
    Node,
}

/// One limit the new pods exceed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Blocker {
    /// A quota or the nodes.
    pub kind: BlockerKind,
    /// The quota's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The resource: `requests.cpu`, `limits.memory`, `pods`, `cpu`, `memory`, …
    pub resource: String,
    /// What the plan needs, in the resource's unit: millicores, bytes or pods.
    pub need: u64,
    /// What is left.
    pub available: u64,
}

/// Whether the plan's new pods fit, and what blocks them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Fit {
    /// The verdict.
    pub status: FitStatus,
    /// Pods the plan adds at its peak.
    pub pods: u64,
    /// CPU requests it adds at its peak, millicores.
    pub cpu_millicores: u64,
    /// Memory requests it adds at its peak, bytes.
    pub memory_bytes: u64,
    /// What blocks them, quotas first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<Blocker>,
}

/// What kind of data a plan loses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum DataLossKind {
    /// A deleted PersistentVolumeClaim; its volume follows its reclaim policy.
    ClaimDeleted,
    /// An `emptyDir` volume of pods that are replaced or stop.
    EmptyDir,
    /// Claims a StatefulSet deletes with itself or its removed replicas
    /// (`persistentVolumeClaimRetentionPolicy`).
    ClaimsOfStatefulSet,
}

/// Data a plan loses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DataLoss {
    /// What kind.
    pub kind: DataLossKind,
    /// The workload whose pods hold it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workload: Option<WorkloadRef>,
    /// The claim, or the StatefulSet's claim template.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim: Option<String>,
    /// The volume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<String>,
    /// Where the containers mount it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mount_paths: Vec<String>,
}

/// A field the plan removes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Removal {
    /// The object.
    pub target: ObjectName,
    /// The field.
    pub path: String,
    /// Its value now (redacted for a Secret).
    pub before: String,
    /// It was left out of an edited YAML, rather than asked to be removed.
    pub left_out_of_edit: bool,
}

/// A CloudNativePG `Cluster` a plan creates: its operator starts its instances.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseStart {
    /// The `Cluster`'s name.
    pub name: String,
    /// The engine, as people write it (`PostgreSQL`).
    pub engine: String,
    /// Its instances (`spec.instances`).
    pub instances: i32,
}

/// What a plan does once applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    /// The workloads whose pods change, sorted.
    pub rollouts: Vec<Rollout>,
    /// The databases it creates, by name. Their instances count in `fit`.
    pub databases: Vec<DatabaseStart>,
    /// The public domains its new Ingresses and `HTTPRoutes` route, sorted.
    pub domains: Vec<String>,
    /// Whether the new pods fit.
    pub fit: Fit,
    /// The data it loses.
    pub data_loss: Vec<DataLoss>,
    /// The fields it removes, by object then path.
    pub removals: Vec<Removal>,
    /// Reads the person was refused, and what they left unchecked.
    pub unreadable: Vec<UnreadableView>,
}

/// What the nodes and quotas give the fit, as far as they were read.
#[derive(Debug, Clone, Default)]
pub(crate) struct Room {
    /// Per ready, schedulable node: CPU, memory and pods left. `None` when nodes or pods could
    /// not be read, or the pod read stopped early.
    pub(crate) nodes: Option<Vec<(Amount, u64)>>,
    /// The namespace's quotas; `None` when they could not be read.
    pub(crate) quotas: Option<Vec<ResourceQuota>>,
}

/// The room on each ready, schedulable node: allocatable less its pods' requests.
pub(crate) fn node_room(nodes: &[Node], pods: &[ClusterPod]) -> Vec<(Amount, u64)> {
    let mut used: BTreeMap<&str, (Amount, u64)> = BTreeMap::new();
    for pod in pods {
        if matches!(pod.phase.as_deref(), Some("Succeeded" | "Failed")) {
            continue;
        }
        if let Some(node) = pod.node_name.as_deref() {
            let entry = used.entry(node).or_default();
            entry.0.add(pod_amounts(pod).0);
            entry.1 += 1;
        }
    }
    nodes
        .iter()
        .filter(|node| {
            let spec_ok = !node
                .spec
                .as_ref()
                .and_then(|s| s.unschedulable)
                .unwrap_or(false);
            let ready = node
                .status
                .as_ref()
                .and_then(|s| s.conditions.as_ref())
                .is_some_and(|c| c.iter().any(|c| c.type_ == "Ready" && c.status == "True"));
            spec_ok && ready
        })
        .map(|node| {
            let allocatable = node.status.as_ref().and_then(|s| s.allocatable.as_ref());
            let quantity = |key: &str| allocatable.and_then(|a| a.get(key)).map(|q| q.0.as_str());
            let (taken, count) = used
                .get(node.metadata.name.as_deref().unwrap_or_default())
                .copied()
                .unwrap_or_default();
            (
                Amount {
                    cpu: quantity("cpu")
                        .and_then(millicores)
                        .unwrap_or(0)
                        .saturating_sub(taken.cpu),
                    memory: quantity("memory")
                        .and_then(bytes)
                        .unwrap_or(0)
                        .saturating_sub(taken.memory),
                },
                quantity("pods")
                    .and_then(|p| p.parse::<u64>().ok())
                    .unwrap_or(110)
                    .saturating_sub(count),
            )
        })
        .collect()
}

/// The workload a planned object is, when it is one.
fn workload_of(object: &PlannedObject) -> Option<WorkloadRef> {
    let kind = WorkloadKind::parse(&object.target.kind)?;
    matches!(
        kind,
        WorkloadKind::Deployment | WorkloadKind::StatefulSet | WorkloadKind::DaemonSet
    )
    .then(|| WorkloadRef {
        kind,
        name: object.target.name.clone(),
    })
}

fn after_of(object: &PlannedObject) -> Option<&Value> {
    (!object.raw_after.is_null()).then_some(&object.raw_after)
}

fn replicas(object: Option<&Value>) -> Option<i32> {
    object?
        .pointer("/spec/replicas")
        .and_then(Value::as_i64)
        .and_then(|r| i32::try_from(r).ok())
}

/// A percentage or count of `replicas`, rounded up or down, as Kubernetes resolves
/// `maxSurge` and `maxUnavailable`.
fn resolve(value: Option<&Value>, default: &str, replicas: i32, up: bool) -> i32 {
    let text = value.map_or_else(
        || default.to_owned(),
        |v| v.as_str().map_or_else(|| v.to_string(), str::to_owned),
    );
    text.strip_suffix('%').map_or_else(
        || text.parse().unwrap_or(0),
        |percent| {
            let percent: i64 = percent.parse().unwrap_or(0);
            let scaled = i64::from(replicas) * percent;
            let resolved = if up {
                scaled.div_euclid(100) + i64::from(scaled.rem_euclid(100) != 0)
            } else {
                scaled.div_euclid(100)
            };
            i32::try_from(resolved).unwrap_or(i32::MAX)
        },
    )
}

/// The strategy of `object`, whether it leaves a moment with no pod, and how many pods it adds
/// while rolling.
fn strategy(kind: WorkloadKind, object: &Value) -> (String, bool, i32) {
    let replicas = replicas(Some(object)).unwrap_or(1);
    match kind {
        WorkloadKind::Deployment => {
            let strategy = object.pointer("/spec/strategy");
            let kind = strategy
                .and_then(|s| s.get("type"))
                .and_then(Value::as_str)
                .unwrap_or("RollingUpdate")
                .to_owned();
            if kind == "Recreate" {
                return (kind, replicas > 0, 0);
            }
            let rolling = strategy.and_then(|s| s.get("rollingUpdate"));
            let surge = resolve(
                rolling.and_then(|r| r.get("maxSurge")),
                "25%",
                replicas,
                true,
            );
            let unavailable = resolve(
                rolling.and_then(|r| r.get("maxUnavailable")),
                "25%",
                replicas,
                false,
            );
            (
                kind,
                replicas > 0 && surge == 0 && unavailable >= replicas,
                surge.max(0),
            )
        }
        WorkloadKind::StatefulSet => {
            let kind = object
                .pointer("/spec/updateStrategy/type")
                .and_then(Value::as_str)
                .unwrap_or("RollingUpdate")
                .to_owned();
            let down = kind != "OnDelete" && replicas == 1;
            (kind, down, 0)
        }
        WorkloadKind::DaemonSet => {
            let strategy = object.pointer("/spec/updateStrategy");
            let kind = strategy
                .and_then(|s| s.get("type"))
                .and_then(Value::as_str)
                .unwrap_or("RollingUpdate")
                .to_owned();
            let surge = strategy
                .and_then(|s| s.pointer("/rollingUpdate/maxSurge"))
                .map_or(0, |v| resolve(Some(v), "0", 1, true));
            let down = kind != "OnDelete" && surge == 0;
            (kind, down, 0)
        }
        WorkloadKind::Job | WorkloadKind::CronJob => (String::new(), false, 0),
    }
}

/// What one pod of a template requests and limits.
fn template_amounts(object: &Value) -> (Amount, Amount) {
    let Some(spec) = object
        .pointer("/spec/template/spec")
        .and_then(|s| serde_json::from_value::<PodSpec>(s.clone()).ok())
    else {
        return (Amount::default(), Amount::default());
    };
    spec_amounts(&spec)
}

fn template_fields(object: &PlannedObject) -> Vec<String> {
    object
        .changes
        .iter()
        .filter(|c| c.path.starts_with("spec.template"))
        .map(|c| c.path.clone())
        .take(10)
        .collect()
}

/// What the plan adds at its peak: pods, requests and limits.
#[derive(Debug, Clone, Copy, Default)]
struct Need {
    pods: u64,
    requests: Amount,
    limits: Amount,
    /// One new pod's requests, for the node check.
    pod: Amount,
    /// Pods that each need `pod` on some node.
    placed: u64,
}

const fn times(amount: Amount, n: u64) -> Amount {
    Amount {
        cpu: amount.cpu.saturating_mul(n),
        memory: amount.memory.saturating_mul(n),
    }
}

const fn growth(new: Amount, old: Amount) -> Amount {
    Amount {
        cpu: new.cpu.saturating_sub(old.cpu),
        memory: new.memory.saturating_sub(old.memory),
    }
}

/// The rollouts of `plan` and what they add at their peak.
fn rollouts(plan: &Plan) -> (Vec<Rollout>, Need) {
    let mut out: BTreeMap<WorkloadRef, Rollout> = BTreeMap::new();
    let mut need = Need::default();
    for object in &plan.objects {
        let Some(workload) = workload_of(object) else {
            continue;
        };
        let before = object.before.as_ref();
        let after = after_of(object);
        let mut rollout = Rollout {
            workload: workload.clone(),
            actions: Vec::new(),
            from_replicas: None,
            replicas: replicas(after),
            strategy: None,
            downtime: false,
            fields: Vec::new(),
        };
        match (before, after, &object.write) {
            (_, _, Write::Delete { .. }) => {
                rollout.actions.push(RolloutAction::Stop);
                rollout.replicas = None;
                rollout.downtime = true;
            }
            (None, Some(after), _) => {
                rollout.actions.push(RolloutAction::Start);
                let (requests, limits) = template_amounts(after);
                let count = u64::try_from(replicas(Some(after)).unwrap_or(1)).unwrap_or(0);
                need.pods += count;
                need.requests.add(times(requests, count));
                need.limits.add(times(limits, count));
                need.pod = requests;
                need.placed += count;
            }
            (Some(before), Some(after), _) => {
                let (old_requests, old_limits) = template_amounts(before);
                let (new_requests, new_limits) = template_amounts(after);
                let from = replicas(Some(before)).unwrap_or(1);
                let to = replicas(Some(after)).unwrap_or(1);
                if before.pointer("/spec/template") != after.pointer("/spec/template") {
                    let (kind, downtime, surge) = strategy(workload.kind, after);
                    rollout.actions.push(RolloutAction::Roll);
                    rollout.downtime = downtime;
                    rollout.fields = template_fields(object);
                    let surge = u64::try_from(surge.min(to)).unwrap_or(0);
                    let kept = u64::try_from(to).unwrap_or(0);
                    if kind != "OnDelete" {
                        need.pods += surge;
                        need.requests.add(times(new_requests, surge));
                        need.limits.add(times(new_limits, surge));
                        need.requests
                            .add(times(growth(new_requests, old_requests), kept));
                        need.limits.add(times(growth(new_limits, old_limits), kept));
                        need.pod = new_requests;
                        need.placed += surge.max(u64::from(new_requests != old_requests));
                    }
                    rollout.strategy = Some(kind);
                }
                if from != to {
                    rollout.actions.push(RolloutAction::Scale);
                    rollout.from_replicas = Some(from);
                    if to > from {
                        let added = u64::try_from(to - from).unwrap_or(0);
                        need.pods += added;
                        need.requests.add(times(new_requests, added));
                        need.limits.add(times(new_limits, added));
                        need.pod = new_requests;
                        need.placed += added;
                    }
                }
            }
            _ => {}
        }
        if !rollout.actions.is_empty() {
            out.insert(workload, rollout);
        }
    }
    for warning in &plan.warnings {
        if let Warning::RestartNeeded { workloads } = warning {
            for workload in workloads {
                out.entry(workload.clone())
                    .or_insert_with(|| Rollout {
                        workload: workload.clone(),
                        actions: Vec::new(),
                        from_replicas: None,
                        replicas: None,
                        strategy: None,
                        downtime: false,
                        fields: Vec::new(),
                    })
                    .actions
                    .push(RolloutAction::PendingRestart);
            }
        }
    }
    (out.into_values().collect(), need)
}

/// The CloudNativePG `Clusters` `plan` creates, with what their instances add to `need`.
fn databases(plan: &Plan, need: &mut Need) -> Vec<DatabaseStart> {
    let mut out = Vec::new();
    for object in &plan.objects {
        let Some(after) = after_of(object) else {
            continue;
        };
        if object.before.is_some()
            || object.target.kind != "Cluster"
            || !object.target.api_version.starts_with("postgresql.cnpg.io/")
        {
            continue;
        }
        let instances = after
            .pointer("/spec/instances")
            .and_then(Value::as_i64)
            .and_then(|n| i32::try_from(n).ok())
            .unwrap_or(1);
        let amount = |at: &str| Amount {
            cpu: after
                .pointer(&format!("/spec/resources/{at}/cpu"))
                .and_then(Value::as_str)
                .and_then(millicores)
                .unwrap_or(0),
            memory: after
                .pointer(&format!("/spec/resources/{at}/memory"))
                .and_then(Value::as_str)
                .and_then(bytes)
                .unwrap_or(0),
        };
        let count = u64::try_from(instances).unwrap_or(0);
        let requests = amount("requests");
        need.pods += count;
        need.requests.add(times(requests, count));
        need.limits.add(times(amount("limits"), count));
        need.pod = requests;
        need.placed += count;
        out.push(DatabaseStart {
            name: object.target.name.clone(),
            engine: "PostgreSQL".to_owned(),
            instances,
        });
    }
    out
}

/// The hosts the Ingresses and `HTTPRoutes` of the namespace serve now.
fn serving(objects: &NamespaceObjects) -> BTreeSet<String> {
    let ingresses = objects
        .ingresses
        .iter()
        .filter_map(|i| i.spec.as_ref()?.rules.as_ref())
        .flatten()
        .filter_map(|r| r.host.clone());
    let routes = objects
        .http_routes
        .iter()
        .filter_map(|r| r.pointer("/spec/hostnames")?.as_array())
        .flatten()
        .filter_map(|h| h.as_str().map(str::to_owned));
    ingresses.chain(routes).collect()
}

/// The hosts of the Ingresses and `HTTPRoutes` `plan` creates that do not serve yet, sorted and
/// without repeats: a second route for a host that serves (maintenance, a split route) starts
/// nothing.
fn domains(plan: &Plan, serving: &BTreeSet<String>) -> Vec<String> {
    let mut out = BTreeSet::new();
    for object in plan.objects.iter().filter(|o| o.before.is_none()) {
        let Some(after) = after_of(object) else {
            continue;
        };
        let hosts: Vec<&Value> = match object.target.kind.as_str() {
            "Ingress" => after
                .pointer("/spec/rules")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|r| r.get("host"))
                .collect(),
            "HTTPRoute" => after
                .pointer("/spec/hostnames")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .collect(),
            _ => Vec::new(),
        };
        out.extend(
            hosts
                .into_iter()
                .filter_map(Value::as_str)
                .map(str::to_owned),
        );
    }
    out.into_iter().filter(|h| !serving.contains(h)).collect()
}

/// The quotas `need` exceeds.
fn quota_blockers(need: Need, quotas: &[ResourceQuota]) -> Vec<Blocker> {
    let mut out = Vec::new();
    for quota in quotas {
        let status = quota.status.as_ref();
        let hard = status.and_then(|s| s.hard.as_ref());
        let used = status.and_then(|s| s.used.as_ref());
        for (resource, limit) in hard.into_iter().flatten() {
            let wanted = match resource.as_str() {
                "pods" => need.pods,
                "requests.cpu" | "cpu" => need.requests.cpu,
                "requests.memory" | "memory" => need.requests.memory,
                "limits.cpu" => need.limits.cpu,
                "limits.memory" => need.limits.memory,
                _ => continue,
            };
            if wanted == 0 {
                continue;
            }
            let amount = |text: &str| match resource.as_str() {
                "pods" => text.parse::<u64>().ok(),
                r if r.ends_with("cpu") => millicores(text),
                _ => bytes(text),
            };
            let left = amount(&limit.0).unwrap_or(0).saturating_sub(
                used.and_then(|u| u.get(resource))
                    .and_then(|u| amount(&u.0))
                    .unwrap_or(0),
            );
            if wanted > left {
                out.push(Blocker {
                    kind: BlockerKind::Quota,
                    name: quota.metadata.name.clone(),
                    resource: resource.clone(),
                    need: wanted,
                    available: left,
                });
            }
        }
    }
    out
}

/// What keeps `need`'s new pods off the nodes: the resource one pod needs more of than any node
/// has left, else the count of pods that fit.
fn node_blockers(need: Need, nodes: &[(Amount, u64)]) -> Vec<Blocker> {
    if need.placed == 0 {
        return Vec::new();
    }
    let per_node = |(free, pods): &(Amount, u64)| -> u64 {
        let by = |free: u64, one: u64| free.checked_div(one).unwrap_or(u64::MAX);
        by(free.cpu, need.pod.cpu)
            .min(by(free.memory, need.pod.memory))
            .min(*pods)
    };
    let placeable: u64 = nodes.iter().map(per_node).fold(0, u64::saturating_add);
    if placeable >= need.placed {
        return Vec::new();
    }
    let most =
        |pick: fn(&Amount) -> u64| nodes.iter().map(|(free, _)| pick(free)).max().unwrap_or(0);
    let short: Vec<Blocker> = [
        ("cpu", need.pod.cpu, most(|a| a.cpu)),
        ("memory", need.pod.memory, most(|a| a.memory)),
    ]
    .into_iter()
    .filter(|(_, one, available)| one > available)
    .map(|(resource, one, available)| Blocker {
        kind: BlockerKind::Node,
        name: None,
        resource: resource.to_owned(),
        need: one,
        available,
    })
    .collect();
    if short.is_empty() {
        vec![Blocker {
            kind: BlockerKind::Node,
            name: None,
            resource: "pods".to_owned(),
            need: need.placed,
            available: placeable,
        }]
    } else {
        short
    }
}

/// Whether `need` fits the quotas and nodes of `room`.
fn fit(need: Need, room: &Room) -> Fit {
    let mut fit = Fit {
        status: FitStatus::NotNeeded,
        pods: need.pods,
        cpu_millicores: need.requests.cpu,
        memory_bytes: need.requests.memory,
        blockers: Vec::new(),
    };
    if need.pods == 0 && need.requests == Amount::default() {
        return fit;
    }
    if let Some(quotas) = &room.quotas {
        fit.blockers.extend(quota_blockers(need, quotas));
    }
    if let Some(nodes) = &room.nodes {
        fit.blockers.extend(node_blockers(need, nodes));
    }
    fit.status = if !fit.blockers.is_empty() {
        FitStatus::Blocked
    } else if room.quotas.is_none() || room.nodes.is_none() {
        FitStatus::Unknown
    } else {
        FitStatus::Fits
    };
    fit
}

fn mounts(template: &Value, volume: &str) -> Vec<String> {
    template
        .pointer("/spec/template/spec/containers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|c| {
            c.get("volumeMounts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter(|m| m.get("name").and_then(Value::as_str) == Some(volume))
        .filter_map(|m| {
            m.get("mountPath")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect()
}

fn data_loss(plan: &Plan, rollouts: &[Rollout]) -> Vec<DataLoss> {
    let mut out = Vec::new();
    for object in &plan.objects {
        if object.target.plural == "persistentvolumeclaims"
            && matches!(object.write, Write::Delete { .. })
        {
            out.push(DataLoss {
                kind: DataLossKind::ClaimDeleted,
                workload: None,
                claim: Some(object.target.name.clone()),
                volume: None,
                mount_paths: Vec::new(),
            });
        }
        let Some(workload) = workload_of(object) else {
            continue;
        };
        let Some(before) = object.before.as_ref() else {
            continue;
        };
        let replaced = rollouts.iter().any(|r| {
            r.workload == workload
                && r.actions
                    .iter()
                    .any(|a| matches!(a, RolloutAction::Roll | RolloutAction::Stop))
                && r.strategy.as_deref() != Some("OnDelete")
        });
        if replaced {
            for volume in before
                .pointer("/spec/template/spec/volumes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|v| v.get("emptyDir").is_some())
            {
                let name = volume
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                out.push(DataLoss {
                    kind: DataLossKind::EmptyDir,
                    workload: Some(workload.clone()),
                    claim: None,
                    volume: Some(name.to_owned()),
                    mount_paths: mounts(before, name),
                });
            }
        }
        if workload.kind == WorkloadKind::StatefulSet {
            let policy = |when: &str| {
                before
                    .pointer(&format!(
                        "/spec/persistentVolumeClaimRetentionPolicy/{when}"
                    ))
                    .and_then(Value::as_str)
                    == Some("Delete")
            };
            let deleted = matches!(object.write, Write::Delete { .. });
            let scaled_down =
                replicas(Some(before)) > replicas(after_of(object)) && after_of(object).is_some();
            if (deleted && policy("whenDeleted")) || (scaled_down && policy("whenScaled")) {
                for template in before
                    .pointer("/spec/volumeClaimTemplates")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let name = template
                        .pointer("/metadata/name")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    out.push(DataLoss {
                        kind: DataLossKind::ClaimsOfStatefulSet,
                        workload: Some(workload.clone()),
                        claim: Some(name.to_owned()),
                        volume: None,
                        mount_paths: mounts(before, name),
                    });
                }
            }
        }
    }
    out
}

fn removals(plan: &Plan, request: &PlanRequest) -> Vec<Removal> {
    let edited = |object: &PlannedObject| {
        request.changes.iter().any(|change| {
            matches!(change, Change::Edit { resource, name, .. }
                if resource.resource == object.target.plural && *name == object.target.name)
        })
    };
    plan.objects
        .iter()
        .filter(|o| !matches!(o.write, Write::Delete { .. }) && o.before.is_some())
        .flat_map(|object| {
            let left_out = edited(object);
            object
                .changes
                .iter()
                .filter(|c| c.after.is_none())
                .filter_map(move |c| {
                    Some(Removal {
                        target: ObjectName {
                            kind: object.target.kind.clone(),
                            name: object.target.name.clone(),
                        },
                        path: c.path.clone(),
                        before: c.before.clone()?,
                        left_out_of_edit: left_out,
                    })
                })
        })
        .collect()
}

/// What `plan`, made for `request`, does once applied, with the room `room` gives its pods.
pub(crate) fn preview(
    plan: &Plan,
    request: &PlanRequest,
    room: &Room,
    serving: &BTreeSet<String>,
    unreadable: Vec<UnreadableView>,
) -> Preview {
    let (rollouts, mut need) = rollouts(plan);
    let databases = databases(plan, &mut need);
    Preview {
        fit: fit(need, room),
        data_loss: data_loss(plan, &rollouts),
        removals: removals(plan, request),
        rollouts,
        databases,
        domains: domains(plan, serving),
        unreadable,
    }
}

/// Reads what `plan`'s new pods need to fit, when it adds or grows any, and previews it.
pub(crate) async fn read(
    cluster: &dyn Cluster,
    project: &str,
    plan: &Plan,
    request: &PlanRequest,
) -> Result<Preview, ApiError> {
    let mut unreadable = Vec::new();
    let (_, mut need) = rollouts(plan);
    databases(plan, &mut need);
    let room = if need.pods == 0 && need.requests == Amount::default() {
        Room::default()
    } else {
        let (quotas, nodes, pods) = futures_util::join!(
            cluster.list::<ResourceQuota>(Some(project)),
            cluster.list::<Node>(None),
            cluster.cluster_pods(WIDE_LIMIT),
        );
        let quotas_read = quotas.is_ok();
        let quotas = allowed(quotas, "resourcequotas", &mut unreadable)?;
        let nodes_read = nodes.is_ok();
        let nodes = allowed(nodes, "nodes", &mut unreadable)?;
        let pods_read = pods.is_ok();
        let pods = allowed(pods, "pods", &mut unreadable)?;
        Room {
            nodes: (nodes_read && pods_read && !pods.truncated)
                .then(|| node_room(&nodes, &pods.pods)),
            quotas: quotas_read.then_some(quotas),
        }
    };
    let creates_route = plan
        .objects
        .iter()
        .any(|o| o.before.is_none() && matches!(o.target.kind.as_str(), "Ingress" | "HTTPRoute"));
    let serving = if creates_route {
        cluster
            .namespace_objects(project)
            .await
            .map_or_default(|objects| serving(&objects))
    } else {
        BTreeSet::new()
    };
    Ok(preview(plan, request, &room, &serving, unreadable))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn surge_and_unavailable_resolve_as_kubernetes_rounds_them() {
        assert_eq!(resolve(None, "25%", 1, true), 1);
        assert_eq!(resolve(None, "25%", 1, false), 0);
        assert_eq!(resolve(Some(&json!("50%")), "25%", 3, true), 2);
        assert_eq!(resolve(Some(&json!(0)), "25%", 3, true), 0);
    }

    #[test]
    fn a_strategy_says_when_no_pod_serves() {
        let deployment = |strategy: Value, replicas: i32| json!({"spec": {"replicas": replicas, "strategy": strategy}});
        assert!(
            strategy(
                WorkloadKind::Deployment,
                &deployment(json!({"type": "Recreate"}), 2)
            )
            .1
        );
        assert!(!strategy(WorkloadKind::Deployment, &deployment(json!({}), 1)).1);
        assert!(
            strategy(
                WorkloadKind::Deployment,
                &deployment(
                    json!({"rollingUpdate": {"maxSurge": 0, "maxUnavailable": "100%"}}),
                    2
                )
            )
            .1
        );
        assert!(strategy(WorkloadKind::StatefulSet, &json!({"spec": {"replicas": 1}})).1);
        assert!(!strategy(WorkloadKind::StatefulSet, &json!({"spec": {"replicas": 3}})).1);
        assert!(strategy(WorkloadKind::DaemonSet, &json!({"spec": {}})).1);
    }

    #[test]
    fn a_quota_and_full_nodes_each_block_with_numbers() {
        let need = Need {
            pods: 1,
            requests: Amount {
                cpu: 500,
                memory: 256 << 20,
            },
            limits: Amount::default(),
            pod: Amount {
                cpu: 500,
                memory: 256 << 20,
            },
            placed: 1,
        };
        let quota: ResourceQuota = serde_json::from_value(json!({
            "metadata": {"name": "compute"},
            "status": {"hard": {"requests.cpu": "1", "pods": "10"},
                "used": {"requests.cpu": "800m", "pods": "2"}}
        }))
        .unwrap();
        let room = Room {
            nodes: Some(vec![(
                Amount {
                    cpu: 200,
                    memory: 1 << 30,
                },
                50,
            )]),
            quotas: Some(vec![quota]),
        };
        let fit = fit(need, &room);
        assert_eq!(fit.status, FitStatus::Blocked);
        assert_eq!(fit.blockers.len(), 2);
        assert_eq!(fit.blockers[0].kind, BlockerKind::Quota);
        assert_eq!(fit.blockers[0].available, 200);
        assert_eq!(fit.blockers[1].kind, BlockerKind::Node);
        assert_eq!(fit.blockers[1].resource, "cpu");

        let roomy = Room {
            nodes: Some(vec![(
                Amount {
                    cpu: 4000,
                    memory: 8 << 30,
                },
                50,
            )]),
            quotas: Some(Vec::new()),
        };
        assert_eq!(super::fit(need, &roomy).status, FitStatus::Fits);
        assert_eq!(
            super::fit(need, &Room::default()).status,
            FitStatus::Unknown
        );
    }
}
