//! Loads the language-neutral test cases in `fixtures/`.
//!
//! A case is a directory holding `case.yaml`, an optional `input.yaml` with Kubernetes objects
//! and `expected.json`. The format is described in `fixtures/README.md`; this module only reads
//! it. Objects stay [`serde_json::Value`]s here: the crate under test decides how to type them.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

/// The areas under `fixtures/`.
pub const AREAS: [&str; 5] = ["graph", "projection", "mutation", "gaps", "modules"];

/// One test case.
#[derive(Debug, Clone)]
pub struct Fixture {
    /// `<area>/<case>`, for messages.
    pub name: String,
    /// The case's directory.
    pub dir: PathBuf,
    /// What is tested.
    pub case: Case,
    /// The Kubernetes objects of `input.yaml`, in file order (empty without the file).
    pub input: Vec<Value>,
    /// The parsed `expected.json`.
    pub expected: Value,
}

impl Fixture {
    /// The error code the case expects, if it expects a failure.
    #[must_use]
    pub fn expected_error(&self) -> Option<&str> {
        self.expected
            .as_object()
            .filter(|o| o.len() == 1)
            .and_then(|o| o.get("error"))
            .and_then(Value::as_str)
    }

    /// The input objects of one kind.
    pub fn objects<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a Value> + 'a {
        self.input
            .iter()
            .filter(move |o| o.get("kind").and_then(Value::as_str) == Some(kind))
    }
}

/// The contents of `case.yaml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Case {
    /// One sentence on the behaviour under test.
    pub description: String,
    /// The operation, `<area>.<name>`, from the table in `fixtures/README.md`.
    pub operation: String,
    /// Inputs that are not Kubernetes objects.
    #[serde(default)]
    pub params: Value,
}

/// Why a case could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// A file could not be read.
    #[error("{path}: {source}")]
    Read {
        /// The file.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// A file is not valid YAML or JSON of the expected shape.
    #[error("{path}: {message}")]
    Parse {
        /// The file.
        path: PathBuf,
        /// What is wrong.
        message: String,
    },
}

/// The repository's `fixtures/` directory.
#[must_use]
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// Loads every case of `area`, sorted by name.
///
/// # Errors
///
/// When a directory or file cannot be read or parsed.
pub fn load(area: &str) -> Result<Vec<Fixture>, LoadError> {
    load_from(&root(), area)
}

/// [`load`] from another fixtures directory.
///
/// # Errors
///
/// When a directory or file cannot be read or parsed.
pub fn load_from(root: &Path, area: &str) -> Result<Vec<Fixture>, LoadError> {
    let dir = root.join(area);
    let entries = fs::read_dir(&dir).map_err(|source| LoadError::Read {
        path: dir.clone(),
        source,
    })?;
    let mut dirs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| LoadError::Read {
            path: dir.clone(),
            source,
        })?;
        if entry.path().is_dir() {
            dirs.push(entry.path());
        }
    }
    dirs.sort();
    dirs.iter().map(|d| load_case(area, d)).collect()
}

fn load_case(area: &str, dir: &Path) -> Result<Fixture, LoadError> {
    let file = dir.file_name().map(|n| n.to_string_lossy().into_owned());
    let name = format!("{area}/{}", file.unwrap_or_default());
    let case: Case = parse_yaml(&dir.join("case.yaml"))?;
    let input_path = dir.join("input.yaml");
    let input = if input_path.exists() {
        let text = read(&input_path)?;
        serde_saphyr::from_multiple::<Value>(&text)
            .map_err(|e| parse_error(&input_path, &e))?
            .into_iter()
            .filter(|v| !v.is_null())
            .collect()
    } else {
        Vec::new()
    };
    let expected_path = dir.join("expected.json");
    let expected = serde_json::from_str(&read(&expected_path)?)
        .map_err(|e| parse_error(&expected_path, &e))?;
    Ok(Fixture {
        name,
        dir: dir.to_path_buf(),
        case,
        input,
        expected,
    })
}

fn parse_yaml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, LoadError> {
    serde_saphyr::from_str(&read(path)?).map_err(|e| parse_error(path, &e))
}

