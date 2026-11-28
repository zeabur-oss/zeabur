//! The one place that builds a write's query and sends a raw request to kube-apiserver, for what
//! kube-client cannot do yet. Each workaround names the kube-rs issue that removes it; when kube-rs
//! ships the fix, this module calls kube-rs and its callers do not change.
//!
//! - **Query.** Creations and patches name `fieldManager=zeabur` [k8s-field-manager] and ask for
//!   `fieldValidation=Strict` [k8s-validation]; a dry run adds `dryRun=All` [k8s-dry-run]. A
//!   deletion writes no fields: its dry run goes in its body ([`delete_params`]).
//! - **Send.** [`send`] goes through `Client::send` and reads the `Warning` headers of the answer.
//!
//! kubeconfig, authentication, TLS, discovery, the exec WebSocket and kube-runtime stay on kube-rs,
//! as do reads, which kube-client sends and answers in full.
//!
//! [k8s-field-manager]: https://kubernetes.io/docs/reference/using-api/server-side-apply/#field-management
//! [k8s-validation]: https://kubernetes.io/docs/reference/using-api/api-concepts/#field-validation
//! [k8s-dry-run]: https://kubernetes.io/docs/reference/using-api/api-concepts/#dry-run

use kube::api::{DeleteParams, PatchParams};
use kube::core::Status;

use crate::port::{ClusterError, FIELD_MANAGER, Preconditions};

/// A `POST` of `object` to the collection at `path`, with the query of [`query`].
///
/// This is a workaround. When kube's `PostParams` can ask for field validation
/// ([kube-rs/kube#2177]), build it with `kube::core::Request::create` and remove this function.
///
/// [kube-rs/kube#2177]: https://github.com/kube-rs/kube/issues/2177
pub(crate) fn create(
    path: &str,
    object: &serde_json::Value,
    dry_run: bool,
) -> Result<http::Request<Vec<u8>>, ClusterError> {
    let body = serde_json::to_vec(object).map_err(|e| ClusterError::Decode(e.to_string()))?;
    http::Request::post(format!("{path}?{}", query(dry_run)))
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(http::header::ACCEPT, "application/json")
        .body(body)
        .map_err(|e| ClusterError::Transport(e.to_string()))
}

/// The query string of a creation.
fn query(dry_run: bool) -> String {
    let mut parts = vec![
        format!("fieldManager={FIELD_MANAGER}"),
        "fieldValidation=Strict".to_owned(),
    ];
    if dry_run {
        parts.push("dryRun=All".to_owned());
    }
    parts.join("&")
}

/// The query of a patch: the field manager, strict field validation, and `dryRun=All` for a dry
/// run.
pub(crate) fn patch_params(dry_run: bool) -> PatchParams {
    let params = PatchParams {
        dry_run,
        field_manager: Some(FIELD_MANAGER.to_owned()),
        ..PatchParams::default()
    };
    params.validation_strict()
}

/// The options of a deletion [k8s-delete-options], which kube sends as its body, the dry run
/// included: kube-apiserver reads a deletion's options from its body when it has one and only
/// otherwise from the query string [delete-handler], so a `dryRun=All` query beside a body would
/// be ignored and the object deleted for real.
///
/// [k8s-delete-options]: https://kubernetes.io/docs/reference/kubernetes-api/common-definitions/delete-options/
/// [delete-handler]: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/apiserver/pkg/endpoints/handlers/delete.go
pub(crate) fn delete_params(preconditions: &Preconditions, dry_run: bool) -> DeleteParams {
    DeleteParams {
        dry_run,
        preconditions: Some(kube::api::Preconditions {
            uid: preconditions.uid.clone(),
            resource_version: preconditions.resource_version.clone(),
        }),
        grace_period_seconds: preconditions.grace_period_seconds,
        ..if preconditions.foreground {
            DeleteParams::foreground()
        } else {
            DeleteParams::background()
        }
    }
}

/// Sends `request` and returns the body of the answer and the text of its `Warning` headers.
/// kube's typed calls (`Api::patch`, `Client::request`) read only the body, so the request goes
/// through `Client::send`, and an error status becomes the error kube would give.
///
/// This is a workaround. When kube-client gives callers the `Warning` headers of an answer
/// ([kube-rs/kube#2094]), use the typed calls again and remove this function.
///
/// [kube-rs/kube#2094]: https://github.com/kube-rs/kube/issues/2094
pub(crate) async fn send(
    client: &kube::Client,
    request: http::Request<Vec<u8>>,
) -> Result<(Vec<u8>, Vec<String>), ClusterError> {
    let answer = client.send(request.map(kube::client::Body::from)).await?;
    let status = answer.status();
    let warnings = warnings(answer.headers());
    let body = answer.into_body().collect_bytes().await?.to_vec();
    if status.is_client_error() || status.is_server_error() {
        return Err(refusal(status, &body).into());
    }
    Ok((body, warnings))
}

/// The error kube gives for an answer with an error `status`.
///
/// This is a workaround: a copy of kube-client's private `handle_api_errors`, which its typed
/// calls use. It goes with [`send`], when kube-client gives callers the `Warning` headers
/// ([kube-rs/kube#2094]).
///
/// [kube-rs/kube#2094]: https://github.com/kube-rs/kube/issues/2094
fn refusal(status: http::StatusCode, body: &[u8]) -> kube::Error {
    let refusal = serde_json::from_slice::<Status>(body).unwrap_or_else(|_not_a_status| {
        Status::failure(
            String::from_utf8_lossy(body).trim(),
            "Failed to parse error data",
        )
        .with_code(status.as_u16())
    });
    kube::Error::Api(Box::new(refusal))
}

