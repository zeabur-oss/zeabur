//! The Kubernetes RBAC matcher: whether the rules of a `SelfSubjectRulesReview` allow a request.
//!
//! The rules are matched the way kube-apiserver's RBAC authorizer matches a `PolicyRule` against
//! a request [rbac-source]: the verb, the API group, the resource (with its subresource) and the
//! object's name must each match, and `*` matches any verb, group or resource. Where the
//! documentation and the source differ in wording, the source decides.
//!
//! - A resource rule matches `resource/subresource` exactly, `*` matches every resource and
//!   subresource, and `*/subresource` matches that subresource of every resource. `resource/*` is
//!   not a wildcard in the authorizer [rbac-source], so it matches nothing here.
//! - A rule with `resourceNames` matches only a request that carries one of those names. A
//!   top-level `create` and a `deletecollection` cannot be limited to a name [rbac-names], so
//!   whatever name a caller holds for them is dropped and a named rule never allows them. A
//!   subresource `create` such as `pods/exec` keeps its name.
//! - A named rule does not allow a `list` or `watch` of the whole kind, which has no name. It
//!   allows one that carries the name as a `metadata.name` field selector [rbac-names].
//!
//! # Reviews are hints
//!
//! A `SelfSubjectRulesReview` of a namespace lists the rules of the Roles and ClusterRoles bound
//! in it, and cannot say whether a ClusterRole is bound for the whole cluster or only by a
//! RoleBinding [ssrr]. So its answer is exact only for namespaced requests. For a cluster-scoped
//! request (nodes, a CustomResourceDefinition) ask a `SelfSubjectAccessReview` with no
//! namespace instead. An incomplete review, one that could not be made, and a malformed request
//! never turn into [`Access::Allowed`](crate::Access::Allowed).
//!
//! # References
//!
//! - [rbac-source] The Kubernetes Authors, `plugin/pkg/auth/authorizer/rbac/rbac.go` (`RuleAllows`)
//!   and `pkg/apis/rbac/v1/evaluation_helpers.go` (`VerbMatches`, `APIGroupMatches`, `ResourceMatches`,
//!   `ResourceNameMatches`), Kubernetes v1.34.0. <https://github.com/kubernetes/kubernetes/tree/v1.34.0>
//! - [rbac-names] The Kubernetes Authors, "Using RBAC Authorization," Kubernetes Documentation, the
//!   note on `resourceNames`. <https://kubernetes.io/docs/reference/access-authn-authz/rbac/#referring-to-resources>
//! - [ssrr] The Kubernetes Authors, "Authorization Overview," Kubernetes Documentation. <https://kubernetes.io/docs/reference/access-authn-authz/authorization/#checking-api-access>
//!
//! [rbac-source]: https://github.com/kubernetes/kubernetes/tree/v1.34.0/plugin/pkg/auth/authorizer/rbac
//! [rbac-names]: https://kubernetes.io/docs/reference/access-authn-authz/rbac/#referring-to-resources
//! [ssrr]: https://kubernetes.io/docs/reference/access-authn-authz/authorization/#checking-api-access

use crate::objects::{AccessRule, ResourceRef};
/// The attributes of one resource request, as RBAC sees them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attributes {
    verb: String,
    group: String,
    resource: String,
    subresource: Option<String>,
    name: Option<String>,
    well_formed: bool,
}

impl Attributes {
    /// A request for `verb` on `resource` of `group`. `resource` may carry a subresource
    /// (`pods/log`), as RBAC writes it.
    #[must_use]
    pub fn new(verb: &str, group: &str, resource: &str) -> Self {
        let (resource, subresource) = match resource.split_once('/') {
            Some((resource, subresource)) => (resource, Some(subresource)),
            None => (resource, None),
        };
        let well_formed = !verb.is_empty()
            && !resource.is_empty()
            && subresource.is_none_or(|s| !s.is_empty() && !s.contains('/'));
        Self {
            verb: verb.to_owned(),
            group: group.to_owned(),
            resource: resource.to_owned(),
            subresource: subresource.map(str::to_owned),
            name: None,
            well_formed,
        }
    }

    /// A request for `verb` on `resource`.
    #[must_use]
    pub fn of(verb: &str, resource: &ResourceRef) -> Self {
        Self::new(verb, &resource.group, &resource.resource)
    }

