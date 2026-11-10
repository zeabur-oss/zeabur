use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Loads every case of an area from the calling crate's own `fixtures/` directory, sorted by
/// name: `load!("graph")` reads `<crate>/fixtures/graph`. A [`Result`] of [`Vec<Fixture>`] or
/// [`LoadError`].
#[macro_export]
macro_rules! load {
    ($area:expr) => {
        $crate::fixtures::load_from(
            &::std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures"),
            $area,
        )
    };
}

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

/// Loads every case of `area` under the fixtures directory `root`, sorted by name.
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

/// Loads the case whose `case.yaml` is `file`, in the directory `<area>/<case>`.
///
/// # Errors
///
/// When a file of the case cannot be read or parsed.
pub fn load_case_file(file: &Path) -> Result<Fixture, LoadError> {
    let dir = file.parent().unwrap_or_else(|| Path::new(""));
    let area = dir.parent().and_then(Path::file_name);
    load_case(
        &area.map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        dir,
    )
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

fn parse_yaml<T: DeserializeOwned>(path: &Path) -> Result<T, LoadError> {
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

/// One operation of a fixture area, with the type of its parameters.
///
/// A case runs through [`run_op`], which reads `params` as [`FixtureOp::Params`]; a misspelt,
/// extra or wrongly typed parameter fails when the case loads.
pub trait FixtureOp {
    /// The operation name in `case.yaml`, `<area>.<name>`.
    const NAME: &'static str;
    /// The parameters. Derive `Deserialize` with `deny_unknown_fields`.
    type Params: DeserializeOwned;
    /// Runs the operation on the case's input objects.
    ///
    /// # Errors
    ///
    /// When the input objects do not fit the operation.
    fn run(input: &[Value], params: Self::Params) -> Result<Value, String>;
}

/// Runs a case through its operation.
///
/// # Errors
///
/// When the case names another operation or its `params` do not fit [`FixtureOp::Params`].
pub fn run_op<Op: FixtureOp>(fixture: &Fixture) -> Result<Value, String> {
    if fixture.case.operation != Op::NAME {
        return Err(format!(
            "{}: operation {} is not {}",
            fixture.name,
            fixture.case.operation,
            Op::NAME
        ));
    }
    // A case without `params` has none, as an operation with no parameters expects.
    let mut params = fixture.case.params.clone();
    if params.is_null() {
        params = Value::Object(serde_json::Map::new());
    }
    let params = serde_json::from_value(params)
        .map_err(|e| format!("{}: params of {}: {e}", fixture.name, Op::NAME))?;
    Op::run(&fixture.input, params).map_err(|e| format!("{}: {e}", fixture.name))
}

/// The parameters of an operation that takes none: only an empty map fits.
#[derive(Debug)]
pub struct NoParams;

impl<'de> Deserialize<'de> for NoParams {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let given = Value::deserialize(deserializer)?;
        if given.as_object().is_some_and(serde_json::Map::is_empty) {
            Ok(Self)
        } else {
            Err(serde::de::Error::custom(format!(
                "expected no parameters, got {given}"
            )))
        }
    }
}

/// A parameter of a type that ignores unknown fields (a wire type): reading it fails when the
/// fixture holds a field the type drops.
#[derive(Debug, Clone, Default)]
pub struct Strict<T>(pub T);

impl<'de, T: DeserializeOwned + Serialize> Deserialize<'de> for Strict<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let given = Value::deserialize(deserializer)?;
        let value: T = serde_json::from_value(given.clone()).map_err(D::Error::custom)?;
        let kept = serde_json::to_value(&value).map_err(D::Error::custom)?;
        if let Some(path) = dropped("$", &given, &kept) {
            return Err(D::Error::custom(format!("unknown field at {path}")));
        }
        Ok(Self(value))
    }
}

/// The first place where `given` has a value that `kept` lacks.
fn dropped(path: &str, given: &Value, kept: &Value) -> Option<String> {
    match (given, kept) {
        (Value::Object(given), Value::Object(kept)) => given.iter().find_map(|(key, value)| {
            let at = format!("{path}.{key}");
            match kept.get(key) {
                Some(kept) => dropped(&at, value, kept),
                None => (!value.is_null()).then_some(at),
            }
        }),
        (Value::Array(given), Value::Array(kept)) => given
            .iter()
            .zip(kept)
            .enumerate()
            .find_map(|(index, (g, k))| dropped(&format!("{path}[{index}]"), g, k)),
        _ => None,
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
    use serde::Deserialize;
    use serde_json::{Value, json};

    use super::{Case, Fixture, FixtureOp, Strict, compare, run_op};

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

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Pair {
        left: u32,
        #[serde(default)]
        nested: Option<Strict<Vec<Leaf>>>,
    }

    impl Pair {
        fn weight(&self) -> usize {
            self.left as usize + self.nested.as_ref().map_or(0, |n| n.0.len())
        }
    }

    #[derive(Deserialize, serde::Serialize)]
    struct Leaf {
        name: String,
    }

    struct Add;

    impl FixtureOp for Add {
        const NAME: &'static str = "test.add";
        type Params = Pair;
        fn run(_: &[Value], params: Pair) -> Result<Value, String> {
            Ok(json!(params.weight()))
        }
    }

    fn case(operation: &str, params: Value) -> Fixture {
        Fixture {
            name: "test/case".to_owned(),
            dir: "".into(),
            case: Case {
                description: String::new(),
                operation: operation.to_owned(),
                params,
            },
            input: Vec::new(),
            expected: Value::Null,
        }
    }

    #[test]
    fn a_case_runs_with_its_parameters() {
        assert_eq!(
            run_op::<Add>(&case("test.add", json!({"left": 2}))),
            Ok(json!(2))
        );
    }

    #[test]
    fn a_misspelt_extra_or_mistyped_parameter_fails_the_load() {
        for params in [
            json!({"lefft": 2}),
            json!({"left": 2, "right": 1}),
            json!({"left": "2"}),
            json!({}),
            Value::Null,
        ] {
            assert!(
                run_op::<Add>(&case("test.add", params.clone())).is_err(),
                "{params}"
            );
        }
    }

    #[test]
    fn a_nested_wire_type_refuses_a_field_it_would_drop() {
        let nested =
            |leaf: Value| run_op::<Add>(&case("test.add", json!({"left": 1, "nested": [leaf]})));
        assert!(nested(json!({"name": "a"})).is_ok());
        let error = nested(json!({"name": "a", "colour": "red"})).unwrap_err();
        assert!(error.contains("$[0].colour"), "{error}");
    }

    #[test]
    fn parameters_given_to_an_operation_without_any_fail_the_load() {
        struct Nothing;
        impl FixtureOp for Nothing {
            const NAME: &'static str = "test.nothing";
            type Params = super::NoParams;
            fn run(_: &[Value], _: Self::Params) -> Result<Value, String> {
                Ok(json!(true))
            }
        }
        assert!(run_op::<Nothing>(&case("test.nothing", Value::Null)).is_ok());
        assert!(run_op::<Nothing>(&case("test.nothing", json!({"x": 1}))).is_err());
    }

    #[test]
    fn a_case_of_another_operation_is_refused() {
        assert!(run_op::<Add>(&case("test.sub", json!({"left": 1}))).is_err());
    }
}
