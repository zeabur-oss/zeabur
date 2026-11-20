//! The pods a plan starts, tried before the plan is sealed.
//!
//! A `ResourceQuota` on `pods`, `requests.cpu` or `requests.memory` applies to Pods, not to the
//! Deployment that makes them [k8s-quota], so the dry runs of a plan's own objects pass a scale
//! past it. For each Deployment or StatefulSet the plan starts or scales up, one Pod built from
//! its template is sent as a dry run, as the person. kube-apiserver refuses it for a quota, a
//! `LimitRange` maximum or a Pod Security level, and answers it with the `LimitRange`'s default
//! requests and limits filled in [k8s-limitrange]. That Pod's requests and limits, times the pods
//! the plan adds, are then held against each quota's `status.hard` and `status.used`.
//!
//! Not counted: the extra pods of a rolling update, DaemonSets (one pod per node) and quotas
//! with scopes. When the person may not create Pods, or the quotas cannot be read, nothing is
//! known and the plan goes on: the rollout shows the reason after apply.
//!
//! [k8s-quota]: https://kubernetes.io/docs/concepts/policy/resource-quotas/
//! [k8s-limitrange]: https://kubernetes.io/docs/concepts/policy/limit-range/

use serde::de::DeserializeOwned;
use serde_json::Value;
use zeabur_core::message::Message;
use zeabur_core::origin::Origin;
use zeabur_core::problem::{Kind, Problem, ProblemAt};
use zeabur_kube::k8s_openapi::api::apps::v1::{Deployment, StatefulSet};
use zeabur_kube::k8s_openapi::api::core::v1::{
    PersistentVolumeClaim, PersistentVolumeClaimVolumeSource, Pod, PodTemplateSpec, ResourceQuota,
    Volume,
};
use zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use zeabur_kube::{Cluster, ClusterExt as _, ClusterPod, WriteRequest};
use zeabur_plan::{Change, Plan, PlanRequest, PlannedObject, Target, Write};

use super::creations::without_secret_values;
use super::refusals::refused;
use crate::cluster::capacity::{Amount, bytes, millicores, pod_amounts};
use crate::error::ApiError;

/// A workload whose pods the plan adds.
struct Added<'p> {
    object: &'p PlannedObject,
    template: PodTemplateSpec,
    /// A StatefulSet's `volumeClaimTemplates`.
    claims: Vec<PersistentVolumeClaim>,
    pods: u64,
    /// The index of the change that adds them, in the request.
    change: Option<usize>,
}

/// What the added pods need together.
#[derive(Default)]
struct Need {
    pods: u64,
    requests: Amount,
    limits: Amount,
}

/// A Deployment's or StatefulSet's replicas, pod template and claim templates.
struct Workload {
    replicas: u64,
    template: Option<PodTemplateSpec>,
    claims: Vec<PersistentVolumeClaim>,
}

impl Workload {
    /// `object` read as `kind`; `None` for a kind that starts no pods this way.
    fn read(kind: &str, object: &Value) -> Option<Result<Self, ApiError>> {
        let replicas = |r: Option<i32>| u64::try_from(r.unwrap_or(1)).unwrap_or(0);
        match kind {
            "Deployment" => Some(parse::<Deployment>(kind, object).map(|d| {
                let spec = d.spec.unwrap_or_default();
                Self {
                    replicas: replicas(spec.replicas),
                    template: Some(spec.template),
                    claims: Vec::new(),
                }
            })),
            "StatefulSet" => Some(parse::<StatefulSet>(kind, object).map(|s| {
                let spec = s.spec.unwrap_or_default();
                Self {
                    replicas: replicas(spec.replicas),
                    template: Some(spec.template),
                    claims: spec.volume_claim_templates.unwrap_or_default(),
                }
            })),
            _ => None,
        }
    }
}

/// `object` as a `K`. The plan holds objects of any kind as JSON; one that does not parse as
/// the kind it names is a fault in Zeabur.
fn parse<K: DeserializeOwned>(kind: &str, object: &Value) -> Result<K, ApiError> {
    serde_json::from_value(object.clone()).map_err(|error| {
        ApiError::from(Problem::new(
            Kind::Internal,
            format!("A planned {kind} is not a {kind}: {error}"),
        ))
    })
}

