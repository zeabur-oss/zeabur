//! New objects, from a ready-made object or from a person's own YAML.
//!
//! - **Kinds.** Any namespaced kind the cluster serves, built in or custom, as discovery names it
//!   ([`crate::Input::kinds`]); without discovery, the kinds [`crate::CREATABLE`] lists. A
//!   cluster-scoped kind is refused: a project holds only what is in its namespace.
//! - **Namespace.** The object goes in the project's namespace. A `metadata.namespace` that names
//!   another is refused, not rewritten.
//! - **Never an update.** The write is a `POST` of the whole object, which kube-apiserver refuses
//!   with `409 AlreadyExists` when the name is taken [k8s-create]; the API also refuses a name that
//!   exists when it plans.
//! - **Order.** ConfigMaps, Secrets, claims and service accounts first, then Services, then
//!   workloads, then the rest, so what a workload reads exists when its pods start. There is no
//!   transaction across objects: an apply stops at the first refusal, and what it wrote stays.
//!
//! [k8s-create]: https://kubernetes.io/docs/reference/using-api/api-concepts/#api-verbs

use serde_json::{Map, Value, json};
use zeabur_graph::manifest::HIDDEN;

use crate::change::{Change, Plan, Target, Write};
use crate::plan::{CREATABLE, ErrorKind, PlanError, Planner, Working, error};

/// A kind the cluster serves, from discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServedKind {
    /// The API group; empty for the core group.
    pub group: String,
    /// The kind (`Deployment`).
    pub kind: String,
    /// The plural in its path (`deployments`).
    pub plural: String,
    /// It lives in a namespace.
    pub namespaced: bool,
}

/// The object a `create` makes, read from its YAML where it has one; `None` for another change.
///
/// # Errors
///
/// When the YAML does not read, or holds no object or more than one.
#[must_use]
pub fn created_object(change: &Change) -> Option<Result<Value, PlanError>> {
    let Change::Create { object, yaml } = change else {
        return None;
    };
    Some(
        yaml.as_deref()
            .map_or_else(|| Ok(object.clone()), read_yaml),
    )
}

fn read_yaml(yaml: &str) -> Result<Value, PlanError> {
    let objects: Vec<Value> = serde_saphyr::from_multiple(yaml).map_err(|e| {
        let mut refused = error(
            ErrorKind::BadRequest,
            "invalid_yaml",
            "The YAML does not read: check the indentation and the quotes on this line.",
        );
        refused.line = e.location().map(|l| l.line());
        refused
    })?;
    let mut objects = objects.into_iter().filter(|o| !o.is_null());
    match (objects.next(), objects.next()) {
        (Some(object), None) => Ok(object),
        (None, _) => Err(error(
            ErrorKind::BadRequest,
            "no_object",
            "The YAML holds no object.",
        )),
        (Some(_), Some(_)) => Err(error(
            ErrorKind::BadRequest,
            "several_objects",
            "One create makes one object; send each YAML document as a create of its own.",
        )),
    }
}

/// `(plural, namespaced)` of `kind` in the API group of `api_version`.
///
/// As discovery names it, or as [`CREATABLE`] does when discovery named nothing. The version is
/// the object's own: the plural is the same in every version of a group.
#[must_use]
pub fn resolve(kinds: &[ServedKind], api_version: &str, kind: &str) -> Option<(String, bool)> {
    let group = api_version.rsplit_once('/').map_or("", |(group, _)| group);
    if kinds.is_empty() {
        return CREATABLE
            .iter()
            .find(|(v, k, _)| *v == api_version && *k == kind)
            .map(|(_, _, plural)| ((*plural).to_owned(), true));
    }
    kinds
        .iter()
        .find(|k| k.group == group && k.kind == kind)
        .map(|k| (k.plural.clone(), k.namespaced))
}

/// Where a kind comes in the order of creation.
fn rank(api_version: &str, kind: &str) -> u8 {
    let group = api_version.rsplit_once('/').map_or("", |(group, _)| group);
    match (group, kind) {
        ("", "ConfigMap" | "Secret" | "PersistentVolumeClaim" | "ServiceAccount") => 0,
        ("", "Service") => 1,
        ("apps", "Deployment" | "StatefulSet" | "DaemonSet" | "ReplicaSet")
        | ("batch", "Job" | "CronJob")
        | ("", "Pod" | "ReplicationController") => 2,
        _ => 3,
    }
}

