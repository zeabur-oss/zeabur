//! What a plan reads before it creates objects: the kinds the cluster serves, whether a name is
//! taken, and, once planned, whether kube-apiserver would accept each object (a dry run).

use serde_json::Value;
use zeabur_core::problem::{Kind, Problem, ProblemAt};
use zeabur_kube::{Cluster, ServedResource, WriteRequest};
use zeabur_model::Gap;
use zeabur_plan::{Change, Plan, PlanRequest, ServedKind, Write, created_object, resolve};

use super::refusals::refused;
use crate::error::{ApiError, get_read, read_problem};

/// Whether `request` creates an object from a person's YAML.
pub(super) fn from_yaml(request: &PlanRequest) -> bool {
    request
        .changes
        .iter()
        .any(|c| matches!(c, Change::Create { yaml: Some(_), .. }))
}

/// The kinds discovery names, for a request that creates objects or Traefik middlewares; none
/// otherwise, and the planner knows only the kinds it creates itself.
pub(super) async fn served_kinds(
    cluster: &dyn Cluster,
    request: &PlanRequest,
) -> Result<Vec<ServedKind>, ApiError> {
    let middlewares = request
        .changes
        .iter()
        .any(|c| zeabur_plan::access::middleware_read(c).is_some());
    if !from_yaml(request) && !middlewares {
        return Ok(Vec::new());
    }
    let served = cluster
        .served_resources()
        .await
        .map_err(|e| ApiError::from(e.into_problem("the cluster's APIs")))?;
    Ok(served.iter().map(served_kind).collect())
}

fn served_kind(served: &ServedResource) -> ServedKind {
    ServedKind {
        group: served.group.clone(),
        kind: served.kind.clone(),
        plural: served.resource.clone(),
        namespaced: served.namespaced,
    }
}

/// Refuses a `create` whose object already exists: a create never updates. A name the person may
/// not read is left to the write, which kube-apiserver refuses with `409` when it is taken.
pub(super) async fn refuse_existing(
    cluster: &dyn Cluster,
    project: &str,
    request: &PlanRequest,
    kinds: &[ServedKind],
) -> Result<(), ApiError> {
    for (index, change) in request.changes.iter().enumerate() {
        let Some(Ok(object)) = created_object(change) else {
            continue;
        };
        let text = |path: &str| object.pointer(path).and_then(Value::as_str);
        let (Some(api_version), Some(kind), Some(name)) =
            (text("/apiVersion"), text("/kind"), text("/metadata/name"))
        else {
            continue;
        };
        // The planner refuses kinds it does not create; nothing to look up for them.
        let Some((plural, true)) = resolve(kinds, api_version, kind) else {
            continue;
        };
        let (group, version) = api_version.rsplit_once('/').unwrap_or(("", api_version));
        let resource = zeabur_kube::ResourceRef::new(group, version, &plural, kind);
        let found = match cluster.get_object(&resource, Some(project), name).await {
            Ok(found) => found.is_some(),
            Err(error) if error.is_not_found() || error.is_refused() => false,
            Err(other) => {
                return Err(read_problem(
                    other,
                    &format!("{kind} {name}"),
                    get_read(group, &plural, Some(project), name),
                ));
            }
        };
        if found {
            return Err(ApiError::from(Problem {
                at: Some(ProblemAt {
                    change: index,
                    line: None,
                    field: Some("metadata.name".to_owned()),
                }),
                ..Problem::new(
                    Kind::Conflict,
                    format!("{kind} {name} already exists in project {project}."),
                )
            }));
        }
    }
    Ok(())
}

/// Sends every write of a plan that creates objects from YAML as a dry run, as the person: what
/// their rights, validation, quotas and admission policies would refuse is refused now, at the
/// object it is about.
pub(super) async fn dry_run(
    cluster: &dyn Cluster,
    project: &str,
    plan: &Plan,
) -> Result<(), ApiError> {
    for object in &plan.objects {
        let write = WriteRequest {
            dry_run: true,
            ..super::write_request(project, &object.target, &object.write)
        };
        if let Err(error) = cluster.write(&write).await {
            let mut problem = refused(cluster, project, &object.target, &write, error).await;
            if let Some(change) = object.change {
                problem.at = Some(ProblemAt {
                    change,
                    line: None,
                    field: problem.fields.first().map(|f| f.field.clone()),
                });
            }
            return Err(ApiError::from(without_secret_values(problem, plan)));
        }
    }
    Ok(())
}

