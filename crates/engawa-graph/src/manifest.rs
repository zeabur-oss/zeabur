//! An object as a person reads and edits it: YAML without what kube-apiserver keeps for itself.
//!
//! # What is left out
//!
//! - `status`: the controllers' report, not something a person writes.
//! - `metadata.managedFields`, `resourceVersion`, `uid`, `generation`, `creationTimestamp` and
//!   `selfLink`: bookkeeping the server sets on every write. The page shows the age from the
//!   object's own listing, so the timestamp is not lost.
//! - The `kubectl.kubernetes.io/last-applied-configuration` annotation: a copy of the object
//!   `kubectl apply` keeps for its three-way merge [kubectl-apply], which doubles the text and, on
//!   a Secret, repeats its values. On a Secret it stays, with its value hidden, so the person can
//!   see that `kubectl apply` manages the object.
//!
//! The same trimming is what `kubectl-neat` does for `kubectl get -o yaml` [kubectl-neat]; edits
//! are written as JSON Patch against the object as stored, which needs none of the removed
//! fields.
//!
//! # What is hidden
//!
//! A Secret's `data` and `stringData` keep their keys and show [`HIDDEN`] for every value.
//! Values are never part of the text, whatever reached this function.
//!
//! # Quoting
//!
//! Strings are quoted as `kubectl get -o yaml` quotes them, so that `1:20`, `2024-01-01` or `yes`
//! stay strings for a YAML 1.1 reader too (go-yaml's rules, in `manifest/scalar.rs`). Other values
//! print as serde-saphyr writes them.
//!
//! # Order
//!
//! `apiVersion`, `kind`, `metadata` (name, namespace, labels, annotations, then the rest), then
//! the other fields in alphabetical order; maps inside are alphabetical. The same object always
//! gives the same text, so two renderings can be compared line by line.
//!
//! [kubectl-apply]: https://kubernetes.io/docs/tasks/manage-kubernetes-objects/declarative-config/#how-apply-calculates-differences-and-merges-changes
//! [kubectl-neat]: https://github.com/itaysk/kubectl-neat

use serde::ser::{Serialize, SerializeMap, Serializer};
use serde_json::{Map, Value};

use self::scalar::Scalar;

mod scalar;

/// The annotation `kubectl apply` writes the last applied object into.
pub const LAST_APPLIED: &str = "kubectl.kubernetes.io/last-applied-configuration";

/// What a hidden value shows.
pub const HIDDEN: &str = "(hidden)";

/// Metadata the server keeps for itself, which a manifest leaves out.
pub const SERVER_METADATA: [&str; 6] = [
    "managedFields",
    "resourceVersion",
    "uid",
    "generation",
    "creationTimestamp",
    "selfLink",
];

/// Metadata fields first, in this order.
const METADATA_ORDER: [&str; 4] = ["name", "namespace", "labels", "annotations"];

/// Top-level fields first, in this order.
const TOP_ORDER: [&str; 3] = ["apiVersion", "kind", "metadata"];

fn is_secret(object: &Value) -> bool {
    object.get("kind").and_then(Value::as_str) == Some("Secret")
        && object.get("apiVersion").and_then(Value::as_str) == Some("v1")
}

/// The object without server bookkeeping and with a Secret's values hidden (see the module
/// documentation).
#[must_use]
pub fn clean(object: &Value) -> Value {
    let mut out = object.clone();
    let secret = is_secret(object);
    let Some(top) = out.as_object_mut() else {
        return out;
    };
    top.remove("status");
    if let Some(metadata) = top.get_mut("metadata").and_then(Value::as_object_mut) {
        for field in SERVER_METADATA {
            metadata.remove(field);
        }
        if let Some(annotations) = metadata
            .get_mut("annotations")
            .and_then(Value::as_object_mut)
        {
            if secret {
                if let Some(value) = annotations.get_mut(LAST_APPLIED) {
                    *value = Value::String(HIDDEN.to_owned());
                }
            } else {
                annotations.remove(LAST_APPLIED);
            }
        }
        if metadata
            .get("annotations")
            .and_then(Value::as_object)
            .is_some_and(Map::is_empty)
        {
            metadata.remove("annotations");
        }
    }
    if secret {
        for field in ["data", "stringData"] {
            if let Some(values) = top.get_mut(field).and_then(Value::as_object_mut) {
                for value in values.values_mut() {
                    *value = Value::String(HIDDEN.to_owned());
                }
            }
        }
    }
    out
}

/// A value whose strings are quoted as kubectl quotes them.
struct Node<'a>(&'a Value);

impl Serialize for Node<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Value::String(text) => Scalar(text).serialize(serializer),
            Value::Array(items) => serializer.collect_seq(items.iter().map(Node)),
            Value::Object(map) => {
                serializer.collect_map(map.iter().map(|(key, value)| (Scalar(key), Node(value))))
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => self.0.serialize(serializer),
        }
    }
}

/// A map whose first keys come in a given order, the rest alphabetically.
struct Ordered<'a> {
    map: &'a Map<String, Value>,
    first: &'a [&'a str],
    metadata: bool,
}