/// The indices of `changes` in the order they are planned: every other change as asked, then
/// the creations in dependency order, each rank in the order written.
pub(crate) fn order(changes: &[Change]) -> Vec<usize> {
    let mut keyed: Vec<(u8, usize)> = changes
        .iter()
        .enumerate()
        .map(|(index, change)| {
            let rank = match created_object(change) {
                None => 0,
                Some(Ok(object)) => {
                    let text = |path: &str| {
                        object
                            .pointer(path)
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned()
                    };
                    1 + rank(&text("/apiVersion"), &text("/kind"))
                }
                Some(Err(_)) => 1,
            };
            (rank, index)
        })
        .collect();
    keyed.sort_unstable();
    keyed.into_iter().map(|(_, index)| index).collect()
}

/// Whether `name` can be the last segment of an object's path, as kube-apiserver's
/// `IsValidPathSegmentName` asks; each kind's own rules are kube-apiserver's to check.
fn path_segment(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name.len() <= 253
        && !name.contains(['/', '%'])
}

/// The metadata a person may set on a new object; the rest is the server's.
const KEPT_METADATA: [&str; 4] = ["labels", "annotations", "finalizers", "ownerReferences"];

#[expect(
    clippy::multiple_inherent_impl,
    reason = "the planner's change kinds are split across modules by area"
)]
impl Planner<'_> {
    /// A new object in the project's namespace, created with a `POST` to its collection.
    pub(crate) fn create(&mut self, change: &Change, index: usize) -> Result<(), PlanError> {
        let Some(object) = created_object(change) else {
            return Ok(());
        };
        let object = object?;
        let target = self.created_target(&object)?;
        let namespace = self.input.snapshot.namespace.clone();
        let mut metadata = Map::new();
        metadata.insert("name".to_owned(), json!(target.name));
        metadata.insert("namespace".to_owned(), json!(namespace));
        for key in KEPT_METADATA {
            if let Some(value) = object.pointer(&format!("/metadata/{key}")) {
                metadata.insert(key.to_owned(), value.clone());
            }
        }
        let mut body =
            json!({"apiVersion": target.api_version, "kind": target.kind, "metadata": metadata});
        if let (Some(fields), Some(out)) = (object.as_object(), body.as_object_mut()) {
            for (key, value) in fields {
                if !matches!(key.as_str(), "apiVersion" | "kind" | "metadata" | "status") {
                    out.insert(key.clone(), value.clone());
                }
            }
        }
        self.created.push((target.clone(), index));
        self.objects.push(Working {
            target,
            original: None,
            current: body,
            operations: Vec::new(),
            owners: Vec::new(),
            deleted: false,
        });
        Ok(())
    }

    /// The object a create makes, once it is known to be one the project may hold.
    fn created_target(&self, object: &Value) -> Result<Target, PlanError> {
        if !object.is_object() {
            return Err(error(
                ErrorKind::BadRequest,
                "not_an_object",
                "The YAML is not an object: it needs apiVersion, kind and metadata.",
            ));
        }
        let text = |path: &str| object.pointer(path).and_then(Value::as_str);
        let required = |path: &str, field: &str| {
            text(path).filter(|v| !v.is_empty()).ok_or_else(|| {
                let mut missing = error(
                    ErrorKind::BadRequest,
                    "missing_field",
                    format!("Every object needs {field}."),
                );
                missing.field = Some(field.to_owned());
                missing
            })
        };
        let api_version = required("/apiVersion", "apiVersion")?;
        let kind = required("/kind", "kind")?;
        let name = required("/metadata/name", "metadata.name")?;
        let Some((plural, namespaced)) = resolve(self.input.kinds, api_version, kind) else {
            let mut refused = error(
                ErrorKind::Invalid,
                "not_creatable",
                format!("The cluster does not serve {kind} ({api_version})."),
            );
            refused.field = Some("kind".to_owned());
            return Err(refused);
        };
        if !namespaced {
            let mut refused = error(
                ErrorKind::Invalid,
                "cluster_scoped",
                format!(
                    "{kind} belongs to the whole cluster; Zeabur creates objects in a project."
                ),
            );
            refused.field = Some("kind".to_owned());
            return Err(refused);
        }
        if !path_segment(name) {
            let mut refused = error(
                ErrorKind::BadRequest,
                "invalid_name",
                format!("{name:?} is not a valid {kind} name."),
            );
            refused.field = Some("metadata.name".to_owned());
            return Err(refused);
        }
        let namespace = &self.input.snapshot.namespace;
        if let Some(other) = text("/metadata/namespace").filter(|n| n != namespace) {
            let mut refused = error(
                ErrorKind::Invalid,
                "other_namespace",
                format!("{kind} {name} names namespace {other}, not the project's ({namespace})."),
            );
            refused.field = Some("metadata.namespace".to_owned());
            return Err(refused);
        }
        let target = Target {
            api_version: api_version.to_owned(),
            kind: kind.to_owned(),
            plural,
            name: name.to_owned(),
        };
        if self.objects.iter().any(|o| {
            o.target.name == target.name
                && o.target.plural == target.plural
                && group_of(&o.target.api_version) == group_of(api_version)
        }) {
            let mut refused = error(
                ErrorKind::BadRequest,
                "duplicate_object",
                format!("{kind} {name} is in the request twice."),
            );
            refused.field = Some("metadata.name".to_owned());
            return Err(refused);
        }
        Ok(target)
    }
}

