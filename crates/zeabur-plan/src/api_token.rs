//! API tokens: a ServiceAccount in a project's namespace, labelled [`LABEL`], bound to a project
//! role, and the tokens kube-apiserver issues for it.
//!
//! A token is never stored. Each comes from a `TokenRequest` (`create` of `serviceaccounts/token`)
//! [k8s-token-request] and expires; a `kubernetes.io/service-account-token` Secret would outlive
//! the person who made it, since every Developer of the project can read it. Renewing asks for
//! another token for the same ServiceAccount. kube-apiserver may cut the expiry asked for
//! (`--service-account-max-token-expiration`), so the expiry it answered is recorded in
//! [`EXPIRES_AT_ANNOTATION`], for the latest token only. Deleting the ServiceAccount revokes every
//! token issued for it: kube-apiserver checks that the ServiceAccount still exists, with the same
//! `uid`, on each request [k8s-sa-tokens].
//!
//! The binding is a `RoleBinding` in the namespace ([`binding_name`]), or for [`Scope::Cluster`] a
//! `ClusterRoleBinding` ([`cluster_binding_name`]) that gives the role in every namespace. Either
//! has one subject, the ServiceAccount. It goes before the ServiceAccount when a token is deleted,
//! so a failure half-way leaves fewer rights, never more.
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

use crate::json::{add, pointer, test};

/// Label, `"true"`, on an API token's ServiceAccount and its binding.
pub const LABEL: &str = "zeabur.com/api-token";
/// Annotation on the ServiceAccount: the `ClusterRole` it is bound to.
pub const ROLE_ANNOTATION: &str = "zeabur.com/api-token-role";
/// Annotation on the ServiceAccount: `project` or `cluster`.
pub const SCOPE_ANNOTATION: &str = "zeabur.com/api-token-scope";
/// Annotation on the ServiceAccount: the Kubernetes user name of the person who made it.
pub const CREATED_BY_ANNOTATION: &str = "zeabur.com/api-token-created-by";
/// Annotation on the ServiceAccount: when its latest token expires, RFC 3339.
pub const EXPIRES_AT_ANNOTATION: &str = "zeabur.com/api-token-expires-at";
/// The shortest a token may last: kube-apiserver refuses less than ten minutes.
pub const MIN_SECONDS: i64 = 600;
/// The longest a token may be asked for.
pub const MAX_SECONDS: i64 = 365 * 86_400;

const MANAGED_BY: &str = "app.kubernetes.io/managed-by";
const RBAC_GROUP: &str = "rbac.authorization.k8s.io";
const RBAC_VERSION: &str = "rbac.authorization.k8s.io/v1";

/// Where a token's role applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The project's namespace (`RoleBinding`).
    Project,
    /// Every namespace (`ClusterRoleBinding`).
    Cluster,
}

impl Scope {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Cluster => "cluster",
        }
    }
}

/// A new API token.
#[derive(Debug, Clone, Copy)]
pub struct NewToken<'a> {
    /// The project's namespace, where the ServiceAccount lives.
    pub namespace: &'a str,
    /// The ServiceAccount's name.
    pub name: &'a str,
    /// The `ClusterRole` to bind.
    pub role: &'a str,
    /// Where the role applies.
    pub scope: Scope,
    /// The Kubernetes user name of the person who makes it.
    pub created_by: &'a str,
}

/// An object a step writes, as the API addresses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Addressed {
    /// `apiVersion`.
    pub api_version: String,
    /// The kind.
    pub kind: String,
    /// The resource (plural).
    pub plural: String,
    /// The namespace; empty for a cluster-scoped object.
    pub namespace: String,
    /// The name.
    pub name: String,
}

impl Addressed {
    /// The token's ServiceAccount.
    #[must_use]
    pub fn service_account(namespace: &str, name: &str) -> Self {
        Self {
            api_version: "v1".to_owned(),
            kind: "ServiceAccount".to_owned(),
            plural: "serviceaccounts".to_owned(),
            namespace: namespace.to_owned(),
            name: name.to_owned(),
        }
    }

