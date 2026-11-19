//! The planner for Pod Security [k8s-pss]: the level a workload's pods meet.
//!
//! `restricted` sets, on the pod template, `runAsNonRoot: true` and a
//! `RuntimeDefault` seccomp profile unless it names a confined one, and on every container
//! `allowPrivilegeEscalation: false` and `capabilities.drop: [ALL]`; it removes a container's
//! `runAsNonRoot: false` and `Unconfined` seccomp profile, which would override the pod's. It is
//! refused when the template still breaks the level afterwards (`runAsUser: 0`, a `hostPath`
//! volume, an added capability), which only an edit of those fields can change. `baseline`
//! removes `runAsNonRoot: true` and the dropped `ALL`, which stop an image that runs as root from
//! starting or from using the capabilities a root process expects; it keeps the seccomp profile
//! and `allowPrivilegeEscalation: false`, which do not.
//!
//! A project's Namespace enforces `baseline` (`crate::project`), so either profile is admitted;
//! an administrator may label it `restricted` to refuse every pod that breaks that level.
//!
//! [k8s-pss]: https://kubernetes.io/docs/concepts/security/pod-security-standards/

use k8s_openapi::api::core::v1::PodSpec;
use serde_json::{Map, Value, json};
use zeabur_graph::fields::Step;
use zeabur_graph::pod_security;
use zeabur_model::{PodSecurityLevel, WorkloadKind, WorkloadRef};

use crate::json::{add, pointer, remove, replace, test};
use crate::plan::{
    ErrorKind, PlanError, Planner, error, pod_spec_steps, workload_json, workload_target,
};

/// A `securityContext` to write: where, who may own it, and its value before and after.
struct Edit {
    pointer: String,
    name_test: Option<(String, String)>,
    steps: Vec<Step>,
    shown: String,
    before: Option<Value>,
    after: Option<Value>,
}

