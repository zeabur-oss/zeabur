//! A YAML edit as a JSON Patch: what differs between the object as read and the object as the
//! person edited it, each change guarded by a `test` of the value it replaces [rfc6902].
//!
//! Fields the server owns (`status`, `metadata.resourceVersion`, `uid`, `generation`,
//! `creationTimestamp`, `managedFields`, …) and the annotations controllers and tools keep for
//! themselves ([`MANAGED_ANNOTATIONS`]) are left as they are, whether the edit keeps, changes or
//! drops them: the YAML a person edits leaves them out. Lists merged by key
//! ([`crate::json::list_key`]: containers, env, mounts, ports, …) are compared entry by entry, so
//! editing one container does not touch the others. Other lists are replaced whole, and so is a
//! keyed list whose kept entries change order or that gains an entry before one it keeps.
//!
//! [rfc6902]: https://www.rfc-editor.org/rfc/rfc6902

pub use engawa_graph::manifest::LAST_APPLIED;
use serde_json::{Map, Value};

use crate::fields::Step;
use crate::json::{add, escape, list_key, remove, replace, test};

/// Annotations an edit never changes.
///
/// `kubectl apply`'s copy of the last applied object, and the revision the Deployment controller
/// numbers the current template with ([`crate::releases::REVISION`]). The controller writes the
/// revision after a template change, so an edit loaded before that write would otherwise set it
/// back.
pub const MANAGED_ANNOTATIONS: [&str; 2] = [LAST_APPLIED, crate::releases::REVISION];

/// Metadata fields an edit never changes besides those a manifest leaves out
/// ([`engawa_graph::manifest::SERVER_METADATA`]): a deletion under way and the owners. The YAML a
/// person edits shows these, so an edit of them is ignored, not refused.
const EDIT_IGNORES: [&str; 3] = [
    "deletionTimestamp",
    "deletionGracePeriodSeconds",
    "ownerReferences",
];

/// `object` without what an edit never changes.
#[must_use]
pub fn editable(object: &Value) -> Value {
    let mut out = object.clone();
    if let Some(map) = out.as_object_mut() {
        map.remove("status");
    }
    if let Some(metadata) = out.get_mut("metadata").and_then(Value::as_object_mut) {
        for field in engawa_graph::manifest::SERVER_METADATA
            .iter()
            .chain(&EDIT_IGNORES)
        {
            metadata.remove(*field);
        }
        if let Some(annotations) = metadata
            .get_mut("annotations")
            .and_then(Value::as_object_mut)
        {
            for annotation in MANAGED_ANNOTATIONS {
                annotations.remove(annotation);
            }
            if annotations.is_empty() {
                metadata.remove("annotations");
            }
        }
    }
    out
}

/// One change of the patch, and the field it touches in `managedFields` terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditOperation {
    /// The `test` and the change.
    pub operations: Vec<Value>,
    /// The field, for ownership.
    pub field: Vec<Step>,
    /// The field, dotted, for a person.
    pub shown: String,
}

/// The operations that turn `current` into `edited`, both [`editable`].
#[must_use]
pub fn edit_operations(current: &Value, edited: &Value) -> Vec<EditOperation> {
    let mut out = Vec::new();
    walk(current, edited, &Location::default(), None, &mut out);
    out
}

#[derive(Debug, Clone, Default)]
struct Location {
    pointer: String,
    steps: Vec<Step>,
    shown: String,
}

impl Location {
    fn field(&self, name: &str) -> Self {
        Self {
            pointer: format!("{}/{}", self.pointer, escape(name)),
            steps: [self.steps.clone(), vec![Step::field(name)]].concat(),
            shown: if self.shown.is_empty() {
                name.to_owned()
            } else {
                format!("{}.{name}", self.shown)
            },
        }
    }

    /// The entry at `index` of this keyed list, identified by its `key` field.
    ///
    /// `managedFields` names an entry by every key field of its list: a container port by its
    /// number and its protocol (`+listMapKey=containerPort`, `+listMapKey=protocol`), and the
    /// protocol defaults to `TCP` [k8s-core-types]. An entry named by the number alone would
    /// match no owner, and a field Helm, Argo CD or Flux owns there would be taken over.
    ///
    /// [k8s-core-types]: https://github.com/kubernetes/api/blob/master/core/v1/types.go
    fn entry(&self, index: usize, key: &str, id: &Value, item: &Value) -> Self {
        let mut map = Map::new();
        map.insert(key.to_owned(), id.clone());
        if key == "containerPort" {
            let protocol = item
                .get("protocol")
                .cloned()
                .unwrap_or_else(|| Value::from("TCP"));
            map.insert("protocol".to_owned(), protocol);
        }
        let shown_id = id.as_str().map_or_else(|| id.to_string(), str::to_owned);
        Self {
            pointer: format!("{}/{index}", self.pointer),
            steps: [self.steps.clone(), vec![Step::Key(Value::Object(map))]].concat(),
            shown: format!("{}[{key}={shown_id}]", self.shown),
        }
    }