/// The Deployments and StatefulSets `plan` starts or scales up, with the pods each adds.
fn added<'p>(request: &PlanRequest, plan: &'p Plan) -> Result<Vec<Added<'p>>, ApiError> {
    let mut added = Vec::new();
    for object in &plan.objects {
        let kind = object.target.kind.as_str();
        if object.target.api_version != "apps/v1"
            || matches!(object.write, Write::Delete { .. })
            || object.raw_after.is_null()
        {
            continue;
        }
        let Some(after) = Workload::read(kind, &object.raw_after).transpose()? else {
            continue;
        };
        let before = match &object.before {
            Some(before) => Workload::read(kind, before)
                .transpose()?
                .map_or(0, |w| w.replicas),
            None => 0,
        };
        let pods = after.replicas.saturating_sub(before);
        if let (true, Some(template)) = (pods > 0, after.template) {
            added.push(Added {
                object,
                template,
                claims: after.claims,
                pods,
                change: object.change.or_else(|| changed_by(request, object)),
            });
        }
    }
    Ok(added)
}

/// The change of `request` that scales or edits `object`.
fn changed_by(request: &PlanRequest, object: &PlannedObject) -> Option<usize> {
    let target = &object.target;
    request.changes.iter().position(|change| {
        matches!(change, Change::Scale { workload, .. }
            if workload.kind.as_str() == target.kind && workload.name == target.name)
            || matches!(change, Change::Edit { resource, name, .. }
                if resource.resource == target.plural && *name == target.name)
    })
}

/// Refuses `plan` when a Pod it starts would be refused, or when its pods together would go
/// over a quota, at the change of the workload that crosses it.
pub(super) async fn dry_run(
    cluster: &dyn Cluster,
    project: &str,
    request: &PlanRequest,
    plan: &Plan,
) -> Result<(), ApiError> {
    let added = added(request, plan)?;
    if added.is_empty() {
        return Ok(());
    }
    let quotas = cluster
        .list::<ResourceQuota>(Some(project))
        .await
        .unwrap_or_default();
    let mut need = Need::default();
    for workload in &added {
        let Some(pod) = admitted(cluster, project, plan, workload).await? else {
            continue;
        };
        let (requests, limits) = pod_amounts(&pod);
        need.pods = need.pods.saturating_add(workload.pods);
        need.requests.add(requests.times(workload.pods));
        need.limits.add(limits.times(workload.pods));
        if let Some(problem) = quotas
            .iter()
            .find_map(|quota| over(project, quota, &need, workload))
        {
            return Err(ApiError::from(problem));
        }
    }
    Ok(())
}

/// One Pod of `workload` as kube-apiserver admits it; `None` when that is not known.
async fn admitted(
    cluster: &dyn Cluster,
    project: &str,
    plan: &Plan,
    workload: &Added<'_>,
) -> Result<Option<ClusterPod>, ApiError> {
    // Generated, so a Pod already named like it can't answer 409 in place of admission.
    let name = format!("{}-dry-run-", workload.object.target.name);
    let target = Target::of::<Pod>(&name);
    let write = WriteRequest {
        dry_run: true,
        ..super::write_request(
            project,
            &target,
            &Write::Create {
                body: serde_json::to_value(pod(workload, &name, plan)).map_err(|error| {
                    ApiError::from(Problem::new(
                        Kind::Internal,
                        format!("The dry-run Pod does not serialize: {error}"),
                    ))
                })?,
            },
        )
    };
    match cluster.write(&write).await {
        Ok(written) => written
            .object
            .map(|object| {
                ClusterPod::from_object(object).map_err(|error| {
                    ApiError::from(Problem::new(
                        Kind::Internal,
                        format!("kube-apiserver answered the dry-run Pod with no Pod: {error}"),
                    ))
                })
            })
            .transpose(),
        Err(error) => {
            let mut problem = refused(cluster, project, &target, &write, error).await;
            if !matches!(problem.kind, Kind::QuotaExceeded | Kind::PolicyDenied) {
                return Ok(None);
            }
            problem.at = at(workload);
            Err(ApiError::from(without_secret_values(problem, plan)))
        }
    }
}

