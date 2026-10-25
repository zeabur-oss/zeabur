//! Every case in each crate's `fixtures/` loads, names a documented operation, and its input
//! objects are well-formed Kubernetes objects.

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::Value;
    use zeabur_test_support::fixtures::load_from;

    /// Each area, and the crate whose `fixtures/` holds it.
    const AREAS: [(&str, &str); 5] = [
        ("graph", "zeabur-graph"),
        ("projection", "zeabur-graph"),
        ("mutation", "zeabur-plan"),
        ("gaps", "zeabur-gaps"),
        ("modules", "zeabur-modules"),
    ];

    #[test]
    fn every_fixture_loads_and_is_well_formed() {
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let readme = std::fs::read_to_string(crates.join("../fixtures/README.md"))
            .expect("fixtures/README.md");
        let mut total = 0;
        for (area, owner) in AREAS {
            let root = crates.join(owner).join("fixtures");
            for fixture in load_from(&root, area).expect("fixtures load") {
                total += 1;
                let name = &fixture.name;
                assert!(
                    fixture.case.operation.starts_with(&format!("{area}.")),
                    "{name}: operation {} is not in area {area}",
                    fixture.case.operation
                );
                assert!(
                    readme.contains(&format!("`{}`", fixture.case.operation)),
                    "{name}: operation {} is not documented in fixtures/README.md",
                    fixture.case.operation
                );
                assert!(
                    !fixture.case.description.trim().is_empty(),
                    "{name}: no description"
                );
                for object in &fixture.input {
                    for field in ["apiVersion", "kind"] {
                        assert!(
                            object.get(field).and_then(Value::as_str).is_some(),
                            "{name}: an input object has no {field}"
                        );
                    }
                    assert!(
                        object
                            .pointer("/metadata/name")
                            .and_then(Value::as_str)
                            .is_some(),
                        "{name}: an input object has no metadata.name"
                    );
                }
                if let Some(code) = fixture.expected_error() {
                    assert!(
                        code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                        "{name}: error code {code:?} is not snake_case"
                    );
                }
            }
        }
        assert!(total > 0, "no fixtures found");
    }
}