/// The text of each `Warning` header [k8s-warnings]. kube-apiserver sends one header per warning,
/// `299 - "<text>"`, the format of RFC 7234, section 5.5 [rfc7234-warning]; a value in another
/// form is kept as it is. The text may hold any UTF-8, such as a webhook's message in another
/// language; bytes that are not UTF-8 become U+FFFD.
///
/// [k8s-warnings]: https://kubernetes.io/blog/2020/09/03/warnings/
/// [rfc7234-warning]: https://www.rfc-editor.org/rfc/rfc7234#section-5.5
fn warnings(headers: &http::HeaderMap) -> Vec<String> {
    headers
        .get_all(http::header::WARNING)
        .iter()
        .map(|value| String::from_utf8_lossy(value.as_bytes()))
        .flat_map(|value| warn_texts(&value).unwrap_or_else(|| vec![value.trim().to_owned()]))
        .collect()
}

/// The `warn-text` of each `warning-value` in `value`; `None` when it does not parse.
fn warn_texts(value: &str) -> Option<Vec<String>> {
    let mut texts = Vec::new();
    let mut rest = value.trim_start();
    loop {
        let (code, after) = rest.split_once(' ')?;
        if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let (_agent, after) = after.split_once(' ')?;
        let mut chars = after.strip_prefix('"')?.char_indices();
        let mut text = String::new();
        let end = loop {
            match chars.next()? {
                (_, '\\') => text.push(chars.next()?.1),
                (at, '"') => break at,
                (_, other) => text.push(other),
            }
        };
        texts.push(text);
        // After the closing quote: an optional ` "<date>"`, then `,` and the next value.
        let mut after = after.get(end.saturating_add(2)..)?.trim_start();
        if let Some(date) = after.strip_prefix('"') {
            after = date.split_once('"')?.1.trim_start();
        }
        if after.is_empty() {
            return Some(texts);
        }
        rest = after.strip_prefix(',')?.trim_start();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_write_names_the_manager_and_asks_for_strict_validation() {
        assert_eq!(
            query(true),
            "fieldManager=zeabur&fieldValidation=Strict&dryRun=All"
        );
        assert_eq!(query(false), "fieldManager=zeabur&fieldValidation=Strict");
        let params = patch_params(true);
        assert_eq!(params.field_manager.as_deref(), Some("zeabur"));
        assert_eq!(
            params
                .field_validation
                .as_ref()
                .map(kube::api::ValidationDirective::as_str),
            Some("Strict")
        );
        assert!(params.dry_run);
        assert!(!patch_params(false).dry_run);
        let created = create(
            "/api/v1/namespaces/shop/configmaps",
            &serde_json::json!({}),
            true,
        )
        .unwrap();
        assert_eq!(
            created.uri(),
            "/api/v1/namespaces/shop/configmaps?fieldManager=zeabur&fieldValidation=Strict&dryRun=All"
        );
    }

    #[test]
    fn a_deletion_propagates_in_the_background_and_names_its_preconditions() {
        let preconditions = Preconditions {
            uid: Some("u-1".to_owned()),
            resource_version: Some("42".to_owned()),
            grace_period_seconds: None,
            foreground: false,
        };
        let body = |dry_run| serde_json::to_value(delete_params(&preconditions, dry_run)).unwrap();
        let options = body(false);
        assert_eq!(options["propagationPolicy"], "Background");
        assert_eq!(
            options["preconditions"],
            serde_json::json!({"uid": "u-1", "resourceVersion": "42"})
        );
        assert_eq!(options.get("dryRun"), None);
        assert_eq!(body(true)["dryRun"], serde_json::json!(["All"]));
    }

    #[test]
    fn a_foreground_deletion_asks_for_foreground_propagation() {
        let preconditions = Preconditions {
            uid: Some("u-1".to_owned()),
            resource_version: Some("42".to_owned()),
            grace_period_seconds: None,
            foreground: true,
        };
        assert_eq!(
            serde_json::to_value(delete_params(&preconditions, false)).unwrap(),
            serde_json::json!({
                "propagationPolicy": "Foreground",
                "preconditions": { "uid": "u-1", "resourceVersion": "42" },
            })
        );
    }

    #[test]
    fn each_warning_header_gives_its_text() {
        let mut headers = http::HeaderMap::new();
        for value in [
            r#"299 - "would violate PodSecurity \"restricted:latest\": privileged""#,
            r#"299 - "first", 299 - "second" "Sat, 28 Nov 2026 00:00:00 GMT""#,
            "not a warning",
        ] {
            headers.append(http::header::WARNING, http::HeaderValue::from_static(value));
        }
        let localized = r#"299 - "映像檔已過時""#.as_bytes();
        headers.append(
            http::header::WARNING,
            http::HeaderValue::from_bytes(localized).unwrap(),
        );
        assert_eq!(
            warnings(&headers),
            [
                r#"would violate PodSecurity "restricted:latest": privileged"#,
                "first",
                "second",
                "not a warning",
                "映像檔已過時",
            ]
        );
    }

    #[test]
    fn a_warning_that_does_not_parse_gives_none() {
        for value in [
            "",
            r#"29 - "short code""#,
            "299 - unquoted",
            r#"299 - "unclosed"#,
            r#"299 - "one" trailing"#,
        ] {
            assert_eq!(warn_texts(value), None, "{value}");
        }
    }
}
