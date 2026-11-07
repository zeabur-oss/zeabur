//! API tokens: a ServiceAccount in [`NAMESPACE`], labelled [`LABEL`] with its own name, and the
//! tokens kube-apiserver issues for it. Its rights are bindings like a member's (see
//! `zeabur_api::members::access`).
//!
//! Nothing else runs in [`NAMESPACE`] and only cluster administrators have rights there, so only
//! they make tokens, request tokens or run a pod as one.
//!
//! A token is never stored. Each comes from a `TokenRequest` (`create` of `serviceaccounts/token`)
//! [k8s-token-request] and expires. Renewing asks for another token for the same ServiceAccount.
//! kube-apiserver may cut the expiry asked for (`--service-account-max-token-expiration`), so the
//! expiry it answered is recorded in [`EXPIRES_AT_ANNOTATION`], for the latest token only.
//! Deleting the ServiceAccount revokes every token issued for it: kube-apiserver checks that the
//! ServiceAccount still exists, with the same `uid`, on each request [k8s-sa-tokens].
//!
//! # References
//!
//! - [k8s-token-request] The Kubernetes Authors, "`TokenRequest`," Kubernetes API Reference. <https://kubernetes.io/docs/reference/kubernetes-api/authentication-resources/token-request-v1/>
//! - [k8s-sa-tokens] The Kubernetes Authors, "Managing Service Accounts," Kubernetes Documentation.
//!   <https://kubernetes.io/docs/reference/access-authn-authz/service-accounts-admin/#bound-service-account-tokens>
//!
//! [k8s-token-request]: https://kubernetes.io/docs/reference/kubernetes-api/authentication-resources/token-request-v1/
//! [k8s-sa-tokens]: https://kubernetes.io/docs/reference/access-authn-authz/service-accounts-admin/#bound-service-account-tokens

use serde_json::{Value, json};
pub use zeabur_model::kernel::names::API_TOKENS_NAMESPACE as NAMESPACE;

use crate::json::{add, pointer, test};

/// Label on an API token's ServiceAccount and its bindings, with the token's name.
pub const LABEL: &str = "zeabur.com/api-token";
/// Annotation on the ServiceAccount: the Kubernetes user name of the person who made it.
pub const CREATED_BY_ANNOTATION: &str = "zeabur.com/api-token-created-by";
/// Annotation on the ServiceAccount: when its latest token expires, RFC 3339.
pub const EXPIRES_AT_ANNOTATION: &str = "zeabur.com/api-token-expires-at";
/// The shortest a token may last: kube-apiserver refuses less than ten minutes.
pub const MIN_SECONDS: i64 = 600;
/// The longest a token may be asked for.
pub const MAX_SECONDS: i64 = 365 * 86_400;

const MANAGED_BY: &str = "app.kubernetes.io/managed-by";

/// The ServiceAccount of the token `name`.
#[must_use]
pub fn service_account(name: &str, created_by: &str) -> Value {
    json!({
        "apiVersion": "v1",
        "kind": "ServiceAccount",
        "metadata": {
            "name": name,
            "namespace": NAMESPACE,
            "labels": {MANAGED_BY: "zeabur", LABEL: name},
            "annotations": {CREATED_BY_ANNOTATION: created_by},
        },
        "automountServiceAccountToken": false,
    })
}

/// The `TokenRequest` as the plan shows it.
#[must_use]
pub fn token_request(seconds: i64) -> Value {
    json!({
        "apiVersion": "authentication.k8s.io/v1",
        "kind": "TokenRequest",
        "spec": {"expirationSeconds": seconds},
    })
}

/// The JSON Patch that records `expires_at` on the ServiceAccount of the token `name`; its `test`
/// refuses a ServiceAccount that is no longer that token.
#[must_use]
pub fn expiry_patch(name: &str, expires_at: &str) -> Vec<Value> {
    vec![
        test(&pointer(&["metadata", "labels", LABEL]), json!(name)),
        add(
            &pointer(&["metadata", "annotations", EXPIRES_AT_ANNOTATION]),
            json!(expires_at),
        ),
    ]
}

/// Why a token cannot be made as asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The name is not a DNS label.
    Name,
    /// The expiry is outside [`MIN_SECONDS`]..=[`MAX_SECONDS`].
    Seconds,
}

