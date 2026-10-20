//! A project's ConfigMaps, and a Secret's values on request.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::ClusterExt as _;
use zeabur_kube::k8s_openapi::api::core::v1::ConfigMap;
use zeabur_model::{Origin, ResourceKind, id};

use crate::error::{ApiError, get_read, list_read, read_problem};
use crate::handlers::Person;
use crate::views::{ConfigMapList, ConfigMapView, RevealedSecret, SecretValue};

/// The ConfigMaps of a project, in full, sorted by name. Lists ConfigMaps only.
#[utoipa::path(get, path = "/api/v1/projects/{project}/configmaps", tag = "resources",
    params(("project" = String, Path)),
    responses((status = 200, body = ConfigMapList), (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn config_maps(
    Person(cluster): Person,
    Path(project): Path<String>,
) -> Result<Json<ConfigMapList>, ApiError> {
    let config_maps = cluster
        .list::<ConfigMap>(Some(&project))
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("ConfigMaps in project {project}"),
                list_read("", "configmaps", Some(&project)),
            )
        })?;
    let mut items: Vec<ConfigMapView> = config_maps
        .into_iter()
        .map(|cm| {
            let name = cm.metadata.name.as_deref().unwrap_or_default();
            ConfigMapView {
                id: id::resource(&project, ResourceKind::ConfigMap, name),
                origin: Origin::complete("v1", "ConfigMap", Some(&project), name),
                data: cm.data.unwrap_or_default(),
                binary_keys: cm.binary_data.unwrap_or_default().into_keys().collect(),
            }
        })
        .collect();
    items.sort_by(|a, b| a.origin.name.cmp(&b.origin.name));
    Ok(Json(ConfigMapList { items }))
}

/// A Secret's values. A `POST`, so that it is always a deliberate action and never prefetched;
/// the response is never cached, and nothing in Zeabur logs or keeps it.
#[utoipa::path(post, path = "/api/v1/projects/{project}/secrets/{name}/reveal", tag = "resources",
    params(("project" = String, Path), ("name" = String, Path)),
    responses((status = 200, body = RevealedSecret), (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn reveal_secret(
    Person(cluster): Person,
    Path((project, name)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let values = cluster.reveal_secret(&project, &name).await.map_err(|e| {
        read_problem(
            e,
            &format!("secret {name}"),
            get_read("", "secrets", Some(&project), &name),
        )
    })?;
    let body = RevealedSecret {
        origin: Origin::complete("v1", "Secret", Some(&project), &name),
        values: values
            .into_iter()
            .map(|(key, bytes)| (key, secret_value(bytes)))
            .collect::<BTreeMap<_, _>>(),
    };
    // `Cache-Control: no-store`, as on every answer, comes from the router (`crate::cache`).
    Ok(Json(body).into_response())
}

fn secret_value(bytes: Vec<u8>) -> SecretValue {
    match String::from_utf8(bytes) {
        Ok(text) => SecretValue {
            value: text,
            encoding: "utf-8".to_owned(),
        },
        Err(error) => SecretValue {
            value: base64(error.as_bytes()),
            encoding: "base64".to_owned(),
        },
    }
}

/// Standard base64 with padding (RFC 4648 §4).
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk.first().copied().unwrap_or(0),
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18_u32, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                let index = usize::try_from((n >> shift) & 0x3F).unwrap_or(0);
                out.push(char::from(ALPHABET.get(index).copied().unwrap_or(b'=')));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc_4648_test_vectors() {
        for (input, output) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), output);
        }
    }

    #[test]
    fn binary_values_are_base64_and_text_is_kept() {
        assert_eq!(secret_value(b"hunter2".to_vec()).encoding, "utf-8");
        let binary = secret_value(vec![0xFF, 0x00]);
        assert_eq!(
            (binary.value.as_str(), binary.encoding.as_str()),
            ("/wA=", "base64")
        );
    }
}