/// A Pod of `workload`'s template whose name kube-apiserver generates from `name`. A
/// StatefulSet's claim templates become the claims of its first replica, so both `volumeMounts`
/// and `volumeDevices` resolve (a block device takes only a claim); each replaces a template
/// volume of its name, as the controller's `updateStorage` does. A dry run doesn't read the claim.
/// A ServiceAccount that `plan` creates does not exist yet, and the ServiceAccount admission plugin
/// refuses a Pod that names a missing one, so the Pod runs as `default` instead: quotas, limit
/// ranges and Pod Security do not read it.
fn pod(workload: &Added<'_>, name: &str, plan: &Plan) -> Pod {
    let mut spec = workload.template.spec.clone().unwrap_or_default();
    let account = spec
        .service_account_name
        .as_deref()
        .or(spec.service_account.as_deref());
    if account.is_some_and(|account| creates_account(plan, account)) {
        spec.service_account_name = None;
        spec.service_account = None;
    }
    let claims: Vec<Volume> = workload
        .claims
        .iter()
        .filter_map(|claim| claim.metadata.name.as_deref())
        .map(|claim| Volume {
            name: claim.to_owned(),
            persistent_volume_claim: Some(PersistentVolumeClaimVolumeSource {
                claim_name: format!("{claim}-{}-0", workload.object.target.name),
                read_only: None,
            }),
            ..Volume::default()
        })
        .collect();
    if !claims.is_empty() {
        let volumes = spec.volumes.get_or_insert_default();
        volumes.retain(|v| !claims.iter().any(|c| c.name == v.name));
        volumes.extend(claims);
    }
    Pod {
        metadata: ObjectMeta {
            name: None,
            generate_name: Some(name.to_owned()),
            ..workload.template.metadata.clone().unwrap_or_default()
        },
        spec: Some(spec),
        status: None,
    }
}

fn creates_account(plan: &Plan, account: &str) -> bool {
    plan.objects.iter().any(|o| {
        o.target.api_version == "v1"
            && o.target.kind == "ServiceAccount"
            && o.target.name == account
            && matches!(o.write, Write::Create { .. })
    })
}

fn at(workload: &Added<'_>) -> Option<ProblemAt> {
    workload.change.map(|change| ProblemAt {
        change,
        line: None,
        field: None,
    })
}

/// The problem when `need` goes over `quota`, for the pods `workload` adds last.
fn over(
    project: &str,
    quota: &ResourceQuota,
    need: &Need,
    workload: &Added<'_>,
) -> Option<Problem> {
    let (resource, hard, used) = exceeded(quota, need)?;
    let quota = quota.metadata.name.as_deref()?;
    let name = workload.object.target.name.as_str();
    let field = |part: &str| {
        Origin::complete("v1", "ResourceQuota", Some(project), quota)
            .with_field(format!("status.{part}.{resource}"))
    };
    let mut problem = Problem::new(
        Kind::QuotaExceeded,
        format!(
            "Adding {} pods to {name} would take the project over its limit {quota}.",
            workload.pods
        ),
    );
    problem.summary = problem.summary.with("name", name);
    problem.why = Some(
        Message::new("problem.why.quota-exceeded")
            .with("quota", quota)
            .with("resource", resource)
            .with("hard", hard.as_str())
            .with("used", used.as_str()),
    );
    problem.evidence = vec![
        field("hard").with_value(&hard),
        field("used").with_value(&used),
    ];
    problem.fix_for_occasion(
        format!(
            "Run fewer replicas of {name}, reserve less for each, or ask a cluster administrator \
             to raise the limit {quota}."
        ),
        Message::new("problem.quotaExceeded.fix.scale")
            .with("quota", quota)
            .with("object", name),
    );
    problem.at = at(workload);
    Some(problem)
}

