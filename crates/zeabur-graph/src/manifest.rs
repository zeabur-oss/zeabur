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
//! [`clean`] also hides a Secret's values: its `data` and `stringData` keep their keys and show
//! [`HIDDEN`] for every value. [`to_yaml`] writes what it is given, values included, so a caller
//! that copies a Secret into a plan, a log or an error passes it through [`clean`] first.
//!
//! # Quoting
//!
//! serde-saphyr quotes every string that a YAML 1.1 reader would take for another type, as
//! `kubectl get -o yaml` does: booleans and nulls such as `yes`, numbers such as `0755` or `1:20`,
//! and timestamps such as `2024-01-01` ([`serde_saphyr::SerializerOptions`], `yaml_12` off). So
//! the text stays the same strings for go-yaml and other YAML 1.1 readers. The quote style can
//! differ from kubectl's.
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
    let mut out = trim(object);
    if is_secret(object)
        && let Some(top) = out.as_object_mut()
    {
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

/// The object without server bookkeeping; a Secret keeps its values (see the module
/// documentation).
#[must_use]
pub fn trim(object: &Value) -> Value {
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
    out
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
                        key,
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
                | Value::Object(_) => out.serialize_entry(key, value)?,
            }
        }
        out.end()
    }
}

/// The object as YAML, [`trim`]med and in a fixed order (see the module documentation).
///
/// # Errors
///
/// When the YAML writer fails, which a JSON value should never make it do.
pub fn to_yaml(object: &Value) -> Result<String, String> {
    let cleaned = trim(object);
    match &cleaned {
        Value::Object(map) => serde_saphyr::to_string(&Ordered {
            map,
            first: &TOP_ORDER,
            metadata: true,
        }),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Array(_) => {
            serde_saphyr::to_string(&cleaned)
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
    fn a_cleaned_secret_never_shows_a_value() {
        let secret = json!({
            "apiVersion": "v1", "kind": "Secret",
            "metadata": {"name": "db", "namespace": "shop", "annotations": {
                LAST_APPLIED: "{\"data\":{\"password\":\"aHVudGVyMg==\"}}"
            }},
            "type": "Opaque",
            "data": {"password": "aHVudGVyMg==", "user": "YWRtaW4="},
            "stringData": {"token": "s3cr3t"}
        });
        let yaml = to_yaml(&clean(&secret)).unwrap();
        for value in ["aHVudGVyMg==", "YWRtaW4=", "s3cr3t", "hunter2"] {
            assert!(!yaml.contains(value), "{value} in\n{yaml}");
        }
        assert!(yaml.contains("password: (hidden)"), "{yaml}");
        assert!(yaml.contains("token: (hidden)"), "{yaml}");
        assert!(yaml.contains(LAST_APPLIED), "{yaml}");

        let raw = to_yaml(&secret).unwrap();
        assert!(raw.contains("password: aHVudGVyMg=="), "{raw}");
        assert!(
            raw.contains(&format!("{LAST_APPLIED}: (hidden)")),
            "the last-applied copy stays hidden: {raw}"
        );
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
    fn strings_kubectl_quotes_are_quoted_everywhere_in_the_object() {
        // People copy this YAML into tools on go-yaml, a YAML 1.1 reader, which takes `yes`,
        // `1:20` or `2024-01-01` for a boolean, a number or a timestamp
        // (https://yaml.org/type/bool.html, float.html, timestamp.html). The full table is in
        // `strings_kubectl_quotes_are_quoted`.
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
             data:\n  duration: \"1:20\"\n  mode: \"0755\"\n  since: \"2024-01-01\"\n  who: \"@here\"\n\
             list:\n- \"no\"\n- \"*\"\n- nginx:1.27\n- 3\n- true\n- null\n"
        );
        let back: Value = serde_saphyr::from_str(&yaml).unwrap();
        assert_eq!(back, object);
    }

    /// Each value as `kubectl create configmap x --from-literal=k=<value> --dry-run=client -o
    /// yaml` writes it (kubectl v1.37.1). Where kubectl quotes, the value is quoted here too,
    /// maybe in the other style.
    #[test]
    fn strings_kubectl_quotes_are_quoted() {
        #[rustfmt::skip]
        let table: &[(&str, &str)] = &[
            // YAML 1.1 booleans, in the spellings go-yaml reads; others stay bare.
            ("true", r#""true""#), ("True", r#""True""#), ("FALSE", r#""FALSE""#),
            ("yes", r#""yes""#), ("Yes", r#""Yes""#), ("NO", r#""NO""#),
            ("y", r#""y""#), ("N", r#""N""#), ("on", r#""on""#),
            ("OFF", r#""OFF""#),
            // Nulls.
            ("", r#""""#), ("~", r#""~""#), ("null", r#""null""#), ("NULL", r#""NULL""#),
            // Sexagesimal numbers.
            ("1:20", r#""1:20""#), ("190:20:30", r#""190:20:30""#),
            ("190:20:30.15", r#""190:20:30.15""#), ("-1:20", r#""-1:20""#),
            ("1_0:20", r#""1_0:20""#), ("1:60", "1:60"), ("1:2_0", "1:2_0"),
            // Timestamps.
            ("2024-01-01", r#""2024-01-01""#), ("2024-1-1", r#""2024-1-1""#),
            ("2001-12-14t21:59:43.10-05:00", r#""2001-12-14t21:59:43.10-05:00""#),
            ("2001-12-14T21:59:43.10Z", r#""2001-12-14T21:59:43.10Z""#),
            ("2001-12-14 21:59:43.10", r#""2001-12-14 21:59:43.10""#),
            ("2024-01-01  10:00:00", r#""2024-01-01  10:00:00""#),
            ("2024-01-01 10:00:00,5", r#""2024-01-01 10:00:00,5""#),
            ("2024-01-01T10:00:00+24:00", r#""2024-01-01T10:00:00+24:00""#),
            ("2024-01-01T10:00:00+05:60", r#""2024-01-01T10:00:00+05:60""#),
            ("2024-02-29", r#""2024-02-29""#), ("2000-02-29", r#""2000-02-29""#),
            ("2024-01-01T10:00:00+0530", "2024-01-01T10:00:00+0530"),
            ("2024-01-01 10:00", "2024-01-01 10:00"),
            ("2024-001-01", "2024-001-01"), ("12024-01-01", "12024-01-01"),
            // Integers in YAML 1.1 and Go spellings.
            ("0x1F", r#""0x1F""#), ("+0x1F", r#""+0x1F""#), ("0X1f", r#""0X1f""#),
            ("0o17", r#""0o17""#), ("0O17", r#""0O17""#), ("017", r#""017""#),
            ("08", r#""08""#), ("0b1010", r#""0b1010""#), ("-0b11", r#""-0b11""#),
            ("1_000", r#""1_000""#), ("1__0", r#""1__0""#), ("1_", r#""1_""#),
            ("0x_1F", r#""0x_1F""#), ("-1", r#""-1""#), ("+1", r#""+1""#),
            ("0xFFFFFFFFFFFFFFFF", r#""0xFFFFFFFFFFFFFFFF""#),
            ("-0x8000000000000000", r#""-0x8000000000000000""#),
            ("99999999999999999999", r#""99999999999999999999""#),
            ("0x", "0x"), ("0b2", "0b2"), ("0x1p3", "0x1p3"),
            // Floats.
            (".inf", r#"".inf""#), ("-.Inf", r#""-.Inf""#), (".NaN", r#"".NaN""#),
            ("1e3", r#""1e3""#), ("1E3", r#""1E3""#), ("1.", r#""1.""#), (".5", r#"".5""#),
            ("+.5", r#""+.5""#), ("1.e5", r#""1.e5""#), ("6.8523015e+5", r#""6.8523015e+5""#),
            (".5_0", r#"".5_0""#), ("1_0.5", r#""1_0.5""#), ("1e-400", r#""1e-400""#),
            // Strings that would break the syntax bare.
            ("@here", "'@here'"), ("%x", "'%x'"), ("-", "'-'"), ("- item", "'- item'"),
            ("? x", "'? x'"), ("a: b", "'a: b'"), ("ends:", "'ends:'"),
            (" leading", "' leading'"), ("trailing ", "'trailing '"),
            ("#comment", "'#comment'"), ("a #b", "'a #b'"), ("*", "'*'"),
            ("&anchor", "'&anchor'"), ("!tag", "'!tag'"), ("|x", "'|x'"), (">x", "'>x'"),
            ("'q", "'''q'"), ("\"q", "'\"q'"), ("`q", "'`q'"), (",x", "',x'"),
            ("[x", "'[x'"), ("{x", "'{x'"), ("--- x", "'--- x'"), ("...", "'...'"),
            ("a\tb", r#""a\tb""#),
            // Ordinary strings stay bare.
            ("-x", "-x"), ("?x", "?x"), ("nginx:1.27", "nginx:1.27"),
            ("/data", "/data"), ("100m", "100m"), ("v1.2.3", "v1.2.3"),
            ("https://example.com/a", "https://example.com/a"),
        ];
        for (value, kubectl) in table {
            let yaml = to_yaml(&json!({ "k": value })).unwrap();
            let written = yaml.strip_prefix("k: ").unwrap_or_default();
            if kubectl.starts_with(['"', '\'']) {
                assert!(written.starts_with(['"', '\'']), "{value:?} => {yaml:?}");
            }
            let back: Value = serde_saphyr::from_str(&yaml).unwrap();
            assert_eq!(back["k"], *value, "{value:?} => {yaml:?}");
        }
    }

    /// Whatever the quoting, the text reads back as the same string.
    #[test]
    fn what_is_written_reads_back_the_same() {
        for value in [
            "yes",
            "no",
            "on",
            "off",
            "y",
            "n",
            "true",
            "null",
            "~",
            "",
            "0123",
            "0",
            "00",
            "1e3",
            "1E3",
            "0x1F",
            "1_000",
            ".5",
            "-1",
            "+1",
            "1.0",
            "1.2.3",
            "1:20",
            ":",
            "a:b",
            "a: b",
            "ends:",
            ":x",
            "#",
            "a#b",
            "a #b",
            "#a",
            "-",
            "- a",
            "?",
            "? a",
            "&a",
            "*a",
            "!a",
            "%a",
            "@a",
            "'a'",
            "\"a\"",
            "a'b",
            "a\"b",
            " a",
            "a ",
            "a\tb",
            "a\nb",
            "a\n",
            "\n",
            "[a]",
            "{a}",
            "a,b",
            "|",
            ">",
            "<<",
            "=",
            "inf",
            ".inf",
            "nan",
            ".nan",
            "2024-01-01",
            "2024-01-01T10:00:00Z",
            "12:30",
            "v1.27",
            "1.27",
            "nginx:1.27",
            "https://example.com/a#b",
            "false ",
            "Yes",
            "NO",
            "Null",
            "NULL",
            "é",
            "日本語",
        ] {
            let yaml = to_yaml(&json!({ "k": value })).unwrap();
            let back: Value = serde_saphyr::from_str(&yaml).unwrap();
            assert_eq!(back["k"], value, "{value:?} => {yaml:?}");
        }
    }
}
