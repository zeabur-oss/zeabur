//! Field ownership from `managedFields` [k8s-ssa-fields].
//!
//! Each entry names a manager and, in `fieldsV1`, a trie of the fields it owns: `f:<name>` is a
//! field, `k:<json>` the entry of a keyed list whose key fields equal that JSON object,
//! `v:<json>` the entry of a set equal to that value, `i:<n>` an entry by position, and `.` the
//! node itself.
//!
//! [k8s-ssa-fields]: https://kubernetes.io/docs/reference/using-api/server-side-apply/#field-management

use serde_json::Value;

/// One step of a field path, in `managedFields` terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// A field (`f:<name>`).
    Field(String),
    /// The entry of a keyed list whose fields equal these (`k:{…}`).
    Key(Value),
}

impl Step {
    /// `f:<name>`.
    #[must_use]
    pub fn field(name: &str) -> Self {
        Self::Field(name.to_owned())
    }

    /// `k:{"name": <name>}`.
    #[must_use]
    pub fn named(name: &str) -> Self {
        Self::Key(serde_json::json!({ "name": name }))
    }

    fn matches(&self, token: &str) -> bool {
        match self {
            Self::Field(name) => token.strip_prefix("f:") == Some(name.as_str()),
            Self::Key(key) => token
                .strip_prefix("k:")
                .and_then(|json| serde_json::from_str::<Value>(json).ok())
                .is_some_and(|parsed| &parsed == key),
        }
    }
}

/// The managers that own `path` or anything under it, sorted and without duplicates.
#[must_use]
pub fn owners(object: &Value, path: &[Step]) -> Vec<String> {
    let mut managers: Vec<String> = object
        .pointer("/metadata/managedFields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry
                .get("fieldsV1")
                .is_some_and(|fields| reaches(fields, path))
        })
        .filter_map(|entry| entry.get("manager").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    managers.sort();
    managers.dedup();
    managers
}

/// Whether the trie `fields` contains `path`.
#[must_use]
pub fn reaches(fields: &Value, path: &[Step]) -> bool {
    let Some((step, rest)) = path.split_first() else {
        return true;
    };
    fields.as_object().is_some_and(|map| {
        map.iter()
            .any(|(token, child)| step.matches(token) && reaches(child, rest))
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn owners_are_found_by_field_and_key() {
        let object = json!({"metadata": {"managedFields": [
            {"manager": "helm", "fieldsV1": {"f:spec": {"f:template": {"f:spec": {"f:containers": {
                "k:{\"name\":\"app\"}": {".": {}, "f:image": {}, "f:env": {"k:{\"name\":\"A\"}": {".": {}}}}}}}}}},
            {"manager": "kubectl", "fieldsV1": {"f:spec": {"f:replicas": {}}}}
        ]}});
        let env_a = [
            Step::field("spec"),
            Step::field("template"),
            Step::field("spec"),
            Step::field("containers"),
            Step::named("app"),
            Step::field("env"),
            Step::named("A"),
        ];
        assert_eq!(owners(&object, &env_a), ["helm"]);
        let mut env_b = env_a.to_vec();
        env_b.pop();
        env_b.push(Step::named("B"));
        assert_eq!(owners(&object, &env_b), Vec::<String>::new());
        assert_eq!(
            owners(&object, &[Step::field("spec"), Step::field("replicas")]),
            ["kubectl"]
        );
    }

    #[test]
    fn a_key_of_several_fields_matches_whatever_their_order() {
        let fields = json!({"f:ports": {"k:{\"protocol\":\"TCP\",\"containerPort\":80}": {}}});
        let port = Step::Key(json!({"containerPort": 80, "protocol": "TCP"}));
        assert!(reaches(&fields, &[Step::field("ports"), port]));
    }
}