impl Serialize for Ordered<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut out = serializer.serialize_map(Some(self.map.len()))?;
        let rest = self
            .map
            .iter()
            .filter(|(key, _)| !self.first.contains(&key.as_str()));
        let first = self
            .first
            .iter()
            .filter_map(|key| self.map.get_key_value(*key));
        for (key, value) in first.chain(rest) {
            match value {
                Value::Object(inner) if self.metadata && key == "metadata" => {
                    out.serialize_entry(
                        &Scalar(key),
                        &Ordered {
                            map: inner,
                            first: &METADATA_ORDER,
                            metadata: false,
                        },
                    )?;
                }
                Value::Null
                | Value::Bool(_)
                | Value::Number(_)
                | Value::String(_)
                | Value::Array(_)
                | Value::Object(_) => out.serialize_entry(&Scalar(key), &Node(value))?,
            }
        }
        out.end()
    }
}

/// The object as YAML, cleaned and in a fixed order (see the module documentation).
///
/// # Errors
///
/// When the YAML writer fails, which a JSON value should never make it do.
pub fn to_yaml(object: &Value) -> Result<String, String> {
    let cleaned = clean(object);
    match &cleaned {
        Value::Object(map) => serde_saphyr::to_string(&Ordered {
            map,
            first: &TOP_ORDER,
            metadata: true,
        }),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Array(_) => {
            serde_saphyr::to_string(&Node(&cleaned))
        }
    }
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn deployment() -> Value {
        json!({
            "status": {"replicas": 2},
            "spec": {"replicas": 2, "template": {"spec": {"containers": [{"name": "web", "image": "nginx:1.29"}]}}},
            "metadata": {
                "uid": "1234", "resourceVersion": "99", "generation": 3,
                "creationTimestamp": "2026-10-15T00:00:00Z",
                "managedFields": [{"manager": "kubectl"}],
                "annotations": {LAST_APPLIED: "{\"spec\":{}}"},
                "labels": {"app": "web"},
                "namespace": "shop", "name": "web"
            },
            "kind": "Deployment", "apiVersion": "apps/v1"
        })
    }

    #[test]
    fn server_bookkeeping_is_left_out_and_the_order_is_fixed() {
        let yaml = to_yaml(&deployment()).unwrap();
        for gone in [
            "status",
            "uid",
            "resourceVersion",
            "generation",
            "creationTimestamp",
            "managedFields",
            LAST_APPLIED,
            "annotations",
        ] {
            assert!(!yaml.contains(gone), "{gone} in\n{yaml}");
        }
        let at = |text: &str| yaml.find(text).unwrap();
        assert!(at("apiVersion") < at("kind") && at("kind") < at("metadata"));
        assert!(at("metadata") < at("name: web") && at("name: web") < at("namespace: shop"));
        assert!(at("namespace: shop") < at("labels") && at("labels") < at("spec"));
        assert_eq!(to_yaml(&deployment()).unwrap(), yaml);
    }

    #[test]
    fn a_secret_never_shows_a_value() {
        let secret = json!({
            "apiVersion": "v1", "kind": "Secret",
            "metadata": {"name": "db", "namespace": "shop", "annotations": {
                LAST_APPLIED: "{\"data\":{\"password\":\"aHVudGVyMg==\"}}"
            }},
            "type": "Opaque",
            "data": {"password": "aHVudGVyMg==", "user": "YWRtaW4="},
            "stringData": {"token": "s3cr3t"}
        });
        let yaml = to_yaml(&secret).unwrap();
        for value in ["aHVudGVyMg==", "YWRtaW4=", "s3cr3t", "hunter2"] {
            assert!(!yaml.contains(value), "{value} in\n{yaml}");
        }
        assert!(yaml.contains("password: (hidden)"), "{yaml}");
        assert!(yaml.contains("token: (hidden)"), "{yaml}");
        assert!(yaml.contains(LAST_APPLIED), "{yaml}");
    }

    #[test]
    fn a_config_map_keeps_its_values() {
        let config = json!({
            "apiVersion": "v1", "kind": "ConfigMap",
            "metadata": {"name": "app"}, "data": {"mode": "fast"}
        });
        assert!(to_yaml(&config).unwrap().contains("mode: fast"));
    }

    #[test]
    fn strings_are_quoted_as_kubectl_quotes_them_everywhere_in_the_object() {
        // People copy this YAML into tools on go-yaml, a YAML 1.1 reader, which takes `yes`,
        // `1:20` or `2024-01-01` for a boolean, a number or a timestamp
        // (https://yaml.org/type/bool.html, float.html, timestamp.html). The full table is in
        // `scalar`.
        let object = json!({
            "apiVersion": "v1", "kind": "ConfigMap",
            "metadata": {"name": "app", "labels": {"on": "yes"}},
            "data": {"duration": "1:20", "since": "2024-01-01", "mode": "0755", "who": "@here"},
            "list": ["no", "*", "nginx:1.27", 3, true, null]
        });
        let yaml = to_yaml(&object).unwrap();
        assert_eq!(
            yaml,
            "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: app\n  labels:\n    \"on\": \"yes\"\n\
             data:\n  duration: \"1:20\"\n  mode: \"0755\"\n  since: \"2024-01-01\"\n  who: '@here'\n\
             list:\n- \"no\"\n- '*'\n- nginx:1.27\n- 3\n- true\n- null\n"
        );
        let back: Value = serde_saphyr::from_str(&yaml).unwrap();
        assert_eq!(back, object);
    }
}
