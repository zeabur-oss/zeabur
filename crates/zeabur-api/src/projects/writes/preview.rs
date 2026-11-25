//! What a plan does once applied, beyond the fields it writes: which workloads roll and whether
//! that leaves a moment with no pod, what data goes, and which fields it removes.
//!
//! The preview does not predict whether new pods fit. After apply, the rollout shows its progress
//! and the reason a pod cannot start.
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
//! [k8s-emptydir]: https://kubernetes.io/docs/concepts/storage/volumes/#emptydir
//! [k8s-statefulset-pvc-retention]: https://kubernetes.io/docs/concepts/workloads/controllers/statefulset/#persistentvolumeclaim-retention

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_kube::{Cluster, NamespaceObjects};
use zeabur_model::{WorkloadKind, WorkloadRef};
use zeabur_modules::databases::{Engine, Operator};
use zeabur_plan::{Change, ObjectName, Plan, PlanRequest, PlannedObject, Warning, Write};

/// What happens to a workload's pods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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

/// What kind of data a plan loses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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

/// A database object a plan creates: its operator starts its members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseStart {
    /// The object's name.
    pub name: String,
    /// The engine.
    pub engine: Engine,
    /// The members it asks for.
    pub instances: i32,
}

/// What a plan does once applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    /// The workloads whose pods change, sorted.
    pub rollouts: Vec<Rollout>,
    /// The databases it creates, by name.
    pub databases: Vec<DatabaseStart>,
    /// The names of the databases it deletes.
    pub deleted_databases: Vec<String>,
    /// The public domains its new Ingresses and `HTTPRoutes` route, sorted.
    pub domains: Vec<String>,
    /// The data it loses.
    pub data_loss: Vec<DataLoss>,
    /// The fields it removes, by object then path.
    pub removals: Vec<Removal>,
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

fn template_fields(object: &PlannedObject) -> Vec<String> {
    object
        .changes
        .iter()
        .filter(|c| c.path.starts_with("spec.template"))
        .map(|c| c.path.clone())
        .take(10)
        .collect()
}

/// The rollouts of `plan`.
fn rollouts(plan: &Plan) -> Vec<Rollout> {
    let mut out: BTreeMap<WorkloadRef, Rollout> = BTreeMap::new();
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
            (None, Some(_), _) => rollout.actions.push(RolloutAction::Start),
            (Some(before), Some(after), _) => {
                let from = replicas(Some(before)).unwrap_or(1);
                let to = replicas(Some(after)).unwrap_or(1);
                if before.pointer("/spec/template") != after.pointer("/spec/template") {
                    let (kind, downtime, _) = strategy(workload.kind, after);
                    rollout.actions.push(RolloutAction::Roll);
                    rollout.downtime = downtime;
                    rollout.fields = template_fields(object);
                    rollout.strategy = Some(kind);
                }
                if from != to {
                    rollout.actions.push(RolloutAction::Scale);
                    rollout.from_replicas = Some(from);
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
    out.into_values().collect()
}

/// The databases `plan` creates, and the names of those it deletes.
fn databases(plan: &Plan) -> (Vec<DatabaseStart>, Vec<String>) {
    let mut starts = Vec::new();
    let mut deletes = Vec::new();
    for object in &plan.objects {
        let Some(operator) = Operator::of(&object.target.api_version, &object.target.kind) else {
            continue;
        };
        if matches!(object.write, Write::Delete { .. }) {
            deletes.push(object.target.name.clone());
        } else if let (None, Some(after)) = (&object.before, after_of(object)) {
            starts.push(DatabaseStart {
                name: object.target.name.clone(),
                engine: operator.engine,
                instances: i32::try_from(operator.members(after)).unwrap_or(1),
            });
        }
    }
    (starts, deletes)
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

/// What `plan`, made for `request`, does once applied.
pub(crate) fn preview(plan: &Plan, request: &PlanRequest, serving: &BTreeSet<String>) -> Preview {
    let rollouts = rollouts(plan);
    let (databases, deleted_databases) = databases(plan);
    Preview {
        data_loss: data_loss(plan, &rollouts),
        removals: removals(plan, request),
        databases,
        deleted_databases,
        domains: domains(plan, serving),
        rollouts,
    }
}

/// Reads the routes the namespace serves now, when `plan` creates one, and previews it.
pub(crate) async fn read(
    cluster: &dyn Cluster,
    project: &str,
    plan: &Plan,
    request: &PlanRequest,
) -> Preview {
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
    preview(plan, request, &serving)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zeabur_plan::Target;

    use super::*;

    fn object(api_version: &str, kind: &str, name: &str, write: Write) -> PlannedObject {
        let creates = matches!(write, Write::Create { .. });
        let deletes = matches!(write, Write::Delete { .. });
        let body = json!({ "spec": { "replicas": 2, "instances": 2 } });
        PlannedObject {
            target: Target {
                api_version: api_version.to_owned(),
                kind: kind.to_owned(),
                plural: String::new(),
                name: name.to_owned(),
            },
            before: (!creates).then(|| body.clone()),
            after: (!deletes).then(|| body.clone()),
            before_yaml: None,
            after_yaml: None,
            changes: Vec::new(),
            owners: Vec::new(),
            resource_version: None,
            write,
            change: None,
            raw_after: if deletes { Value::Null } else { body },
        }
    }

    fn create() -> Write {
        Write::Create { body: json!({}) }
    }

    fn plan_of(objects: Vec<PlannedObject>) -> Plan {
        Plan {
            objects,
            impact: Vec::new(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn a_database_of_each_engine_starts_with_its_members() {
        let kinds = [
            ("postgresql.cnpg.io/v1", "Cluster", Engine::Postgresql, 2),
            (
                "psmdb.percona.com/v1",
                "PerconaServerMongoDB",
                Engine::Mongodb,
                3,
            ),
            ("moco.cybozu.com/v1beta2", "MySQLCluster", Engine::Mysql, 2),
            ("k8s.mariadb.com/v1alpha1", "MariaDB", Engine::Mariadb, 2),
            ("dragonflydb.io/v1alpha1", "Dragonfly", Engine::Dragonfly, 2),
        ];
        let objects = kinds
            .iter()
            .map(|(api, kind, ..)| object(api, kind, "db", create()))
            .collect();
        let (starts, deletes) = databases(&plan_of(objects));
        let expected: Vec<_> = kinds
            .iter()
            .map(|(_, _, engine, instances)| (*engine, *instances))
            .collect();
        let found: Vec<_> = starts.iter().map(|d| (d.engine, d.instances)).collect();
        assert_eq!(found, expected);
        assert_eq!(deletes, Vec::<String>::new());
    }

    #[test]
    fn only_a_new_database_starts_and_a_deleted_one_is_named() {
        let delete = Write::Delete {
            uid: None,
            resource_version: "1".to_owned(),
            foreground: false,
        };
        let patch = Write::Patch {
            operations: Vec::new(),
        };
        let (starts, deletes) = databases(&plan_of(vec![
            object("postgresql.cnpg.io/v1", "Cluster", "edited", patch),
            object("moco.cybozu.com/v1beta2", "MySQLCluster", "gone", delete),
            object("apps/v1", "Deployment", "web", create()),
        ]));
        assert_eq!(starts, Vec::new());
        assert_eq!(deletes, ["gone"]);
    }

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
}