fn read(path: &Path) -> Result<String, LoadError> {
    fs::read_to_string(path).map_err(|source| LoadError::Read {
        path: path.to_path_buf(),
        source,
    })
}

fn parse_error(path: &Path, error: &dyn std::fmt::Display) -> LoadError {
    LoadError::Parse {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

/// Compares a result with a case's `expected.json` under the rules of `fixtures/README.md`.
///
/// - A field present in `expected` is asserted; a field left out is not.
/// - A field set to `null` asserts that the field is absent from `actual`, `null`, or `false`.
/// - Arrays are compared in order, element by element, and must have the same length.
/// - Objects compare key by key, whatever the order. An empty expected object asserts an empty
///   result: otherwise `{}` (a map keyed by name with no entry) would assert nothing.
///
/// # Errors
///
/// The first difference, with its JSON path (`$` is the root).
pub fn compare(expected: &Value, actual: &Value) -> Result<(), String> {
    compare_at("$", expected, actual)
}

fn compare_at(path: &str, expected: &Value, actual: &Value) -> Result<(), String> {
    match expected {
        Value::Object(fields) => {
            let Some(object) = actual.as_object() else {
                return Err(format!("{path}: expected an object, got {actual}"));
            };
            if fields.is_empty() && !object.is_empty() {
                return Err(format!("{path}: expected an empty object, got {actual}"));
            }
            for (key, want) in fields {
                let at = format!("{path}.{key}");
                match (want, object.get(key)) {
                    (Value::Null, None | Some(Value::Null | Value::Bool(false))) => {}
                    (Value::Null, Some(got)) => {
                        return Err(format!("{at}: expected absent, got {got}"));
                    }
                    (_, None) => return Err(format!("{at}: missing, expected {want}")),
                    (_, Some(got)) => compare_at(&at, want, got)?,
                }
            }
            Ok(())
        }
        Value::Array(items) => {
            let Some(array) = actual.as_array() else {
                return Err(format!("{path}: expected an array, got {actual}"));
            };
            if items.len() != array.len() {
                return Err(format!(
                    "{path}: expected {} elements, got {}: {actual}",
                    items.len(),
                    array.len()
                ));
            }
            items
                .iter()
                .zip(array)
                .enumerate()
                .try_for_each(|(index, (want, got))| {
                    compare_at(&format!("{path}[{index}]"), want, got)
                })
        }
        Value::Null => match actual {
            Value::Null | Value::Bool(false) => Ok(()),
            Value::Bool(true)
            | Value::Number(_)
            | Value::String(_)
            | Value::Array(_)
            | Value::Object(_) => Err(format!("{path}: expected absent, got {actual}")),
        },
        Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            if expected == actual {
                Ok(())
            } else {
                Err(format!("{path}: expected {expected}, got {actual}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::compare;

    #[test]
    fn fields_left_out_are_not_asserted() {
        assert!(compare(&json!({"a": 1}), &json!({"a": 1, "b": 2})).is_ok());
    }

    #[test]
    fn null_asserts_absent_or_false() {
        assert!(compare(&json!({"a": null}), &json!({})).is_ok());
        assert!(compare(&json!({"a": null}), &json!({"a": false})).is_ok());
        assert!(compare(&json!({"a": null}), &json!({"a": "x"})).is_err());
    }

    #[test]
    fn arrays_compare_in_order_and_length() {
        assert!(compare(&json!([1, 2]), &json!([1, 2])).is_ok());
        assert!(compare(&json!([1, 2]), &json!([2, 1])).is_err());
        assert!(compare(&json!([1]), &json!([1, 2])).is_err());
    }

    #[test]
    fn an_empty_object_asserts_an_empty_map() {
        assert!(compare(&json!({"m": {}}), &json!({"m": {}})).is_ok());
        assert!(compare(&json!({"m": {}}), &json!({"m": {"x": 1}})).is_err());
    }

    #[test]
    fn a_missing_field_is_reported_with_its_path() {
        assert_eq!(
            compare(&json!({"a": {"b": 1}}), &json!({"a": {}})),
            Err("$.a.b: missing, expected 1".to_owned())
        );
    }
}
