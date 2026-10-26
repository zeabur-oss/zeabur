//! The planner for the project's Namespace: creating it, adopting it, letting it go, and its
//! details.
//!
//! - **Creating** writes a new Namespace with the project label, the Pod Security labels and, when
//!   given, the display name. When the cluster labels its platform namespaces
//!   ([`zeabur_model::kernel::isolation::PLATFORM_LABEL`]), it also writes the project's
//!   `NetworkPolicy` ([`zeabur_model::kernel::isolation`]); without the label, Zeabur cannot tell
//!   which namespaces the ingress controller and the operators run in. The Pod Security labels
//!   enforce, audit and warn at `baseline` [k8s-psa-labels], whatever the cluster's default, so a
//!   new project's pods are held to the same level on every cluster: an image that runs as root
//!   starts, and a pod that reaches the host does not. Each service can still meet `restricted`
//!   (the `pod_security` module). kube-apiserver refuses the Namespace (`409 AlreadyExists`) if the
//!   name was taken since the plan, which refuses a name that exists when it plans. It also writes
//!   the `default` `ServiceAccount` with `automountServiceAccountToken: false`, so a pod that names
//!   no ServiceAccount gets no token. A workload that calls the Kubernetes API names a
//!   ServiceAccount of its own.
//! - **Adopting** adds one label, `zeabur.com/project: "true"`; **letting go** removes it. Nothing
//!   else on the Namespace changes, and nothing in it: a namespace that is no longer adopted keeps
//!   every object, and is only no longer shown as a project.
//! - **Details** are three annotations (display name, description, icon), set, replaced or removed
//!   one by one.
//!
//! Like every change to an existing object, each is one JSON Patch pinned to the Namespace's
//! `resourceVersion` (a Namespace has no `metadata.generation`), with every value it replaces or
//! removes tested first; JSON Patch `add` of one map key leaves the other keys as they are
//! [rfc6902-add]. A label or annotation another tool owns (Argo CD, Flux, Helm) is refused, as
//! for any field ([`crate::fields::reapplying_tool`]): the tool would restore it.
//!
//! The namespaces Kubernetes keeps for itself (`kube-` prefix,
//! [`zeabur_model::kernel::names::is_system_namespace`]) are never adopted, nor the one of node
//! shells ([`NODE_SHELL_NAMESPACE`]), which only cluster administrators may use.
//!
//! [rfc6902-add]: https://www.rfc-editor.org/rfc/rfc6902#section-4.1
//! [k8s-psa-labels]: https://kubernetes.io/docs/concepts/security/pod-security-admission/#pod-security-admission-labels-for-namespaces

use k8s_openapi::api::core::v1::ServiceAccount;
use k8s_openapi::api::networking::v1::NetworkPolicy;
use serde_json::{Value, json};
use zeabur_model::kernel::isolation::{PROJECT_POLICY, project_policy};
use zeabur_model::kernel::names::{Conventions, NODE_SHELL_NAMESPACE, is_system_namespace};
use zeabur_model::{DESCRIPTION_MAX, DISPLAY_NAME_MAX, PodSecurityLevel, ProjectIcon};

use crate::change::Target;
use crate::checks::is_dns_label;
use crate::fields::Step;
use crate::json::{add, pointer, remove, replace, test};
use crate::plan::{ErrorKind, PlanError, Planner, Working, error};

fn node_shell_namespace(name: &str) -> PlanError {
    error(
        ErrorKind::SystemNamespace,
        "system_namespace",
        format!(
            "{name} holds node shells, which only cluster administrators may use; it cannot be a \
             project."
        ),
    )
}

/// The ServiceAccount every namespace gets; a project's has no token.
pub const DEFAULT_ACCOUNT: &str = "default";

/// The value of the project label.
const ADOPTED: &str = "true";

