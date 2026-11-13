//! Where a container gets an environment variable, as far as it can be proven.
//!
//! Kubernetes gives a container `env` over every `envFrom` import, and among the imports the last
//! one that holds the key wins [k8s-env]. A prefix turns a key `ORT` into `PORT`. Anything the
//! plan cannot see (an object that was not listed, a value that was not readable) may hold the
//! key too. So an import is named as the source only when everything after it is known not to
//! hold the key; in every other case the answer is [`Source::Unknown`], and the caller sets the
//! variable in the container's own `env`, which wins whatever the imports hold.
//!
//! [k8s-env]: https://kubernetes.io/docs/tasks/inject-data-application/define-environment-variable-container/#using-environment-variables-inside-of-your-config

use k8s_openapi::api::core::v1::Container;

use crate::change::DataKind;

/// Whether an imported object holds a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Holds {
    Yes,
    No,
    /// The plan has not seen the object or its values.
    Unknown,
}

/// Where the value of a variable comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Source {
    /// The container sets it in `env`.
    Own,
    /// The last import that holds it, with nothing after it that could.
    Import(DataKind, String),
    /// No import is sure to be the one.
    Unknown,
}

/// The source of `key` in `container`. `holds` answers for a ConfigMap or Secret and a key in it.
pub(crate) fn source_of(
    container: &Container,
    key: &str,
    holds: impl Fn(DataKind, &str, &str) -> Holds,
) -> Source {
    if container.env.iter().flatten().any(|e| e.name == key) {
        return Source::Own;
    }
    for import in container.env_from.iter().flatten().rev() {
        let prefix = import.prefix.as_deref().unwrap_or_default();
        let Some(inner) = key.strip_prefix(prefix).filter(|k| !k.is_empty()) else {
            continue;
        };
        let (kind, name) = match (&import.secret_ref, &import.config_map_ref) {
            (Some(secret), _) => (DataKind::Secret, &secret.name),
            (None, Some(map)) => (DataKind::ConfigMap, &map.name),
            (None, None) => continue,
        };
        match holds(kind, name, inner) {
            Holds::No => {}
            Holds::Yes if prefix.is_empty() => return Source::Import(kind, name.clone()),
            Holds::Yes | Holds::Unknown => return Source::Unknown,
        }
    }
    Source::Unknown
}

#[cfg(test)]
mod tests {
    use k8s_openapi::api::core::v1::{EnvFromSource, EnvVar};
    use serde_json::{from_value, json};

    use super::*;

    fn imports(names: &[(&str, &str)]) -> Container {
        let from: Vec<EnvFromSource> = names
            .iter()
            .map(|(kind, name)| match *kind {
                "secret" => from_value(json!({"secretRef": {"name": name}})).unwrap(),
                _ => from_value(json!({"configMapRef": {"name": name}})).unwrap(),
            })
            .collect();
        Container {
            env_from: Some(from),
            ..Container::default()
        }
    }

    fn answers(table: &'static [(&'static str, Holds)]) -> impl Fn(DataKind, &str, &str) -> Holds {
        move |_, name, _| {
            table
                .iter()
                .find(|(n, _)| *n == name)
                .map_or(Holds::Unknown, |(_, h)| *h)
        }
    }

    #[test]
    fn the_container_s_own_variable_wins() {
        let mut c = imports(&[("secret", "a")]);
        c.env = Some(vec![EnvVar {
            name: "PORT".to_owned(),
            ..EnvVar::default()
        }]);
        assert_eq!(
            source_of(&c, "PORT", answers(&[("a", Holds::Yes)])),
            Source::Own
        );
    }

    #[test]
    fn the_last_import_that_holds_the_key_is_the_source() {
        let c = imports(&[("secret", "a"), ("config", "b"), ("secret", "c")]);
        let table = &[("a", Holds::Yes), ("b", Holds::Yes), ("c", Holds::No)];
        assert_eq!(
            source_of(&c, "PORT", answers(table)),
            Source::Import(DataKind::ConfigMap, "b".to_owned())
        );
    }

    #[test]
    fn an_import_that_was_not_seen_after_the_holder_makes_it_unknown() {
        let c = imports(&[("config", "a"), ("secret", "gone")]);
        let table = &[("a", Holds::Yes)];
        assert_eq!(source_of(&c, "PORT", answers(table)), Source::Unknown);
    }

    #[test]
    fn a_prefixed_import_that_can_spell_the_key_makes_it_unknown() {
        let mut c = imports(&[("secret", "a"), ("config", "b")]);
        c.env_from.as_mut().unwrap()[1].prefix = Some("P".to_owned());
        let table = &[("a", Holds::Yes), ("b", Holds::Yes)];
        assert_eq!(source_of(&c, "PORT", answers(table)), Source::Unknown);
    }

    #[test]
    fn a_prefix_that_cannot_spell_the_key_is_skipped() {
        let mut c = imports(&[("secret", "a"), ("config", "b")]);
        c.env_from.as_mut().unwrap()[1].prefix = Some("X_".to_owned());
        let table = &[("a", Holds::Yes), ("b", Holds::Yes)];
        assert_eq!(
            source_of(&c, "PORT", answers(table)),
            Source::Import(DataKind::Secret, "a".to_owned())
        );
    }

    #[test]
    fn a_holder_with_a_prefix_is_not_a_source_to_edit() {
        let mut c = imports(&[("secret", "a")]);
        c.env_from.as_mut().unwrap()[0].prefix = Some("P".to_owned());
        assert_eq!(
            source_of(&c, "PORT", answers(&[("a", Holds::Yes)])),
            Source::Unknown
        );
    }
}