    fn change(&self, operations: Vec<Value>) -> EditOperation {
        EditOperation {
            operations,
            field: self.steps.clone(),
            shown: self.shown.clone(),
        }
    }
}

fn walk(
    current: &Value,
    edited: &Value,
    at: &Location,
    field: Option<&str>,
    out: &mut Vec<EditOperation>,
) {
    if current == edited {
        return;
    }
    match (current, edited) {
        (Value::Object(old), Value::Object(new)) => {
            for (name, value) in old {
                let child = at.field(name);
                match new.get(name) {
                    Some(updated) => walk(value, updated, &child, Some(name), out),
                    None => out.push(child.change(vec![
                        test(&child.pointer, value.clone()),
                        remove(&child.pointer),
                    ])),
                }
            }
            for (name, value) in new {
                if !old.contains_key(name) {
                    let child = at.field(name);
                    out.push(child.change(vec![add(&child.pointer, value.clone())]));
                }
            }
        }
        (Value::Array(old), Value::Array(new)) => {
            if !keyed(field, old, new, at, out) {
                out.push(at.change(vec![
                    test(&at.pointer, current.clone()),
                    replace(&at.pointer, edited.clone()),
                ]));
            }
        }
        _ => out.push(at.change(vec![
            test(&at.pointer, current.clone()),
            replace(&at.pointer, edited.clone()),
        ])),
    }
}