    /// The token's binding: a `RoleBinding`, or a `ClusterRoleBinding` for [`Scope::Cluster`].
    #[must_use]
    pub fn binding(namespace: &str, name: &str, scope: Scope) -> Self {
        let (kind, plural, in_namespace, name) = match scope {
            Scope::Project => ("RoleBinding", "rolebindings", namespace, binding_name(name)),
            Scope::Cluster => (
                "ClusterRoleBinding",
                "clusterrolebindings",
                "",
                cluster_binding_name(namespace, name),
            ),
        };
        Self {
            api_version: RBAC_VERSION.to_owned(),
            kind: kind.to_owned(),
            plural: plural.to_owned(),
            namespace: in_namespace.to_owned(),
            name,
        }
    }
}

/// One write of an API token's plan, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// A new object.
    Create {
        /// The object.
        target: Addressed,
        /// Its body.
        body: Value,
    },
    /// A `TokenRequest` for the ServiceAccount.
    Token {
        /// The ServiceAccount.
        target: Addressed,
        /// The expiry asked for.
        seconds: i64,
    },
    /// The expiry kube-apiserver answered, written to [`EXPIRES_AT_ANNOTATION`].
    RecordExpiry {
        /// The ServiceAccount.
        target: Addressed,
    },
    /// A deletion that holds only while the object has this `uid`.
    Delete {
        /// The object.
        target: Addressed,
        /// Its `metadata.uid`.
        uid: Option<String>,
        /// The object as read.
        before: Value,
    },
}

/// The `RoleBinding` of the token `name`.
#[must_use]
pub fn binding_name(name: &str) -> String {
    format!("zeabur-api-token-{name}")
}

/// The `ClusterRoleBinding` of the token `name` in `namespace`. A colon cannot be in either name,
/// so no two tokens share one.
#[must_use]
pub fn cluster_binding_name(namespace: &str, name: &str) -> String {
    format!("zeabur-api-token:{namespace}:{name}")
}

fn labels() -> Value {
    json!({MANAGED_BY: "zeabur", LABEL: "true"})
}

/// The token's ServiceAccount.
#[must_use]
pub fn service_account(token: &NewToken<'_>) -> Value {
    json!({
        "apiVersion": "v1",
        "kind": "ServiceAccount",
        "metadata": {
            "name": token.name,
            "namespace": token.namespace,
            "labels": labels(),
            "annotations": {
                ROLE_ANNOTATION: token.role,
                SCOPE_ANNOTATION: token.scope.as_str(),
                CREATED_BY_ANNOTATION: token.created_by,
            },
        },
        "automountServiceAccountToken": false,
    })
}

