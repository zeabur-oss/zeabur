//! What the person of an in-memory cluster may do, as RBAC rules.
//!
//! Reviews, writes, dry runs and commands in a pod are all decided here by the matcher
//! ([`Attributes`]), so a test changes what a person may do by changing the rules, as a cluster
//! administrator does, and every path agrees.
//!
//! RBAC has no deny. A test that takes one right away from a person who otherwise may do
//! everything records it as a [`Revocation`]: the role was written without it. A revocation is
//! not part of the rules a `SelfSubjectRulesReview` lists; every decision applies it.

use std::collections::BTreeMap;

use crate::objects::{Access, AccessRule, AccessRules};
use crate::rbac::Attributes;

/// A right the person's role was written without.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Revocation {
    /// The verbs; empty: every verb.
    pub verbs: Vec<String>,
    /// The API group; `None`: any.
    pub group: Option<String>,
    /// The resource with its subresource (`pods/exec`); `None`: any.
    pub resource: Option<String>,
    /// The namespace; `None`: every namespace and the cluster.
    pub namespace: Option<String>,
    /// The object's name; `None`: any.
    pub name: Option<String>,
    /// The subresources of `resource` too.
    pub subresources: bool,
}

impl Revocation {
    fn covers(&self, request: &Attributes, namespace: Option<&str>) -> bool {
        (self.verbs.is_empty() || self.verbs.iter().any(|v| v == request.verb()))
            && self.group.as_deref().is_none_or(|g| g == request.group())
            && self.resource.as_deref().is_none_or(|r| {
                let requested = request.resource();
                requested == r
                    || (self.subresources
                        && requested.split_once('/').is_some_and(|(base, _)| base == r))
            })
            && self
                .namespace
                .as_deref()
                .is_none_or(|n| Some(n) == namespace)
            && self
                .name
                .as_deref()
                .is_none_or(|n| Some(n) == request.name())
    }
}

/// The rules of the person of an in-memory cluster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grants {
    /// What cluster-wide requests (those without a namespace) are allowed: a
    /// `ClusterRoleBinding`.
    pub cluster: AccessRules,
    /// What requests in a namespace are allowed, unless `in_namespace` names it.
    pub namespaced: AccessRules,
    /// What requests in a namespace are allowed there instead of `namespaced`: a `RoleBinding`.
    pub in_namespace: BTreeMap<String, AccessRules>,
    /// Namespaces whose rules review fails with `500`.
    pub failing_reviews: Vec<String>,
    /// Rights taken away from the rules above.
    pub revoked: Vec<Revocation>,
}

impl Default for Grants {
    fn default() -> Self {
        Self {
            cluster: AccessRules::everything(),
            namespaced: AccessRules::everything(),
            in_namespace: BTreeMap::new(),
            failing_reviews: Vec::new(),
            revoked: Vec::new(),
        }
    }
}

impl Grants {
    /// A person who may do nothing.
    #[must_use]
    pub fn nothing() -> Self {
        Self {
            cluster: AccessRules::default(),
            namespaced: AccessRules::default(),
            ..Self::default()
        }
    }

    /// Everything, except that the person has no rights in `namespace`.
    #[must_use]
    pub fn refusing_namespace(namespace: &str) -> Self {
        let mut grants = Self::default();
        grants
            .in_namespace
            .insert(namespace.to_owned(), AccessRules::default());
        grants
    }

    /// Everything, except that the person has no rights outside a namespace.
    #[must_use]
    pub fn without_cluster_wide() -> Self {
        Self {
            cluster: AccessRules::default(),
            ..Self::default()
        }
    }

    /// Everything, except `verb` on `resource`.
    #[must_use]
    pub fn refusing_right(verb: &str, resource: &str) -> Self {
        let mut grants = Self::default();
        grants.revoked.push(Revocation {
            verbs: vec![verb.to_owned()],
            resource: Some(resource.to_owned()),
            ..Revocation::default()
        });
        grants
    }

    /// The rules that apply to `namespace` (cluster-wide when `None`).
    #[must_use]
    pub fn rules_in(&self, namespace: Option<&str>) -> &AccessRules {
        namespace.map_or(&self.cluster, |namespace| {
            self.in_namespace.get(namespace).unwrap_or(&self.namespaced)
        })
    }

    /// Whether the person has any right in `namespace` (anywhere, when `None`).
    #[must_use]
    pub fn reaches(&self, namespace: Option<&str>) -> bool {
        match namespace {
            Some(_) => !self.rules_in(namespace).rules.is_empty(),
            None => {
                !self.cluster.rules.is_empty()
                    || !self.namespaced.rules.is_empty()
                    || self.in_namespace.values().any(|r| !r.rules.is_empty())
            }
        }
    }

    /// The answer to `request` in `namespace` (cluster-wide when `None`).
    #[must_use]
    pub fn decide(&self, request: &Attributes, namespace: Option<&str>) -> Access {
        if self.revokes(request, namespace) {
            return Access::Denied;
        }
        self.rules_in(namespace).decide(request)
    }

    /// Whether a revocation takes `request` away.
    #[must_use]
    pub fn revokes(&self, request: &Attributes, namespace: Option<&str>) -> bool {
        self.revoked.iter().any(|r| r.covers(request, namespace))
    }

    /// Whether `request` is allowed.
    #[must_use]
    pub fn allows(&self, request: &Attributes, namespace: Option<&str>) -> bool {
        self.decide(request, namespace) == Access::Allowed
    }

    /// Rules that let the person read everything.
    #[must_use]
    pub fn read_only() -> AccessRules {
        AccessRules {
            rules: vec![AccessRule {
                verbs: ["get", "list", "watch"].map(str::to_owned).to_vec(),
                groups: vec!["*".to_owned()],
                resources: vec!["*".to_owned()],
                names: Vec::new(),
            }],
            incomplete: false,
        }
    }
}