/// Compares a keyed list entry by entry; `false` when it cannot (no key, a duplicate or missing
/// key, or a reordering), and the list is then replaced whole.
fn keyed(
    field: Option<&str>,
    old: &[Value],
    new: &[Value],
    at: &Location,
    out: &mut Vec<EditOperation>,
) -> bool {
    let Some(key) = field.and_then(list_key) else {
        return false;
    };
    let ids = |list: &[Value]| -> Option<Vec<Value>> {
        let ids: Vec<Value> = list
            .iter()
            .map(|e| e.get(key).cloned())
            .collect::<Option<_>>()?;
        let mut sorted: Vec<String> = ids.iter().map(Value::to_string).collect();
        sorted.sort();
        sorted.dedup();
        (sorted.len() == ids.len()).then_some(ids)
    };
    let (Some(old_ids), Some(new_ids)) = (ids(old), ids(new)) else {
        return false;
    };
    // The entries both keep must stay in the same order.
    let kept_old: Vec<&Value> = old_ids.iter().filter(|id| new_ids.contains(id)).collect();
    let kept_new: Vec<&Value> = new_ids.iter().filter(|id| old_ids.contains(id)).collect();
    if kept_old != kept_new {
        return false;
    }
    // New entries are appended (`/-`), so they must come after every kept one; an entry
    // inserted before them would land at the end. Order carries meaning in these lists (init
    // containers run in order, a variable may refer only to those before it [k8s-dependent-env]).
    //
    // [k8s-dependent-env]: https://kubernetes.io/docs/tasks/inject-data-application/define-interdependent-environment-variables/
    let first_new = new_ids.iter().position(|id| !old_ids.contains(id));
    let last_kept = new_ids.iter().rposition(|id| old_ids.contains(id));
    if let (Some(first_new), Some(last_kept)) = (first_new, last_kept)
        && first_new < last_kept
    {
        return false;
    }
    let mut changes = Vec::new();
    // Changed entries, at their current positions.
    for (index, (id, entry)) in old_ids.iter().zip(old).enumerate() {
        if let Some(position) = new_ids.iter().position(|n| n == id)
            && let Some(updated) = new.get(position)
        {
            let child = at.entry(index, key, id, entry);
            let mut inner = Vec::new();
            walk(entry, updated, &child, None, &mut inner);
            for mut operation in inner {
                operation
                    .operations
                    .insert(0, test(&format!("{}/{key}", child.pointer), id.clone()));
                changes.push(operation);
            }
        }
    }
    // Removed entries, from the last, so earlier positions stay valid.
    for (index, (id, entry)) in old_ids.iter().zip(old).enumerate().rev() {
        if !new_ids.contains(id) {
            let child = at.entry(index, key, id, entry);
            changes.push(child.change(vec![
                test(&child.pointer, entry.clone()),
                remove(&child.pointer),
            ]));
        }
    }
    // New entries, appended in their order.
    for (id, entry) in new_ids.iter().zip(new) {
        if !old_ids.contains(id) {
            let child = at.entry(0, key, id, entry);
            changes.push(child.change(vec![add(&format!("{}/-", at.pointer), entry.clone())]));
        }
    }
    out.extend(changes);
    true
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use serde_json::json;

    use super::*;
    use crate::json::apply_patch;

    fn patched(current: &Value, edited: &Value) -> Value {
        let operations: Vec<Value> = edit_operations(current, edited)
            .into_iter()
            .flat_map(|o| o.operations)
            .collect();
        apply_patch(current, &operations).unwrap()
    }

    /// A small Deployment-like object: keyed containers with keyed env, and an unkeyed list.
    /// The alphabets are small so a current and an edited object share most entries.
    fn object() -> impl Strategy<Value = Value> {
        let env = prop::collection::vec(
            (prop::sample::select(vec!["A", "B", "C", "D"]), 0..3_u8),
            0..4,
        );
        let container = (
            prop::sample::select(vec!["app", "side", "init"]),
            1..3_u8,
            env,
        );
        (
            1..3_u8,
            prop::collection::vec(container, 0..4),
            prop::collection::vec(prop::sample::select(vec!["x", "y", "z"]), 0..3),
        )
            .prop_map(|(replicas, containers, args)| {
                let mut seen = Vec::new();
                let containers: Vec<Value> = containers
                    .into_iter()
                    .filter(|(name, ..)| {
                        let fresh = !seen.contains(name);
                        seen.push(*name);
                        fresh
                    })
                    .map(|(name, image, env)| {
                        let mut names = Vec::new();
                        let env: Vec<Value> = env
                            .into_iter()
                            .filter(|(n, _)| {
                                let fresh = !names.contains(n);
                                names.push(*n);
                                fresh
                            })
                            .map(|(n, v)| json!({"name": n, "value": v.to_string()}))
                            .collect();
                        json!({"name": name, "image": format!("{name}:{image}"), "env": env})
                    })
                    .collect();
                json!({"spec": {"replicas": replicas, "template": {"spec": {
                    "containers": containers, "args": args}}}})
            })
    }

    proptest! {
        /// The patch reproduces the edit exactly, whatever was added, removed, reordered or
        /// changed, so the plan a person approves writes what they typed.
        #[test]
        fn the_patch_turns_the_object_into_the_edit(current in object(), edited in object()) {
            prop_assert_eq!(patched(&current, &edited), edited);
        }

        /// Editing some containers of a list never touches the others: an untouched one gets
        /// no operation.
        #[test]
        fn an_untouched_container_gets_no_operation(
            current in object(),
            touch in prop::collection::vec(0..3_u8, 4),
        ) {
            let mut edited = current.clone();
            let mut untouched = Vec::new();
            if let Some(containers) = edited.pointer_mut("/spec/template/spec/containers").and_then(Value::as_array_mut) {
                for (container, action) in containers.iter_mut().zip(&touch) {
                    match action {
                        1 => container["image"] = json!("edited:1"),
                        2 => container["env"] = json!([{"name": "E", "value": "edited"}]),
                        _ => untouched.push(container["name"].as_str().unwrap_or_default().to_owned()),
                    }
                }
            }
            let fields: Vec<String> = edit_operations(&current, &edited)
                .into_iter()
                .map(|o| o.shown)
                .collect();
            prop_assert!(!fields.iter().any(|f| f == "spec.template.spec.containers"), "replaced whole: {:?}", fields);
            for name in untouched {
                let shown = format!("spec.template.spec.containers[name={name}]");
                prop_assert!(!fields.iter().any(|f| f.starts_with(&shown)), "{} in {:?}", shown, fields);
            }
        }
    }

    #[test]
    fn a_reordered_keyed_list_is_replaced_whole() {
        let current = json!({"containers": [{"name": "a"}, {"name": "b"}]});
        let edited = json!({"containers": [{"name": "b"}, {"name": "a"}]});
        let operations = edit_operations(&current, &edited);
        assert_eq!(operations.len(), 1);
        assert_eq!(operations[0].shown, "containers");
        assert_eq!(patched(&current, &edited), edited);
    }

    #[test]
    fn a_port_is_named_by_its_number_and_protocol_as_managed_fields_name_it() {
        let current = json!({"ports": [{"containerPort": 80, "name": "web"},
                                       {"containerPort": 53, "protocol": "UDP"}]});
        let edited = json!({"ports": [{"containerPort": 80, "name": "http"},
                                      {"containerPort": 53, "protocol": "UDP", "name": "dns"}]});
        let fields: Vec<Vec<Step>> = edit_operations(&current, &edited)
            .into_iter()
            .map(|o| o.field)
            .collect();
        let port = |key: Value| vec![Step::field("ports"), Step::Key(key), Step::field("name")];
        assert_eq!(
            fields,
            [
                port(json!({"containerPort": 80, "protocol": "TCP"})),
                port(json!({"containerPort": 53, "protocol": "UDP"})),
            ]
        );
    }

    #[test]
    fn server_fields_are_not_editable() {
        let object = json!({"metadata": {"name": "x", "resourceVersion": "4", "uid": "u",
            "annotations": {LAST_APPLIED: "{}", crate::releases::REVISION: "3", "team": "a"}},
            "status": {"ready": true}});
        assert_eq!(
            editable(&object),
            json!({"metadata": {"name": "x", "annotations": {"team": "a"}}})
        );
    }
}
