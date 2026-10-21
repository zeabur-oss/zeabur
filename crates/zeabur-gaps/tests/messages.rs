//! Every sentence the fixtures expect has words in every language.

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use serde_json::Value;
    use zeabur_gaps::messages::{Language, template};

    fn keys(value: &Value, found: &mut BTreeSet<String>) {
        match value {
            Value::Object(object) => {
                if let Some(Value::String(key)) = object.get("key")
                    && object.get("params").is_none_or(Value::is_object)
                    && ["diagnosis.", "gap.", "trace.", "incident."]
                        .iter()
                        .any(|scope| key.starts_with(scope))
                {
                    found.insert(key.clone());
                }
                object.values().for_each(|value| keys(value, found));
            }
            Value::Array(values) => values.iter().for_each(|value| keys(value, found)),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        }
    }

    fn walk(dir: &Path, found: &mut BTreeSet<String>) {
        for entry in std::fs::read_dir(dir).expect("a fixtures directory") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                walk(&path, found);
                continue;
            }
            let value: Option<Value> = match path.extension().and_then(|e| e.to_str()) {
                Some("json") => serde_json::from_slice(&std::fs::read(&path).expect("read")).ok(),
                Some("yaml") => serde_saphyr::from_slice(&std::fs::read(&path).expect("read")).ok(),
                _ => None,
            };
            if let Some(value) = value {
                keys(&value, found);
            }
        }
    }

    #[test]
    fn every_key_in_the_fixtures_has_words_in_every_language() {
        let mut found = BTreeSet::new();
        walk(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures"),
            &mut found,
        );
        assert!(found.len() > 20, "{found:?}");
        let missing: Vec<_> = found
            .iter()
            .flat_map(|key| Language::ALL.map(|language| (key, language)))
            .filter(|(key, language)| template(key, *language).is_none())
            .map(|(key, language)| format!("{} {key}", language.tag()))
            .collect();
        assert_eq!(missing, Vec::<String>::new());
    }
}