    /// The request is for the object `name`. An empty name is no name.
    #[must_use]
    pub fn named(mut self, name: &str) -> Self {
        self.name = (!name.is_empty()).then(|| name.to_owned());
        self
    }

    /// The verb.
    #[must_use]
    pub fn verb(&self) -> &str {
        &self.verb
    }

    /// The API group.
    #[must_use]
    pub fn group(&self) -> &str {
        &self.group
    }

    /// The resource, with its subresource (`pods/log`).
    #[must_use]
    pub fn resource(&self) -> String {
        self.subresource.as_ref().map_or_else(
            || self.resource.clone(),
            |sub| format!("{}/{sub}", self.resource),
        )
    }

    /// The object's name, when the request carries one.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub(crate) const fn is_well_formed(&self) -> bool {
        self.well_formed
    }

    /// The name the authorizer sees: none for a top-level `create` and for `deletecollection`.
    fn authorized_name(&self) -> Option<&str> {
        let nameless = self.verb == "deletecollection"
            || (self.verb == "create" && self.subresource.is_none());
        if nameless { None } else { self.name.as_deref() }
    }

    pub(crate) fn allowed_by(&self, rule: &AccessRule) -> bool {
        let matches = |values: &[String], wanted: &str| {
            values.iter().any(|value| value == "*" || value == wanted)
        };
        let resource = rule.resources.iter().any(|value| {
            value == "*"
                || self.subresource.as_ref().map_or_else(
                    || *value == self.resource,
                    |sub| {
                        *value == format!("{}/{sub}", self.resource) || *value == format!("*/{sub}")
                    },
                )
        });
        let name = rule.names.is_empty()
            || self
                .authorized_name()
                .is_some_and(|name| rule.names.iter().any(|n| n == name));
        matches(&rule.verbs, &self.verb) && matches(&rule.groups, &self.group) && resource && name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::{Access, AccessRules};

    fn rule(verbs: &[&str], groups: &[&str], resources: &[&str], names: &[&str]) -> AccessRule {
        let own = |values: &[&str]| values.iter().map(|&v| v.to_owned()).collect();
        AccessRule {
            verbs: own(verbs),
            groups: own(groups),
            resources: own(resources),
            names: own(names),
        }
    }

    fn rules(rules: Vec<AccessRule>) -> AccessRules {
        AccessRules {
            rules,
            incomplete: false,
        }
    }

    fn decide(rules: &AccessRules, verb: &str, group: &str, resource: &str) -> Access {
        rules.decide(&Attributes::new(verb, group, resource))
    }

    #[test]
    fn a_wildcard_subresource_matches_that_subresource_of_every_resource_only() {
        let r = rules(vec![rule(&["get"], &[""], &["*/log"], &[])]);
        assert_eq!(decide(&r, "get", "", "pods/log"), Access::Allowed);
        assert_eq!(decide(&r, "get", "", "nodes/log"), Access::Allowed);
        assert_eq!(decide(&r, "get", "", "pods"), Access::Denied);
        assert_eq!(decide(&r, "get", "", "pods/exec"), Access::Denied);
        assert_eq!(decide(&r, "get", "", "log"), Access::Denied);
        assert_eq!(decide(&r, "get", "", "pods/proxy/log"), Access::Denied);
    }

    #[test]
    fn a_resource_does_not_include_its_subresources_and_a_trailing_wildcard_is_no_wildcard() {
        let plain = rules(vec![rule(&["*"], &[""], &["pods"], &[])]);
        assert_eq!(decide(&plain, "get", "", "pods"), Access::Allowed);
        assert_eq!(decide(&plain, "get", "", "pods/log"), Access::Denied);
        let trailing = rules(vec![rule(&["*"], &[""], &["pods/*"], &[])]);
        assert_eq!(decide(&trailing, "get", "", "pods/log"), Access::Denied);
        assert_eq!(decide(&trailing, "get", "", "pods"), Access::Denied);
    }

    #[test]
    fn a_star_resource_includes_subresources_and_star_groups_and_verbs_match_anything() {
        let r = AccessRules::everything();
        assert_eq!(decide(&r, "create", "", "pods/exec"), Access::Allowed);
        assert_eq!(decide(&r, "patch", "k8up.io", "backups"), Access::Allowed);
    }

    #[test]
    fn the_group_must_match_and_the_core_group_is_empty() {
        let r = rules(vec![rule(&["get"], &["apps"], &["deployments"], &[])]);
        assert_eq!(decide(&r, "get", "apps", "deployments"), Access::Allowed);
        assert_eq!(decide(&r, "get", "", "deployments"), Access::Denied);
        assert_eq!(
            decide(&r, "get", "extensions", "deployments"),
            Access::Denied
        );
        let core = rules(vec![rule(&["get"], &[""], &["pods"], &[])]);
        assert_eq!(decide(&core, "get", "apps", "pods"), Access::Denied);
    }

    #[test]
    fn a_named_rule_allows_update_of_that_name_and_never_a_top_level_create() {
        let r = rules(vec![rule(
            &["update", "create"],
            &[""],
            &["configmaps"],
            &["app"],
        )]);
        let configmaps = |verb| Attributes::new(verb, "", "configmaps");
        assert_eq!(
            r.decide(&configmaps("update").named("app")),
            Access::Allowed
        );
        assert_eq!(
            r.decide(&configmaps("update").named("other")),
            Access::Denied
        );
        assert_eq!(r.decide(&configmaps("update")), Access::Denied);
        assert_eq!(r.decide(&configmaps("create").named("app")), Access::Denied);
        assert_eq!(r.decide(&configmaps("create")), Access::Denied);
    }

    #[test]
    fn a_named_rule_allows_a_subresource_create_of_that_name() {
        let r = rules(vec![rule(&["create"], &[""], &["pods/exec"], &["web-0"])]);
        let exec = Attributes::new("create", "", "pods/exec");
        assert_eq!(r.decide(&exec.clone().named("web-0")), Access::Allowed);
        assert_eq!(r.decide(&exec.clone().named("web-1")), Access::Denied);
        assert_eq!(r.decide(&exec), Access::Denied);
    }

    #[test]
    fn a_named_rule_never_allows_deletecollection_or_a_list_without_the_name() {
        let r = rules(vec![rule(&["*"], &[""], &["secrets"], &["one"])]);
        let secrets = |verb| Attributes::new(verb, "", "secrets");
        assert_eq!(
            r.decide(&secrets("deletecollection").named("one")),
            Access::Denied
        );
        assert_eq!(r.decide(&secrets("list")), Access::Denied);
        assert_eq!(r.decide(&secrets("list").named("one")), Access::Allowed);
        assert_eq!(r.decide(&secrets("watch").named("two")), Access::Denied);
    }

    #[test]
    fn an_unnamed_rule_allows_any_name() {
        let r = rules(vec![rule(&["patch"], &[""], &["nodes"], &[])]);
        let patch = Attributes::new("patch", "", "nodes");
        assert_eq!(r.decide(&patch.clone().named("a")), Access::Allowed);
        assert_eq!(r.decide(&patch), Access::Allowed);
    }

    #[test]
    fn an_incomplete_review_never_allows_and_never_denies_for_certain() {
        let mut r = rules(vec![rule(&["get"], &[""], &["pods"], &[])]);
        r.incomplete = true;
        assert_eq!(decide(&r, "get", "", "pods"), Access::Allowed);
        assert_eq!(decide(&r, "delete", "", "pods"), Access::Unknown);
        assert_eq!(
            decide(&AccessRules::default(), "get", "", "pods"),
            Access::Denied
        );
    }

    #[test]
    fn a_malformed_request_is_denied_even_by_a_rule_that_allows_everything() {
        let all = AccessRules::everything();
        for bad in [
            Attributes::new("", "", "pods"),
            Attributes::new("get", "", ""),
            Attributes::new("get", "", "pods/"),
            Attributes::new("get", "", "pods/a/b"),
            Attributes::new("get", "", "/log"),
        ] {
            assert_eq!(all.decide(&bad), Access::Denied, "{bad:?}");
        }
        let incomplete = AccessRules {
            incomplete: true,
            ..AccessRules::default()
        };
        assert_eq!(
            incomplete.decide(&Attributes::new("", "", "pods")),
            Access::Denied
        );
    }

    #[test]
    fn matching_is_exact_and_case_sensitive() {
        let r = rules(vec![rule(&["get"], &[""], &["pods"], &[])]);
        assert_eq!(decide(&r, "GET", "", "pods"), Access::Denied);
        assert_eq!(decide(&r, "get", "", "Pods"), Access::Denied);
        assert_eq!(decide(&r, "get", "", "pod"), Access::Denied);
    }
}
