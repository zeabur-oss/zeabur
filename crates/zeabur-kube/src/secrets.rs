//! Reading a Secret without its values.
//!
//! A Secret that comes back from the generic read path is decoded here, from the bytes
//! kube-apiserver sent, into a struct that has no place for a value. `data` and `stringData` are
//! read as their keys, and the parser skips every value without building a string for it, so a
//! value never exists in memory as anything but the raw response. What is left of the object is
//! the type, the metadata with the last-applied annotation hidden, `immutable`, and the keys,
//! each with an empty value, as the pages that count keys expect.

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::{DynamicObject, TypeMeta};
use serde::Deserialize;
use serde::de::{IgnoredAny, MapAccess, Visitor};

use crate::objects::hide_last_applied;
use crate::port::ClusterError;

/// The keys of a `data` or `stringData` map. Their values are skipped.
#[derive(Default)]
struct Keys(Vec<String>);

impl<'de> Deserialize<'de> for Keys {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KeysOnly;

        impl<'de> Visitor<'de> for KeysOnly {
            type Value = Keys;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a map")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Keys, A::Error> {
                let mut keys = Vec::new();
                while let Some(key) = map.next_key::<String>()? {
                    map.next_value::<IgnoredAny>()?;
                    keys.push(key);
                }
                Ok(Keys(keys))
            }
        }

        deserializer.deserialize_map(KeysOnly)
    }
}

impl Keys {
    fn emptied(self) -> serde_json::Value {
        serde_json::Value::Object(
            self.0
                .into_iter()
                .map(|key| (key, serde_json::Value::String(String::new())))
                .collect(),
        )
    }
}

#[derive(Deserialize)]
struct SlimSecret {
    #[serde(rename = "apiVersion")]
    api_version: Option<String>,
    kind: Option<String>,
    #[serde(default)]
    metadata: ObjectMeta,
    #[serde(rename = "type")]
    type_: Option<String>,
    data: Option<Keys>,
    #[serde(rename = "stringData")]
    string_data: Option<Keys>,
    immutable: Option<bool>,
}

impl From<SlimSecret> for DynamicObject {
    fn from(secret: SlimSecret) -> Self {
        let mut metadata = secret.metadata;
        hide_last_applied(&mut metadata);
        let mut data = serde_json::Map::new();
        if let Some(type_) = secret.type_ {
            data.insert("type".to_owned(), type_.into());
        }
        if let Some(keys) = secret.data {
            data.insert("data".to_owned(), keys.emptied());
        }
        if let Some(keys) = secret.string_data {
            data.insert("stringData".to_owned(), keys.emptied());
        }
        if let Some(immutable) = secret.immutable {
            data.insert("immutable".to_owned(), immutable.into());
        }
        Self {
            types: secret
                .api_version
                .zip(secret.kind)
                .map(|(api_version, kind)| TypeMeta { api_version, kind }),
            metadata,
            data: serde_json::Value::Object(data),
        }
    }
}

/// The Secrets of a LIST answer, without their values.
pub(crate) fn list(body: &[u8]) -> Result<Vec<DynamicObject>, ClusterError> {
    #[derive(Deserialize)]
    struct Items {
        #[serde(default)]
        items: Vec<SlimSecret>,
    }
    serde_json::from_slice::<Items>(body)
        .map(|list| list.items.into_iter().map(DynamicObject::from).collect())
        .map_err(|e| ClusterError::Decode(e.to_string()))
}

/// The Secret of a GET answer, without its values.
pub(crate) fn one(body: &[u8]) -> Result<DynamicObject, ClusterError> {
    serde_json::from_slice::<SlimSecret>(body)
        .map(DynamicObject::from)
        .map_err(|e| ClusterError::Decode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::{HIDDEN, LAST_APPLIED};

    const SECRET: &str = r#"{
        "apiVersion": "v1", "kind": "Secret", "type": "Opaque", "immutable": true,
        "metadata": {"name": "db", "namespace": "shop", "annotations": {
            "kubectl.kubernetes.io/last-applied-configuration":
                "{\"data\":{\"password\":\"aHVudGVyMg==\"}}",
            "team": "data"}},
        "data": {"password": "aHVudGVyMg==", "user": "YWRtaW4="},
        "stringData": {"token": "s3cr3t"}
    }"#;

    #[test]
    fn a_secret_keeps_its_keys_and_type_and_loses_its_values() {
        let object = one(SECRET.as_bytes()).unwrap();
        let text = serde_json::to_string(&object).unwrap();
        for value in ["aHVudGVyMg==", "YWRtaW4=", "s3cr3t"] {
            assert!(!text.contains(value), "{text}");
        }
        assert_eq!(object.data["type"], "Opaque");
        assert_eq!(object.data["immutable"], true);
        assert_eq!(object.data["data"]["password"], "");
        assert_eq!(object.data["data"]["user"], "");
        assert_eq!(object.data["stringData"]["token"], "");
        assert_eq!(object.types.as_ref().unwrap().kind, "Secret");
        let annotations = object.metadata.annotations.unwrap();
        assert_eq!(annotations[LAST_APPLIED], HIDDEN);
        assert_eq!(annotations["team"], "data");
    }

    #[test]
    fn a_list_of_secrets_loses_every_value() {
        let body = format!(
            r#"{{"items": [{SECRET}, {{"metadata": {{"name": "empty"}}, "data": null}}]}}"#
        );
        let objects = list(body.as_bytes()).unwrap();
        assert_eq!(objects.len(), 2);
        let text = serde_json::to_string(&objects).unwrap();
        assert!(
            !text.contains("aHVudGVyMg==") && !text.contains("s3cr3t"),
            "{text}"
        );
        assert_eq!(objects[0].data["data"].as_object().unwrap().len(), 2);
        assert!(objects[1].data.get("data").is_none());
    }

    #[test]
    fn a_secret_that_is_not_an_object_is_a_decode_error() {
        assert!(matches!(
            one(br#"{"metadata": {"name": "db"}, "data": ["x"]}"#),
            Err(ClusterError::Decode(_))
        ));
    }
}