fn group_of(api_version: &str) -> &str {
    api_version.rsplit_once('/').map_or("", |(group, _)| group)
}

impl Plan {
    /// The plan as a person may see it: the values a write puts in a Secret are hidden, as the
    /// objects before and after already show them. The writes themselves are made again from the
    /// request when the plan is applied.
    #[must_use]
    pub fn without_secret_values(mut self) -> Self {
        for object in &mut self.objects {
            if !(object.target.api_version == "v1" && object.target.kind == "Secret") {
                continue;
            }
            match &mut object.write {
                Write::Create { body } => *body = zeabur_graph::manifest::clean(body),
                Write::Patch { operations } => {
                    for operation in operations {
                        let secret =
                            operation
                                .get("path")
                                .and_then(Value::as_str)
                                .is_some_and(|p| {
                                    p.starts_with("/data") || p.starts_with("/stringData")
                                });
                        if let (true, Some(value)) = (secret, operation.get_mut("value")) {
                            hide(value);
                        }
                    }
                }
                Write::Delete { .. } | Write::EphemeralContainers { .. } => {}
            }
        }
        self
    }
}

fn hide(value: &mut Value) {
    match value {
        Value::Object(map) => map.values_mut().for_each(hide),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Array(_) => {
            *value = json!(HIDDEN);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creations_follow_what_they_need() {
        let create = |api_version: &str, kind: &str| Change::Create {
            object: json!({"apiVersion": api_version, "kind": kind, "metadata": {"name": "a"}}),
            yaml: None,
        };
        let changes = [
            create("gateway.networking.k8s.io/v1", "HTTPRoute"),
            create("apps/v1", "Deployment"),
            create("v1", "Service"),
            create("v1", "Secret"),
            create("v1", "ConfigMap"),
            Change::ClusterTools,
        ];
        assert_eq!(order(&changes), vec![5, 3, 4, 2, 1, 0]);
    }

    #[test]
    fn a_yaml_error_names_its_line() {
        let error = read_yaml("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: a\n   x: y\n")
            .unwrap_err();
        assert_eq!(error.code, "invalid_yaml");
        assert!(matches!(error.line, Some(4 | 5)), "{error:?}");
        assert!(!error.detail.contains("x: y"));
    }

    #[test]
    fn one_document_per_create() {
        assert_eq!(
            read_yaml("a: 1\n---\nb: 2\n").unwrap_err().code,
            "several_objects"
        );
        assert_eq!(read_yaml("---\n").unwrap_err().code, "no_object");
        assert_eq!(read_yaml("a: 1\n---\n").unwrap(), json!({"a": 1}));
    }
}
