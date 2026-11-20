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

use serde_json::{Value, json};
use zeabur_core::message::Message;
use zeabur_core::origin::Origin;
use zeabur_core::problem::{Kind, Problem, ProblemAt};
use zeabur_kube::k8s_openapi::api::core::v1::{Pod, ResourceQuota};
use zeabur_kube::{Cluster, ClusterExt as _, ClusterPod, WriteRequest};
use zeabur_plan::{Change, Plan, PlanRequest, PlannedObject, Target, Write};

use super::creations::without_secret_values;
use super::refusals::refused;
use crate::cluster::capacity::{Amount, bytes, millicores, pod_amounts};
use crate::error::ApiError;

/// A workload whose pods the plan adds.
struct Added<'p> {
    object: &'p PlannedObject,
    template: &'p Value,
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

/// The Deployments and StatefulSets `plan` starts or scales up, with the pods each adds.
fn added<'p>(request: &PlanRequest, plan: &'p Plan) -> Vec<Added<'p>> {
    let replicas = |o: &Value| {
        o.pointer("/spec/replicas")
            .and_then(Value::as_u64)
            .unwrap_or(1)
    };
    plan.objects
        .iter()
        .filter(|o| matches!(o.target.kind.as_str(), "Deployment" | "StatefulSet"))
        .filter(|o| !matches!(o.write, Write::Delete { .. }) && !o.raw_after.is_null())
        .filter_map(|object| {
            let after = &object.raw_after;
            let pods = replicas(after).saturating_sub(object.before.as_ref().map_or(0, replicas));
            let template = after.pointer("/spec/template")?;
            (pods > 0).then(|| Added {
                object,
                template,
                pods,
                change: object.change.or_else(|| changed_by(request, object)),
            })
        })
        .collect()
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
    let added = added(request, plan);
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
    let name = format!("{}-dry-run", workload.object.target.name);
    let target = Target::of::<Pod>(&name);
    let write = WriteRequest {
        dry_run: true,
        ..super::write_request(
            project,
            &target,
            &Write::Create {
                body: pod(workload, &name, plan),
            },
        )
    };
    match cluster.write(&write).await {
        Ok(written) => Ok(written.object.and_then(|o| ClusterPod::from_object(o).ok())),
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

/// A Pod of `workload`'s template named `name`. A StatefulSet's claim templates become claim
/// volumes named as the StatefulSet controller names them, so both `volumeMounts` and
/// `volumeDevices` resolve (a block device takes only a claim); each replaces a template volume of
/// its name, as the controller's `updateStorage` does. A dry run doesn't read the claim. A
/// ServiceAccount that `plan` creates does not exist yet, and the ServiceAccount admission plugin
/// refuses a Pod that names a missing one, so the Pod runs as `default` instead: quotas, limit
/// ranges and Pod Security do not read it.
fn pod(workload: &Added<'_>, name: &str, plan: &Plan) -> Value {
    let mut metadata = workload
        .template
        .get("metadata")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    metadata.insert("name".to_owned(), json!(name));
    let mut spec = workload
        .template
        .get("spec")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let claims: Vec<Value> = workload
        .object
        .raw_after
        .pointer("/spec/volumeClaimTemplates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| c.pointer("/metadata/name"))
        .filter_map(Value::as_str)
        .map(|claim| {
            json!({"name": claim, "persistentVolumeClaim": {"claimName": format!("{claim}-{name}")}})
        })
        .collect();
    if let Some(spec) = spec.as_object_mut() {
        let account = spec
            .get("serviceAccountName")
            .or_else(|| spec.get("serviceAccount"))
            .and_then(Value::as_str);
        if account.is_some_and(|account| creates_account(plan, account)) {
            spec.remove("serviceAccountName");
            spec.remove("serviceAccount");
        }
    }
    if !claims.is_empty() {
        let volumes = spec
            .as_object_mut()
            .map(|s| s.entry("volumes").or_insert_with(|| json!([])));
        if let Some(Value::Array(volumes)) = volumes {
            volumes.retain(|v| !claims.iter().any(|c| c.get("name") == v.get("name")));
            volumes.extend(claims);
        }
    }
    json!({"apiVersion": "v1", "kind": "Pod", "metadata": metadata, "spec": spec})
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
    use super::*;

    fn planned(kind: &str, name: &str, body: &Value) -> Value {
        json!({
            "target": {"apiVersion": if kind == "ServiceAccount" { "v1" } else { "apps/v1" },
                       "kind": kind, "plural": format!("{}s", kind.to_lowercase()), "name": name},
            "changes": [], "owners": [], "write": {"type": "create", "body": body}
        })
    }

    fn account_of(plan: &Plan) -> Option<String> {
        let template = json!({"spec": {"serviceAccountName": "worker", "containers": []}});
        let workload = Added {
            object: &plan.objects[0],
            template: &template,
            pods: 1,
            change: Some(0),
        };
        pod(&workload, "web-dry-run", plan)
            .pointer("/spec/serviceAccountName")
            .and_then(Value::as_str)
            .map(str::to_owned)
    }

    #[test]
    fn a_service_account_the_plan_creates_is_left_out_of_the_dry_run() {
        let with: Plan = serde_json::from_value(json!({
            "objects": [planned("Deployment", "web", &json!({})),
                        planned("ServiceAccount", "worker", &json!({}))],
            "impact": [], "warnings": []
        }))
        .unwrap();
        assert_eq!(account_of(&with), None);
        let without: Plan = serde_json::from_value(json!({
            "objects": [planned("Deployment", "web", &json!({}))],
            "impact": [], "warnings": []
        }))
        .unwrap();
        assert_eq!(account_of(&without).as_deref(), Some("worker"));
    }

    #[test]
    fn a_claim_template_becomes_a_claim_volume_in_place_of_the_template_volume_of_its_name() {
        let mut plan: Plan = serde_json::from_value(json!({
            "objects": [planned("StatefulSet", "db", &json!({}))],
            "impact": [], "warnings": []
        }))
        .unwrap();
        plan.objects[0].raw_after =
            json!({"spec": {"volumeClaimTemplates": [{"metadata": {"name": "data"}}]}});
        let template = json!({"spec": {"containers": [], "volumes": [
            {"name": "data", "configMap": {"name": "seed"}},
            {"name": "tmp", "emptyDir": {}}
        ]}});
        let workload = Added {
            object: &plan.objects[0],
            template: &template,
            pods: 1,
            change: Some(0),
        };
        let pod = pod(&workload, "db-dry-run", &plan);
        assert_eq!(
            pod.pointer("/spec/volumes").unwrap(),
            &json!([
                {"name": "tmp", "emptyDir": {}},
                {"name": "data", "persistentVolumeClaim": {"claimName": "data-db-dry-run"}}
            ])
        );
    }
}
