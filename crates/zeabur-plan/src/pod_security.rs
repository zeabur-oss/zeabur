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

use k8s_openapi::api::core::v1::{
    Capabilities, PodSecurityContext, PodSpec, SeccompProfile, SecurityContext,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
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

/// `before` edited by `change`, or `before` itself when it is not a valid `T` or `change` leaves
/// it as it is. An empty result is `None`.
fn merged<T>(before: Option<&Value>, change: impl FnOnce(&mut T)) -> Option<Value>
where
    T: Default + PartialEq + Clone + Serialize + DeserializeOwned,
{
    let current: T = match before {
        Some(value) => match serde_json::from_value(value.clone()) {
            Ok(current) => current,
            Err(_) => return before.cloned(),
        },
        None => T::default(),
    };
    let mut edited = current.clone();
    change(&mut edited);
    if edited == current {
        return before.cloned();
    }
    (edited != T::default())
        .then(|| serde_json::to_value(&edited).ok())
        .flatten()
}

const fn seccomp_type(profile: Option<&SeccompProfile>) -> Option<&str> {
    match profile {
        Some(p) => Some(p.type_.as_str()),
        None => None,
    }
}

fn pod_context(before: Option<&Value>, profile: PodSecurityLevel) -> Option<Value> {
    merged(before, |context: &mut PodSecurityContext| {
        if profile == PodSecurityLevel::Restricted {
            context.run_as_non_root = Some(true);
            if !matches!(
                seccomp_type(context.seccomp_profile.as_ref()),
                Some("RuntimeDefault" | "Localhost")
            ) {
                context.seccomp_profile = Some(SeccompProfile {
                    type_: "RuntimeDefault".to_owned(),
                    localhost_profile: None,
                });
            }
        } else if context.run_as_non_root == Some(true) {
            context.run_as_non_root = None;
        }
    })
}

fn container_context(before: Option<&Value>, profile: PodSecurityLevel) -> Option<Value> {
    merged(before, |context: &mut SecurityContext| {
        let mut capabilities = context.capabilities.take().unwrap_or_default();
        let mut drop = capabilities.drop.take().unwrap_or_default();
        let all = "ALL".to_owned();
        if profile == PodSecurityLevel::Restricted {
            context.allow_privilege_escalation = Some(false);
            if !drop.contains(&all) {
                drop = vec![all];
            }
            if context.run_as_non_root == Some(false) {
                context.run_as_non_root = None;
            }
            if seccomp_type(context.seccomp_profile.as_ref()) == Some("Unconfined") {
                context.seccomp_profile = None;
            }
        } else {
            drop.retain(|d| d != &all);
            if context.run_as_non_root == Some(true) {
                context.run_as_non_root = None;
            }
        }
        capabilities.drop = (!drop.is_empty()).then_some(drop);
        context.capabilities = (capabilities != Capabilities::default()).then_some(capabilities);
    })
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