/// `problem` without any value a Secret in `plan` is written with, as written or base64: a
/// message from kube-apiserver or a policy may quote the object.
pub(super) fn without_secret_values(problem: Problem, plan: &Plan) -> Problem {
    let mut values: Vec<String> = Vec::new();
    for object in &plan.objects {
        let (true, Write::Create { body }) = (
            object.target.api_version == "v1" && object.target.kind == "Secret",
            &object.write,
        ) else {
            continue;
        };
        for key in ["data", "stringData"] {
            for value in body
                .get(key)
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|m| m.values())
                .filter_map(Value::as_str)
            {
                values.push(value.to_owned());
                if key == "data" {
                    if let Some(decoded) = decode(value) {
                        values.push(decoded);
                    }
                } else {
                    values.push(encode(value));
                }
            }
        }
    }
    values.retain(|v| !v.is_empty());
    if values.is_empty() {
        return problem;
    }
    values.sort_by_key(|v| std::cmp::Reverse(v.len()));
    let Ok(mut json) = serde_json::to_value(&problem) else {
        return problem;
    };
    scrub(&mut json, &values);
    serde_json::from_value(json).unwrap_or(problem)
}

fn scrub(value: &mut Value, secrets: &[String]) {
    match value {
        Value::String(text) => {
            for secret in secrets {
                if text.contains(secret.as_str()) {
                    *text = text.replace(secret.as_str(), zeabur_graph::manifest::HIDDEN);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| scrub(v, secrets)),
        Value::Object(map) => map.values_mut().for_each(|v| scrub(v, secrets)),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn decode(value: &str) -> Option<String> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value)
        .ok()?;
    String::from_utf8(bytes).ok()
}

fn encode(value: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(value)
}

/// What the created workloads miss, by the rules that read one object alone: resource requests
/// and limits, and health checks.
pub(super) fn gaps(project: &str, plan: &Plan) -> Vec<Gap> {
    let mut snapshot = zeabur_graph::Snapshot {
        namespace: project.to_owned(),
        ..zeabur_graph::Snapshot::default()
    };
    let mut any = false;
    for object in &plan.objects {
        if let Write::Create { body } = &object.write
            && object.change.is_some()
        {
            any |= snapshot.insert(body.clone()).unwrap_or(false);
        }
    }
    if !any {
        return Vec::new();
    }
    let projection = zeabur_graph::project(&snapshot);
    let traits = zeabur_gaps::ClusterTraits::default();
    zeabur_gaps::evaluate(
        &[&zeabur_gaps::ResourceLimits, &zeabur_gaps::HealthChecks],
        &zeabur_gaps::Input {
            project: &projection,
            snapshot: &snapshot,
            http_routes: &[],
            services_readable: true,
            cluster: &traits,
        },
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_problem_never_quotes_a_secret_value() {
        let plan: Plan = serde_json::from_value(json!({
            "objects": [{
                "target": {"apiVersion": "v1", "kind": "Secret", "plural": "secrets", "name": "env"},
                "changes": [], "owners": [],
                "write": {"type": "create", "body": {
                    "stringData": {"TOKEN": "hunter22"}, "data": {"B": "c2VjcmV0LWI="}
                }}
            }],
            "impact": [], "warnings": []
        }))
        .unwrap();
        let problem = Problem::new(
            Kind::PolicyDenied,
            "TOKEN is hunter22 (aHVudGVyMjI=); B is secret-b",
        );
        let problem = without_secret_values(problem, &plan);
        assert_eq!(
            problem.detail,
            "TOKEN is (hidden) ((hidden)); B is (hidden)"
        );
    }
}
