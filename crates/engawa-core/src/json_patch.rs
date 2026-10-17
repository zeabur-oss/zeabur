//! The subset of RFC 6902 JSON Patch Engawa sends (`test`, `add`, `replace`, `remove`), applied
//! locally [rfc6902].
//!
//! A plan applies it to show the object as it will be after the write; the in-memory cluster
//! applies it to store that object. kube-apiserver applies the same operations to the stored
//! object, and the cluster tests on k3s check that both give the same result.
//!
//! [rfc6902]: https://www.rfc-editor.org/rfc/rfc6902

use serde_json::Value;

/// Why a patch did not apply.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("JSON Patch operation {index} ({op} {path}) does not apply")]
pub struct PatchError {
    /// The operation's position.
    pub index: usize,
    /// Its `op`.
    pub op: String,
    /// Its `path`.
    pub path: String,
}

/// Applies the `test`, `add`, `replace` and `remove` operations of `operations` to `document`, all
/// or nothing [rfc6902].
///
/// [rfc6902]: https://www.rfc-editor.org/rfc/rfc6902
///
/// # Errors
///
/// When an operation's target is missing, a `test` fails, or the operation is not one of the
/// four.
pub fn apply_patch(document: &Value, operations: &[Value]) -> Result<Value, PatchError> {
    let mut out = document.clone();
    for (index, operation) in operations.iter().enumerate() {
        let kind = operation.get("op").and_then(Value::as_str).unwrap_or("");
        let path = operation.get("path").and_then(Value::as_str).unwrap_or("");
        let failure = || PatchError {
            index,
            op: kind.to_owned(),
            path: path.to_owned(),
        };
        let value = operation.get("value").cloned();
        let applied = match (kind, value) {
            ("test", Some(value)) => out.pointer(path) == Some(&value),
            ("add", Some(value)) => put(&mut out, path, value, true),
            ("replace", Some(value)) => {
                out.pointer(path).is_some() && put(&mut out, path, value, false)
            }
            ("remove", None) => take(&mut out, path),
            _ => false,
        };
        if !applied {
            return Err(failure());
        }
    }
    Ok(out)
}

/// One pointer token, unescaped (`~1` → `/`, `~0` → `~`) [rfc6901].
///
/// [rfc6901]: https://www.rfc-editor.org/rfc/rfc6901
fn unescape(token: &str) -> String {
    token.replace("~1", "/").replace("~0", "~")
}

/// Splits a pointer into its parent pointer and last token.
fn split(path: &str) -> Option<(&str, String)> {
    let at = path.rfind('/')?;
    let parent = path.get(..at)?;
    let last = path.get(at + 1..)?;
    Some((parent, unescape(last)))
}

/// Sets the value at `path`; an array index or `-` inserts when `insert` (RFC 6902 `add`).
fn put(document: &mut Value, path: &str, value: Value, insert: bool) -> bool {
    if path.is_empty() {
        *document = value;
        return true;
    }
    let Some((parent, last)) = split(path) else {
        return false;
    };
    match document.pointer_mut(parent) {
        Some(Value::Object(map)) => {
            map.insert(last, value);
            true
        }
        Some(Value::Array(items)) => {
            if last == "-" && insert {
                items.push(value);
                return true;
            }
            let Ok(index) = last.parse::<usize>() else {
                return false;
            };
            if insert && index <= items.len() {
                items.insert(index, value);
                true
            } else if let Some(slot) = items.get_mut(index) {
                *slot = value;
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Removes the value at `path`.
fn take(document: &mut Value, path: &str) -> bool {
    let Some((parent, last)) = split(path) else {
        return false;
    };
    match document.pointer_mut(parent) {
        Some(Value::Object(map)) => map.remove(&last).is_some(),
        Some(Value::Array(items)) => match last.parse::<usize>() {
            Ok(index) if index < items.len() => {
                items.remove(index);
                true
            }
            _ => false,
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_patch_applies_all_or_nothing() {
        let document = json!({"metadata": {"resourceVersion": "7"}, "data": {"a": "1"}});
        let patch = vec![
            json!({"op": "test", "path": "/metadata/resourceVersion", "value": "7"}),
            json!({"op": "add", "path": "/data/b", "value": "2"}),
            json!({"op": "replace", "path": "/data/a", "value": "one"}),
        ];
        assert_eq!(
            apply_patch(&document, &patch).unwrap(),
            json!({"metadata": {"resourceVersion": "7"}, "data": {"a": "one", "b": "2"}})
        );
        let stale = vec![
            json!({"op": "test", "path": "/metadata/resourceVersion", "value": "6"}),
            json!({"op": "remove", "path": "/data/a"}),
        ];
        assert_eq!(apply_patch(&document, &stale).unwrap_err().index, 0);
        let missing = json!({"op": "replace", "path": "/data/missing", "value": 1});
        assert!(apply_patch(&document, &[missing]).is_err());
    }

    #[test]
    fn arrays_take_inserts_appends_and_removals() {
        let document = json!({"list": [1, 2]});
        let patch = vec![
            json!({"op": "add", "path": "/list/-", "value": 3}),
            json!({"op": "add", "path": "/list/0", "value": 0}),
            json!({"op": "remove", "path": "/list/1"}),
        ];
        assert_eq!(
            apply_patch(&document, &patch).unwrap(),
            json!({"list": [0, 2, 3]})
        );
    }
}