/// A new project's Namespace: the project label, the Pod Security labels at `level`, and the
/// display name when given.
#[must_use]
pub fn project_namespace(
    conventions: &Conventions,
    name: &str,
    display_name: Option<&str>,
    level: PodSecurityLevel,
) -> Value {
    let level = level.as_str();
    let mut metadata = json!({
        "name": name,
        "labels": {
            conventions.project_label.clone(): ADOPTED,
            PodSecurityLevel::ENFORCE: level,
            PodSecurityLevel::AUDIT: level,
            PodSecurityLevel::WARN: level,
        },
    });
    if let (Some(display_name), Some(map)) = (display_name, metadata.as_object_mut()) {
        map.insert(
            "annotations".to_owned(),
            json!({ conventions.display_name_annotation.clone(): display_name }),
        );
    }
    json!({"apiVersion": "v1", "kind": "Namespace", "metadata": metadata})
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "the planner's change kinds are split across modules by area"
)]
impl Planner<'_> {
    /// The working copy of the Namespace, and its name.
    fn namespace_object(&mut self) -> Result<(usize, String), PlanError> {
        let object = self.input.namespace.ok_or_else(|| {
            error(
                ErrorKind::NotFound,
                "unknown_namespace",
                "The namespace could not be read.",
            )
        })?;
        let name = object
            .pointer("/metadata/name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let index = self.working(Target::namespace(&name), object.clone());
        Ok((index, name))
    }

    fn adopted(&self, index: usize) -> bool {
        let conventions = self.conventions;
        self.current(index)
            .pointer(&pointer(&[
                "metadata",
                "labels",
                &conventions.project_label,
            ]))
            .is_some()
    }

    pub(crate) fn create_project(&mut self, display_name: Option<&str>) -> Result<(), PlanError> {
        let conventions = self.conventions;
        let name = self.input.snapshot.namespace.clone();
        if !is_dns_label(&name) {
            return Err(error(
                ErrorKind::Invalid,
                "invalid_project_name",
                format!(
                    "{name:?} cannot name a project: use at most 63 lowercase letters, digits \
                     and hyphens, starting and ending with a letter or digit."
                ),
            ));
        }
        if is_system_namespace(&name) {
            return Err(error(
                ErrorKind::SystemNamespace,
                "system_namespace",
                format!(
                    "Names starting with kube- are kept for Kubernetes; {name} cannot be a project."
                ),
            ));
        }
        if name == NODE_SHELL_NAMESPACE {
            return Err(node_shell_namespace(&name));
        }
        if self.input.namespace.is_some() {
            return Err(error(
                ErrorKind::Conflict,
                "namespace_exists",
                format!("A namespace named {name} already exists; adopt it instead."),
            ));
        }
        let display_name = wanted(display_name);
        validate(
            display_name,
            DISPLAY_NAME_MAX,
            false,
            "display_name",
            "display name",
        )?;
        let namespace =
            project_namespace(conventions, &name, display_name, PodSecurityLevel::Baseline);
        self.objects.push(Working {
            target: Target::namespace(&name),
            original: None,
            current: namespace,
            operations: Vec::new(),
            owners: Vec::new(),
            deleted: false,
        });
        self.objects.push(Working {
            target: Target::of::<ServiceAccount>(DEFAULT_ACCOUNT),
            original: None,
            current: json!({
                "apiVersion": "v1", "kind": "ServiceAccount",
                "metadata": {"name": DEFAULT_ACCOUNT, "namespace": name},
                "automountServiceAccountToken": false,
            }),
            operations: Vec::new(),
            owners: Vec::new(),
            deleted: false,
        });
        if self.input.platform_namespaces {
            self.objects.push(Working {
                target: Target::of::<NetworkPolicy>(PROJECT_POLICY),
                original: None,
                current: project_policy(&name),
                operations: Vec::new(),
                owners: Vec::new(),
                deleted: false,
            });
        }
        Ok(())
    }

    pub(crate) fn adopt(&mut self) -> Result<(), PlanError> {
        let conventions = self.conventions;
        let (index, name) = self.namespace_object()?;
        if is_system_namespace(&name) {
            return Err(error(
                ErrorKind::SystemNamespace,
                "system_namespace",
                format!(
                    "{name} is a namespace Kubernetes keeps for its own components; it cannot \
                     become a project."
                ),
            ));
        }
        if name == NODE_SHELL_NAMESPACE {
            return Err(node_shell_namespace(&name));
        }
        if self.adopted(index) {
            return Err(error(
                ErrorKind::Conflict,
                "already_adopted",
                format!("{name} is already a project."),
            ));
        }
        self.claim(
            index,
            &steps("labels", &conventions.project_label),
            &shown("labels", &conventions.project_label),
        )?;
        let operations = set_key(
            &self.current(index),
            "labels",
            &conventions.project_label,
            ADOPTED,
        );
        self.patch(index, operations)
    }

    pub(crate) fn unadopt(&mut self) -> Result<(), PlanError> {
        let conventions = self.conventions;
        let (index, name) = self.namespace_object()?;
        let path = pointer(&["metadata", "labels", &conventions.project_label]);
        let Some(value) = self.current(index).pointer(&path).cloned() else {
            return Err(error(
                ErrorKind::Conflict,
                "not_adopted",
                format!("{name} is not a project."),
            ));
        };
        self.claim(
            index,
            &steps("labels", &conventions.project_label),
            &shown("labels", &conventions.project_label),
        )?;
        self.patch(index, vec![test(&path, value), remove(&path)])
    }

    pub(crate) fn details(
        &mut self,
        display_name: Option<&str>,
        description: Option<&str>,
        icon: Option<ProjectIcon>,
    ) -> Result<(), PlanError> {
        let conventions = self.conventions;
        let (index, name) = self.namespace_object()?;
        if !self.adopted(index) {
            return Err(error(
                ErrorKind::Conflict,
                "not_adopted",
                format!("{name} is not a project; adopt it first."),
            ));
        }
        let display_name = wanted(display_name);
        let description = wanted(description);
        validate(
            display_name,
            DISPLAY_NAME_MAX,
            false,
            "display_name",
            "display name",
        )?;
        validate(
            description,
            DESCRIPTION_MAX,
            true,
            "description",
            "description",
        )?;
        let mut changed = false;
        for (key, value) in [
            (conventions.display_name_annotation.as_ref(), display_name),
            (conventions.description_annotation.as_ref(), description),
            (
                conventions.icon_annotation.as_ref(),
                icon.map(ProjectIcon::as_str),
            ),
        ] {
            let current = self.current(index);
            let path = pointer(&["metadata", "annotations", key]);
            let before = current.pointer(&path).cloned();
            if before.as_ref().and_then(Value::as_str) == value {
                continue;
            }
            self.claim(
                index,
                &steps("annotations", key),
                &shown("annotations", key),
            )?;
            let operations = match (before, value) {
                (None, None) => continue,
                (Some(before), None) => vec![test(&path, before), remove(&path)],
                (Some(before), Some(value)) => {
                    vec![test(&path, before), replace(&path, json!(value))]
                }
                (None, Some(value)) => set_key(&current, "annotations", key, value),
            };
            self.patch(index, operations)?;
            changed = true;
        }
        if changed {
            Ok(())
        } else {
            Err(error(
                ErrorKind::BadRequest,
                "unchanged",
                format!("{name} already has these details."),
            ))
        }
    }
}