/// Checks a new token's name.
///
/// # Errors
///
/// [`Refusal::Name`] for a name that is not a DNS label.
pub fn check_name(name: &str) -> Result<(), Refusal> {
    if crate::checks::is_dns_label(name) {
        Ok(())
    } else {
        Err(Refusal::Name)
    }
}

/// Checks how long a token is asked to last.
///
/// # Errors
///
/// [`Refusal::Seconds`] for an expiry out of range.
pub fn check_seconds(seconds: i64) -> Result<(), Refusal> {
    if (MIN_SECONDS..=MAX_SECONDS).contains(&seconds) {
        Ok(())
    } else {
        Err(Refusal::Seconds)
    }
}

/// An API token, as its ServiceAccount records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The token's name, which is the ServiceAccount's.
    pub name: String,
    /// Who made it.
    pub created_by: Option<String>,
    /// `metadata.creationTimestamp`.
    pub created_at: Option<String>,
    /// When the latest token expires.
    pub expires_at: Option<String>,
}

/// The token a ServiceAccount is, when its [`LABEL`] names it.
#[must_use]
pub fn read(account: &Value) -> Option<Token> {
    let metadata = account.get("metadata")?;
    let name = metadata.get("name")?.as_str()?;
    if metadata.pointer(&pointer(&["labels", LABEL])) != Some(&json!(name)) {
        return None;
    }
    let annotation = |key: &str| {
        metadata
            .pointer(&pointer(&["annotations", key]))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    Some(Token {
        name: name.to_owned(),
        created_by: annotation(CREATED_BY_ANNOTATION),
        created_at: metadata
            .get("creationTimestamp")
            .and_then(Value::as_str)
            .map(str::to_owned),
        expires_at: annotation(EXPIRES_AT_ANNOTATION),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_a_service_account_in_the_token_namespace_labelled_with_its_name() {
        let account = service_account("ci", "zeabur:ada");
        assert_eq!(account["metadata"]["name"], "ci");
        assert_eq!(account["metadata"]["namespace"], NAMESPACE);
        assert_eq!(account["metadata"]["labels"][LABEL], "ci");
        assert_eq!(
            account["metadata"]["annotations"][CREATED_BY_ANNOTATION],
            "zeabur:ada"
        );
        assert_eq!(account["automountServiceAccountToken"], false);
        let read_back = read(&account).unwrap();
        assert_eq!(read_back.name, "ci");
        assert_eq!(read_back.created_by.as_deref(), Some("zeabur:ada"));
    }

    #[test]
    fn names_and_expiries_kube_apiserver_would_refuse_are_refused_first() {
        assert_eq!(check_name("CI"), Err(Refusal::Name));
        assert_eq!(check_name(""), Err(Refusal::Name));
        assert_eq!(check_name("ci"), Ok(()));
        assert_eq!(check_seconds(599), Err(Refusal::Seconds));
        assert_eq!(check_seconds(MAX_SECONDS + 1), Err(Refusal::Seconds));
        assert_eq!(check_seconds(MIN_SECONDS), Ok(()));
    }

    #[test]
    fn only_a_service_account_labelled_with_its_own_name_reads_as_a_token() {
        let account = service_account("ci", "zeabur:ada");
        assert_eq!(read(&account).unwrap().expires_at, None);
        let mut patched = account.clone();
        patched["metadata"]["annotations"][EXPIRES_AT_ANNOTATION] = json!("2026-12-01T00:00:00Z");
        assert_eq!(
            read(&patched).unwrap().expires_at.as_deref(),
            Some("2026-12-01T00:00:00Z")
        );
        let mut other = account.clone();
        other["metadata"]["labels"][LABEL] = json!("true");
        assert_eq!(read(&other), None);
        let mut unlabelled = account;
        unlabelled["metadata"]["labels"] = json!({});
        assert_eq!(read(&unlabelled), None);
        let patch = expiry_patch("ci", "2026-12-01T00:00:00Z");
        assert_eq!(patch[0]["op"], "test");
        assert_eq!(patch[0]["value"], "ci");
        assert_eq!(
            patch[1]["path"],
            "/metadata/annotations/zeabur.com~1api-token-expires-at"
        );
    }
}