/// The binding that gives the ServiceAccount its role.
#[must_use]
pub fn binding(token: &NewToken<'_>) -> Value {
    let target = Addressed::binding(token.namespace, token.name, token.scope);
    let mut metadata = json!({"name": target.name, "labels": labels()});
    if token.scope == Scope::Project
        && let Some(map) = metadata.as_object_mut()
    {
        map.insert("namespace".to_owned(), json!(token.namespace));
    }
    json!({
        "apiVersion": RBAC_VERSION,
        "kind": target.kind,
        "metadata": metadata,
        "roleRef": {"apiGroup": RBAC_GROUP, "kind": "ClusterRole", "name": token.role},
        "subjects": [{"kind": "ServiceAccount", "name": token.name, "namespace": token.namespace}],
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

/// The JSON Patch that records `expires_at` on a token's ServiceAccount; its `test` refuses a
/// ServiceAccount that is no longer an API token.
#[must_use]
pub fn expiry_patch(expires_at: &str) -> Vec<Value> {
    vec![
        test(&pointer(&["metadata", "labels", LABEL]), json!("true")),
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

/// The writes that make a token: the ServiceAccount, its binding, the first token and its expiry.
///
/// # Errors
///
/// [`Refusal`] for a name that is not a DNS label or an expiry out of range.
pub fn create(token: &NewToken<'_>, seconds: i64) -> Result<Vec<Step>, Refusal> {
    if !crate::checks::is_dns_label(token.name) {
        return Err(Refusal::Name);
    }
    check_seconds(seconds)?;
    let account = Addressed::service_account(token.namespace, token.name);
    Ok(vec![
        Step::Create {
            target: account.clone(),
            body: service_account(token),
        },
        Step::Create {
            target: Addressed::binding(token.namespace, token.name, token.scope),
            body: binding(token),
        },
        Step::Token {
            target: account.clone(),
            seconds,
        },
        Step::RecordExpiry { target: account },
    ])
}

/// The writes that issue another token for an existing one.
///
/// # Errors
///
/// [`Refusal::Seconds`] for an expiry out of range.
pub fn renew(namespace: &str, name: &str, seconds: i64) -> Result<Vec<Step>, Refusal> {
    check_seconds(seconds)?;
    let account = Addressed::service_account(namespace, name);
    Ok(vec![
        Step::Token {
            target: account.clone(),
            seconds,
        },
        Step::RecordExpiry { target: account },
    ])
}

/// The writes that delete a token: its binding when there is one, then the ServiceAccount, which
/// revokes every token issued for it.
#[must_use]
pub fn delete(account: &Value, binding: Option<&Value>) -> Vec<Step> {
    let mut steps = Vec::new();
    let mut deletion = |object: &Value| {
        let text = |path: &str| {
            object
                .pointer(path)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let api_version = text("/apiVersion");
        let kind = text("/kind");
        let plural = format!("{}s", kind.to_lowercase());
        steps.push(Step::Delete {
            target: Addressed {
                api_version,
                kind,
                plural,
                namespace: text("/metadata/namespace"),
                name: text("/metadata/name"),
            },
            uid: object
                .pointer("/metadata/uid")
                .and_then(Value::as_str)
                .map(str::to_owned),
            before: object.clone(),
        });
    };
    if let Some(binding) = binding {
        deletion(binding);
    }
    deletion(account);
    steps
}

fn check_seconds(seconds: i64) -> Result<(), Refusal> {
    if (MIN_SECONDS..=MAX_SECONDS).contains(&seconds) {
        Ok(())
    } else {
        Err(Refusal::Seconds)
    }
}

/// An API token, as its ServiceAccount records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The ServiceAccount's name.
    pub name: String,
    /// The `ClusterRole` it is bound to, as recorded when it was made.
    pub role: Option<String>,
    /// Where the role applies.
    pub scope: Scope,
    /// Who made it.
    pub created_by: Option<String>,
    /// `metadata.creationTimestamp`.
    pub created_at: Option<String>,
    /// When the latest token expires.
    pub expires_at: Option<String>,
}

/// The token a ServiceAccount is, when it carries [`LABEL`].
#[must_use]
pub fn read(account: &Value) -> Option<Token> {
    let metadata = account.get("metadata")?;
    if metadata.pointer(&pointer(&["labels", LABEL])) != Some(&json!("true")) {
        return None;
    }
    let annotation = |key: &str| {
        metadata
            .pointer(&pointer(&["annotations", key]))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let scope = if annotation(SCOPE_ANNOTATION).as_deref() == Some("cluster") {
        Scope::Cluster
    } else {
        Scope::Project
    };
    Some(Token {
        name: metadata.get("name")?.as_str()?.to_owned(),
        role: annotation(ROLE_ANNOTATION),
        scope,
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

    fn token(scope: Scope) -> NewToken<'static> {
        NewToken {
            namespace: "shop",
            name: "ci",
            role: "zeabur-project-developer",
            scope,
            created_by: "zeabur:ada",
        }
    }

    fn created(steps: &[Step]) -> Vec<&Value> {
        let mut bodies = Vec::new();
        for step in steps {
            if let Step::Create { body, .. } = step {
                bodies.push(body);
            }
        }
        bodies
    }

    #[test]
    fn a_token_is_a_labelled_service_account_bound_to_its_role_in_the_project() {
        let steps = create(&token(Scope::Project), 86_400).unwrap();
        let bodies = created(&steps);
        let account = bodies[0];
        assert_eq!(account["kind"], "ServiceAccount");
        assert_eq!(account["metadata"]["namespace"], "shop");
        assert_eq!(account["metadata"]["labels"][LABEL], "true");
        assert_eq!(
            account["metadata"]["annotations"][CREATED_BY_ANNOTATION],
            "zeabur:ada"
        );
        assert_eq!(account["automountServiceAccountToken"], false);
        let binding = bodies[1];
        assert_eq!(binding["kind"], "RoleBinding");
        assert_eq!(binding["metadata"]["name"], "zeabur-api-token-ci");
        assert_eq!(binding["metadata"]["namespace"], "shop");
        assert_eq!(binding["metadata"]["labels"][LABEL], "true");
        assert_eq!(binding["roleRef"]["name"], "zeabur-project-developer");
        assert_eq!(
            binding["subjects"],
            json!([{"kind": "ServiceAccount", "name": "ci", "namespace": "shop"}])
        );
        assert!(matches!(
            &steps[2],
            Step::Token { target, seconds: 86_400 } if target.name == "ci" && target.namespace == "shop"
        ));
        assert!(matches!(&steps[3], Step::RecordExpiry { .. }));
        assert_eq!(read(account).unwrap().scope, Scope::Project);
    }

    #[test]
    fn a_cluster_wide_token_gets_a_cluster_role_binding_no_other_token_can_share() {
        let steps = create(&token(Scope::Cluster), 3_600).unwrap();
        let binding = created(&steps)[1];
        assert_eq!(binding["kind"], "ClusterRoleBinding");
        assert_eq!(binding["metadata"].get("namespace"), None);
        assert_eq!(binding["subjects"][0]["namespace"], "shop");
        assert_ne!(
            cluster_binding_name("a-b", "c"),
            cluster_binding_name("a", "b-c")
        );
        assert_eq!(
            read(created(&steps)[0]).unwrap().scope,
            Scope::Cluster,
            "the scope is recorded on the ServiceAccount"
        );
    }

    #[test]
    fn names_and_expiries_kube_apiserver_would_refuse_are_refused_first() {
        let named = |name| NewToken {
            name,
            ..token(Scope::Project)
        };
        assert_eq!(create(&named("CI"), 3_600), Err(Refusal::Name));
        assert_eq!(create(&named(""), 3_600), Err(Refusal::Name));
        assert_eq!(create(&named("ci"), 599), Err(Refusal::Seconds));
        assert_eq!(renew("shop", "ci", MAX_SECONDS + 1), Err(Refusal::Seconds));
        assert!(renew("shop", "ci", MIN_SECONDS).is_ok());
    }

    #[test]
    fn removal_deletes_the_binding_before_the_service_account_and_pins_both() {
        let account = json!({
            "apiVersion": "v1", "kind": "ServiceAccount",
            "metadata": {"name": "ci", "namespace": "shop", "uid": "sa-uid"},
        });
        let binding = json!({
            "apiVersion": RBAC_VERSION, "kind": "ClusterRoleBinding",
            "metadata": {"name": "zeabur-api-token:shop:ci", "uid": "crb-uid"},
        });
        let steps = delete(&account, Some(&binding));
        let mut deleted = Vec::new();
        for step in &steps {
            if let Step::Delete { target, uid, .. } = step {
                deleted.push((
                    target.plural.as_str(),
                    target.namespace.as_str(),
                    uid.as_deref(),
                ));
            }
        }
        assert_eq!(
            deleted,
            [
                ("clusterrolebindings", "", Some("crb-uid")),
                ("serviceaccounts", "shop", Some("sa-uid")),
            ]
        );
        let alone = delete(&account, None);
        assert!(
            matches!(&alone[..], [Step::Delete { target, .. }] if target.kind == "ServiceAccount")
        );
    }

    #[test]
    fn only_a_labelled_service_account_reads_as_a_token_with_its_latest_expiry() {
        let mut account = service_account(&token(Scope::Project));
        assert_eq!(read(&account).unwrap().expires_at, None);
        let mut patched = account.clone();
        patched["metadata"]["annotations"][EXPIRES_AT_ANNOTATION] = json!("2026-12-01T00:00:00Z");
        let read_back = read(&patched).unwrap();
        assert_eq!(
            read_back.expires_at.as_deref(),
            Some("2026-12-01T00:00:00Z")
        );
        assert_eq!(read_back.role.as_deref(), Some("zeabur-project-developer"));
        account["metadata"]["labels"] = json!({});
        assert_eq!(read(&account), None);
        let patch = expiry_patch("2026-12-01T00:00:00Z");
        assert_eq!(patch[0]["op"], "test");
        assert_eq!(
            patch[1]["path"],
            "/metadata/annotations/zeabur.com~1api-token-expires-at"
        );
    }
}
