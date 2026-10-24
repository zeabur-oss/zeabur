//! Helm values: parsing the catalog's YAML, merging layers and setting input paths.
//!
//! The catalog's rule is that of layered Helm values files: maps merge key by key and anything
//! else, lists included, is replaced by the later layer. Values are kept as JSON values; JSON is
//! valid YAML, so a `HelmChart`'s `valuesContent` is written as JSON with sorted keys, which keeps
//! the plan byte-for-byte deterministic.

use serde_json::{Map, Value};

use crate::PlanError;

/// Parses one YAML values document. An empty document is an empty map.
///
/// # Errors
///
/// When the YAML is invalid or its top level is not a map.
pub fn parse(yaml: &str, what: &str) -> Result<Value, PlanError> {
    if yaml.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    let value: Value = serde_saphyr::from_str(yaml).map_err(|e| PlanError::Values {
        what: what.to_owned(),
        reason: e.to_string(),
    })?;
    match value {
        Value::Object(_) => Ok(value),
        Value::Null => Ok(Value::Object(Map::new())),
        Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Array(_) => {
            Err(PlanError::Values {
                what: what.to_owned(),
                reason: "the top level is not a map".to_owned(),
            })
        }
    }
}

/// Merges `over` into `base`: maps merge, everything else replaces.
pub fn merge(base: &mut Value, over: Value) {
    match (base, over) {
        (Value::Object(base), Value::Object(over)) => {
            for (key, value) in over {
                match base.get_mut(&key) {
                    Some(existing) => merge(existing, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, over) => *base = over,
    }
}

/// Sets a dotted path; numeric segments index lists, which grow with nulls when short.
///
/// # Errors
///
/// When a segment crosses a value that is neither a map nor a list.
pub fn set_path(values: &mut Value, path: &str, value: Value) -> Result<(), PlanError> {
    let mut node = values;
    let segments: Vec<&str> = path.split('.').collect();
    let Some((last, parents)) = segments.split_last() else {
        return Err(bad_path(path));
    };
    for segment in parents {
        node = child(node, segment, path)?;
    }
    *child(node, last, path)? = value;
    Ok(())
}

fn child<'v>(node: &'v mut Value, segment: &str, path: &str) -> Result<&'v mut Value, PlanError> {
    if node.is_null() {
        *node = if segment.parse::<usize>().is_ok() {
            Value::Array(Vec::new())
        } else {
            Value::Object(Map::new())
        };
    }
    match node {
        Value::Object(map) => Ok(map.entry(segment.to_owned()).or_insert(Value::Null)),
        Value::Array(list) => {
            let index: usize =
                segment
                    .parse()
                    .map_err(|e: std::num::ParseIntError| PlanError::Values {
                        what: path.to_owned(),
                        reason: e.to_string(),
                    })?;
            if list.len() <= index {
                list.resize(index + 1, Value::Null);
            }
            list.get_mut(index).ok_or_else(|| bad_path(path))
        }
        Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Null => Err(bad_path(path)),
    }
}

fn bad_path(path: &str) -> PlanError {
    PlanError::Values {
        what: path.to_owned(),
        reason: "the path crosses a value that is not a map or list".to_owned(),
    }
}

/// The canonical text of a values document: JSON with sorted keys.
#[must_use]
pub fn canonical(values: &Value) -> String {
    // serde_json's map is ordered by key without the `preserve_order` feature.
    values.to_string()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn maps_merge_and_lists_replace() {
        let mut base = parse("a: {b: 1, c: [1, 2]}\nd: p", "base").unwrap();
        merge(&mut base, parse("a: {c: [3]}\nd: q", "over").unwrap());
        assert_eq!(base, json!({"a": {"b": 1, "c": [3]}, "d": "q"}));
    }

    #[test]
    fn comments_and_empty_documents() {
        assert_eq!(parse("# nothing\n", "x").unwrap(), json!({}));
        assert_eq!(parse("a: 1 # why\n", "x").unwrap(), json!({"a": 1}));
        assert!(parse("- 1", "x").is_err());
    }

    #[test]
    fn paths_create_maps_and_lists() {
        let mut v = json!({});
        set_path(
            &mut v,
            "configuration.backupStorageLocation.0.bucket",
            json!("b"),
        )
        .unwrap();
        set_path(&mut v, "provider.name", json!("cloudflare")).unwrap();
        assert_eq!(
            v,
            json!({"configuration": {"backupStorageLocation": [{"bucket": "b"}]}, "provider": {"name": "cloudflare"}})
        );
        assert!(set_path(&mut v, "provider.name.x", json!(1)).is_err());
    }

    #[test]
    fn canonical_sorts_keys() {
        assert_eq!(
            canonical(&json!({"b": 1, "a": {"d": 2, "c": 3}})),
            r#"{"a":{"c":3,"d":2},"b":1}"#
        );
    }
}