fn object(value: Option<&Value>) -> Map<String, Value> {
    value
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn non_empty(map: Map<String, Value>) -> Option<Value> {
    (!map.is_empty()).then_some(Value::Object(map))
}

fn seccomp(map: &Map<String, Value>) -> Option<&str> {
    map.get("seccompProfile")
        .and_then(|p| p.get("type"))
        .and_then(Value::as_str)
}

fn pod_context(before: Option<&Value>, profile: PodSecurityLevel) -> Option<Value> {
    let mut map = object(before);
    if profile == PodSecurityLevel::Restricted {
        map.insert("runAsNonRoot".to_owned(), json!(true));
        if !matches!(seccomp(&map), Some("RuntimeDefault" | "Localhost")) {
            map.insert(
                "seccompProfile".to_owned(),
                json!({"type": "RuntimeDefault"}),
            );
        }
    } else if map.get("runAsNonRoot") == Some(&json!(true)) {
        map.remove("runAsNonRoot");
    }
    non_empty(map)
}

fn container_context(before: Option<&Value>, profile: PodSecurityLevel) -> Option<Value> {
    let mut map = object(before);
    let mut capabilities = object(map.get("capabilities"));
    let mut drop: Vec<Value> = capabilities
        .get("drop")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let all = json!("ALL");
    if profile == PodSecurityLevel::Restricted {
        map.insert("allowPrivilegeEscalation".to_owned(), json!(false));
        if !drop.contains(&all) {
            drop = vec![all];
        }
        if map.get("runAsNonRoot") == Some(&json!(false)) {
            map.remove("runAsNonRoot");
        }
        if seccomp(&map) == Some("Unconfined") {
            map.remove("seccompProfile");
        }
    } else {
        drop.retain(|d| d != &all);
        if map.get("runAsNonRoot") == Some(&json!(true)) {
            map.remove("runAsNonRoot");
        }
    }
    if drop.is_empty() {
        capabilities.remove("drop");
    } else {
        capabilities.insert("drop".to_owned(), Value::Array(drop));
    }
    match non_empty(capabilities) {
        Some(c) => map.insert("capabilities".to_owned(), c),
        None => map.remove("capabilities"),
    };
    non_empty(map)
}

/// The `securityContext` edits of the pod and each of its containers, `spec` being the pod
/// spec at `spec_pointer` of a workload of `kind`.
fn edits(
    spec: &Value,
    kind: WorkloadKind,
    spec_pointer: &str,
    profile: PodSecurityLevel,
) -> Vec<Edit> {
    let before = spec.get("securityContext").cloned();
    let mut pod_steps = pod_spec_steps(kind);
    pod_steps.push(Step::field("securityContext"));
    let mut edits = vec![Edit {
        pointer: format!("{spec_pointer}/securityContext"),
        name_test: None,
        steps: pod_steps,
        shown: "the pod's securityContext".to_owned(),
        after: pod_context(before.as_ref(), profile),
        before,
    }];
    for list in ["initContainers", "containers"] {
        let containers = spec
            .get(list)
            .and_then(Value::as_array)
            .into_iter()
            .flatten();
        for (position, container) in containers.enumerate() {
            let name = container
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let at = format!("{spec_pointer}/{list}/{position}");
            let before = container.get("securityContext").cloned();
            let mut steps = pod_spec_steps(kind);
            steps.extend([
                Step::field(list),
                Step::named(name),
                Step::field("securityContext"),
            ]);
            edits.push(Edit {
                pointer: format!("{at}/securityContext"),
                name_test: Some((format!("{at}/name"), name.to_owned())),
                steps,
                shown: format!("securityContext of container {name}"),
                after: container_context(before.as_ref(), profile),
                before,
            });
        }
    }
    edits
}

fn spec_tokens(kind: WorkloadKind) -> Vec<&'static str> {
    let mut tokens = kind.pod_template_path().to_vec();
    tokens.push("spec");
    tokens
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "the planner's change kinds are split across modules by area"
)]
impl Planner<'_> {
    pub(crate) fn security_profile(
        &mut self,
        reference: &WorkloadRef,
        profile: PodSecurityLevel,
    ) -> Result<(), PlanError> {
        if profile == PodSecurityLevel::Privileged {
            return Err(error(
                ErrorKind::BadRequest,
                "unknown_profile",
                "A service's pods meet restricted or baseline; edit its YAML for anything else.",
            ));
        }
        let found = self.editable_workload(reference)?;
        let kind = found.kind();
        let index = self.working(workload_target(found), workload_json(found)?);
        let spec_pointer = pointer(&spec_tokens(kind));
        let spec = self
            .current(index)
            .pointer(&spec_pointer)
            .cloned()
            .unwrap_or_else(|| json!({}));
        let mut edits = edits(&spec, kind, &spec_pointer, profile);
        edits.retain(|e| e.before != e.after);
        if edits.is_empty() {
            return Err(error(
                ErrorKind::BadRequest,
                "unchanged",
                format!(
                    "{} {} already has the {} settings.",
                    reference.kind.as_str(),
                    reference.name,
                    profile.as_str()
                ),
            ));
        }
        let mut operations = Vec::new();
        for edit in &edits {
            self.claim(index, &edit.steps, &edit.shown)?;
            if let Some((path, name)) = &edit.name_test {
                operations.push(test(path, json!(name)));
            }
            operations.extend(match (&edit.before, &edit.after) {
                (Some(before), Some(value)) => vec![
                    test(&edit.pointer, before.clone()),
                    replace(&edit.pointer, value.clone()),
                ],
                (None, Some(value)) => vec![add(&edit.pointer, value.clone())],
                (Some(before), None) => {
                    vec![test(&edit.pointer, before.clone()), remove(&edit.pointer)]
                }
                (None, None) => Vec::new(),
            });
        }
        self.impact.insert(reference.clone());
        self.patch(index, operations)?;
        if profile == PodSecurityLevel::Restricted {
            let spec: Option<PodSpec> = self
                .current(index)
                .pointer(&spec_pointer)
                .and_then(|s| serde_json::from_value(s.clone()).ok());
            let left: Vec<&str> = spec
                .as_ref()
                .map_or_else(Vec::new, pod_security::breaches)
                .iter()
                .map(|b| b.field)
                .collect();
            if !left.is_empty() {
                return Err(error(
                    ErrorKind::Conflict,
                    "not_restricted",
                    format!(
                        "{} {} still breaks the restricted level through {}; change those in its \
                         YAML.",
                        reference.kind.as_str(),
                        reference.name,
                        left.join(", ")
                    ),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zeabur_model::PodSecurityLevel::{Baseline, Restricted};

    use super::{container_context, pod_context};

    #[test]
    fn restricted_settings_go_in_and_baseline_takes_out_only_what_stops_root() {
        let pod = pod_context(Some(&json!({"fsGroup": 1000})), Restricted).unwrap();
        assert_eq!(
            pod,
            json!({"fsGroup": 1000, "runAsNonRoot": true, "seccompProfile": {"type": "RuntimeDefault"}})
        );
        assert_eq!(
            pod_context(Some(&pod), Baseline).unwrap(),
            json!({"fsGroup": 1000, "seccompProfile": {"type": "RuntimeDefault"}})
        );
        let container = container_context(
            Some(&json!({"runAsNonRoot": false, "capabilities": {"add": ["NET_BIND_SERVICE"], "drop": ["NET_RAW"]}})),
            Restricted,
        )
        .unwrap();
        assert_eq!(
            container,
            json!({"allowPrivilegeEscalation": false,
                   "capabilities": {"add": ["NET_BIND_SERVICE"], "drop": ["ALL"]}})
        );
        assert_eq!(
            container_context(Some(&container), Baseline).unwrap(),
            json!({"allowPrivilegeEscalation": false, "capabilities": {"add": ["NET_BIND_SERVICE"]}})
        );
        assert_eq!(container_context(None, Baseline), None);
        let localhost = json!({"seccompProfile": {"type": "Localhost", "localhostProfile": "p"}});
        assert_eq!(
            pod_context(Some(&localhost), Restricted).unwrap()["seccompProfile"]["type"],
            "Localhost"
        );
    }
}