/// The first resource of an unscoped `quota` that `need` takes past `status.hard`, with the
/// quota's hard and used amounts.
fn exceeded<'q>(quota: &'q ResourceQuota, need: &Need) -> Option<(&'q str, String, String)> {
    let scoped = quota.spec.as_ref().is_some_and(|s| {
        s.scopes.as_ref().is_some_and(|v| !v.is_empty()) || s.scope_selector.is_some()
    });
    if scoped {
        return None;
    }
    let status = quota.status.as_ref()?;
    let used = status.used.as_ref();
    status.hard.as_ref()?.iter().find_map(|(resource, hard)| {
        let (amount, read): (u64, fn(&str) -> Option<u64>) = match resource.as_str() {
            "pods" | "count/pods" => (need.pods, bytes),
            "cpu" | "requests.cpu" => (need.requests.cpu, millicores),
            "memory" | "requests.memory" => (need.requests.memory, bytes),
            "limits.cpu" => (need.limits.cpu, millicores),
            "limits.memory" => (need.limits.memory, bytes),
            _ => return None,
        };
        let used = used
            .and_then(|u| u.get(resource))
            .map_or_else(|| "0".to_owned(), |q| q.0.clone());
        let over = amount > 0 && read(&used)?.saturating_add(amount) > read(&hard.0)?;
        over.then(|| (resource.as_str(), hard.0.clone(), used))
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zeabur_kube::k8s_openapi::api::core::v1::{
        ConfigMapVolumeSource, EmptyDirVolumeSource, PodSpec,
    };

    use super::*;

    fn planned(kind: &str, name: &str) -> Value {
        json!({
            "target": {"apiVersion": if kind == "ServiceAccount" { "v1" } else { "apps/v1" },
                       "kind": kind, "plural": format!("{}s", kind.to_lowercase()), "name": name},
            "changes": [], "owners": [], "write": {"type": "create", "body": {}}
        })
    }

    fn plan(objects: &[Value]) -> Plan {
        serde_json::from_value(json!({"objects": objects, "impact": [], "warnings": []})).unwrap()
    }

    fn account_of(plan: &Plan) -> Option<String> {
        let workload = Added {
            object: &plan.objects[0],
            template: PodTemplateSpec {
                metadata: None,
                spec: Some(PodSpec {
                    service_account_name: Some("worker".to_owned()),
                    ..PodSpec::default()
                }),
            },
            claims: Vec::new(),
            pods: 1,
            change: Some(0),
        };
        pod(&workload, "web-dry-run-", plan)
            .spec?
            .service_account_name
    }

    #[test]
    fn a_service_account_the_plan_creates_is_left_out_of_the_dry_run() {
        let with = plan(&[
            planned("Deployment", "web"),
            planned("ServiceAccount", "worker"),
        ]);
        assert_eq!(account_of(&with), None);
        let without = plan(&[planned("Deployment", "web")]);
        assert_eq!(account_of(&without).as_deref(), Some("worker"));
    }

    #[test]
    fn a_claim_template_becomes_a_claim_volume_in_place_of_the_template_volume_of_its_name() {
        let plan = plan(&[planned("StatefulSet", "db")]);
        let volume = |name: &str| Volume {
            name: name.to_owned(),
            ..Volume::default()
        };
        let workload = Added {
            object: &plan.objects[0],
            template: PodTemplateSpec {
                metadata: None,
                spec: Some(PodSpec {
                    volumes: Some(vec![
                        Volume {
                            config_map: Some(ConfigMapVolumeSource {
                                name: "seed".to_owned(),
                                ..ConfigMapVolumeSource::default()
                            }),
                            ..volume("data")
                        },
                        Volume {
                            empty_dir: Some(EmptyDirVolumeSource::default()),
                            ..volume("tmp")
                        },
                    ]),
                    ..PodSpec::default()
                }),
            },
            claims: vec![PersistentVolumeClaim {
                metadata: ObjectMeta {
                    name: Some("data".to_owned()),
                    ..ObjectMeta::default()
                },
                ..PersistentVolumeClaim::default()
            }],
            pods: 1,
            change: Some(0),
        };
        let pod = pod(&workload, "db-dry-run-", &plan);
        assert_eq!(pod.metadata.generate_name.as_deref(), Some("db-dry-run-"));
        assert_eq!(
            pod.spec.and_then(|s| s.volumes),
            Some(vec![
                Volume {
                    empty_dir: Some(EmptyDirVolumeSource::default()),
                    ..volume("tmp")
                },
                Volume {
                    persistent_volume_claim: Some(PersistentVolumeClaimVolumeSource {
                        claim_name: "data-db-0".to_owned(),
                        read_only: None,
                    }),
                    ..volume("data")
                },
            ])
        );
    }

    #[test]
    fn a_planned_workload_that_is_not_its_kind_is_a_fault() {
        let mut plan = plan(&[planned("Deployment", "web")]);
        plan.objects[0].raw_after = json!({"spec": {"replicas": "three"}});
        let request: PlanRequest = serde_json::from_value(json!({"changes": []})).unwrap();
        let Err(ApiError(problem)) = added(&request, &plan) else {
            panic!("a Deployment with text replicas parsed");
        };
        assert_eq!(problem.kind, Kind::Internal);
    }
}