/// A detail as asked: trimmed, and absent when blank.
fn wanted(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|v| !v.is_empty())
}

/// Refuses a detail longer than `max` characters, or with control characters (a description
/// may have line breaks).
fn validate(
    value: Option<&str>,
    max: usize,
    lines: bool,
    code: &str,
    what: &str,
) -> Result<(), PlanError> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.chars().count() > max {
        return Err(error(
            ErrorKind::BadRequest,
            &format!("{code}_too_long"),
            format!("The {what} is longer than {max} characters."),
        ));
    }
    if value
        .chars()
        .any(|c| c.is_control() && !(lines && c == '\n'))
    {
        return Err(error(
            ErrorKind::BadRequest,
            &format!("{code}_not_text"),
            format!("The {what} has characters that are not text."),
        ));
    }
    Ok(())
}

/// The `managedFields` steps of one label or annotation.
pub(crate) fn steps(map: &str, key: &str) -> Vec<Step> {
    vec![Step::field("metadata"), Step::field(map), Step::field(key)]
}

/// The field as a plan names it.
pub(crate) fn shown(map: &str, key: &str) -> String {
    format!("metadata.{map}.{key}")
}

/// The operations adding `key: value` to the metadata map `map` (`labels`, `annotations`), which
/// may not exist yet.
pub(crate) fn set_key(object: &Value, map: &str, key: &str, value: &str) -> Vec<Value> {
    if object.pointer(&pointer(&["metadata", map])).is_some() {
        vec![add(&pointer(&["metadata", map, key]), json!(value))]
    } else {
        vec![add(&pointer(&["metadata", map]), json!({ key: value }))]
    }
}
