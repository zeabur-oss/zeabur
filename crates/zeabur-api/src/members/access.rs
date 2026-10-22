//! Members' rights in projects: RBAC bindings to the chart's member roles.
//!
//! An admin is a member whose `User` has the group `admins` (see the parent module). Everyone
//! else gets, per project, a `RoleBinding` named `zeabur-member-<username>` in that project's
//! namespace to the `ClusterRole` `zeabur-project-developer` or `zeabur-project-viewer`, for the
//! user `zeabur:<username>`, and one `ClusterRoleBinding` of the same name to `zeabur-member`
//! (node names and status, every project's name). Zeabur Identity gives no group to every user
//! (a token's groups are the `User`'s `spec.groups` only), and `system:authenticated` would also
//! grant the baseline to every service account, so the baseline is bound per user.
//!
//! Each binding carries the label [`MEMBER_LABEL`] with the user name, which is how Zeabur finds
//! them again: a role change replaces the binding (its `roleRef` cannot change [k8s-roleref]),
//! and removing a member deletes them. Writes go as the person, so creating a binding needs the
//! rights it grants or `bind` on the role [k8s-escalation]: in practice an admin with admin
//! rights in use.
//!
//! [k8s-roleref]: https://kubernetes.io/docs/reference/access-authn-authz/rbac/#clusterrolebinding-example
//! [k8s-escalation]: https://kubernetes.io/docs/reference/access-authn-authz/rbac/#restrictions-on-role-binding-creation-or-update

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use zeabur_core::problem::{Denied, Kind};
use zeabur_kube::{
    Cluster, ClusterError, DynamicObject, ObjectTarget, Preconditions, ResourceRef, WriteBody,
    WriteRequest,
};

use super::USER_PREFIX;
use crate::error::{ApiError, list_read, read_problem};

/// Label naming the member a binding is for.
pub const MEMBER_LABEL: &str = "zeabur.com/member";
/// The `ClusterRole` every member gets.
pub const BASELINE_ROLE: &str = "zeabur-member";
const RBAC_GROUP: &str = "rbac.authorization.k8s.io";
const RBAC_VERSION: &str = "rbac.authorization.k8s.io/v1";
const MANAGED_BY: &str = "app.kubernetes.io/managed-by";

/// What a member may do in one project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ProjectRole {
    /// Deploys, changes variables, restarts, reads logs and Secrets, opens terminals.
    Developer,
    /// Reads status, logs and events.
    Viewer,
}

impl ProjectRole {
    /// The chart's `ClusterRole`.
    #[must_use]
    pub const fn cluster_role(self) -> &'static str {
        match self {
            Self::Developer => "zeabur-project-developer",
            Self::Viewer => "zeabur-project-viewer",
        }
    }

    fn of_cluster_role(name: &str) -> Option<Self> {
        [Self::Developer, Self::Viewer]
            .into_iter()
            .find(|role| role.cluster_role() == name)
    }
}

/// A member's role in a project.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectGrant {
    /// The project (its namespace).
    pub project: String,
    /// The role there.
    pub role: ProjectRole,
}

/// The name of every binding Zeabur makes for `username`.
#[must_use]
pub fn binding_name(username: &str) -> String {
    format!("zeabur-member-{username}")
}

fn labels(username: &str) -> Value {
    json!({MANAGED_BY: "zeabur", MEMBER_LABEL: username})
}

fn subjects(username: &str) -> Value {
    json!([{"apiGroup": RBAC_GROUP, "kind": "User", "name": format!("{USER_PREFIX}{username}")}])
}

/// The `RoleBinding` that gives `username` `role` in `project`.
#[must_use]
pub fn role_binding(project: &str, username: &str, role: ProjectRole) -> Value {
    json!({
        "apiVersion": RBAC_VERSION,
        "kind": "RoleBinding",
        "metadata": {"name": binding_name(username), "namespace": project, "labels": labels(username)},
        "roleRef": {"apiGroup": RBAC_GROUP, "kind": "ClusterRole", "name": role.cluster_role()},
        "subjects": subjects(username),
    })
}

/// The `ClusterRoleBinding` that gives `username` the baseline.
#[must_use]
pub fn baseline_binding(username: &str) -> Value {
    json!({
        "apiVersion": RBAC_VERSION,
        "kind": "ClusterRoleBinding",
        "metadata": {"name": binding_name(username), "labels": labels(username)},
        "roleRef": {"apiGroup": RBAC_GROUP, "kind": "ClusterRole", "name": BASELINE_ROLE},
        "subjects": subjects(username),
    })
}

/// A project binding Zeabur made, as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Binding {
    pub(crate) username: String,
    pub(crate) project: String,
    /// `None`: it names another role (edited by hand), so it is replaced like a changed one.
    pub(crate) role: Option<ProjectRole>,
    pub(crate) uid: Option<String>,
}

fn rolebindings() -> ResourceRef {
    ResourceRef::new(RBAC_GROUP, "v1", "rolebindings", "RoleBinding")
}

/// The bindings among `objects` that Zeabur made for a member: labelled and named as it names
/// them, so a binding someone else labels is not taken for one.
pub(crate) fn bindings_of(objects: Vec<DynamicObject>) -> Vec<Binding> {
    let mut bindings: Vec<Binding> = objects
        .into_iter()
        .filter_map(|object| {
            let username = object.metadata.labels.as_ref()?.get(MEMBER_LABEL)?.clone();
            if object.metadata.name.as_deref() != Some(binding_name(&username).as_str()) {
                return None;
            }
            let role = object
                .data
                .pointer("/roleRef")
                .filter(|r| r.get("kind").and_then(Value::as_str) == Some("ClusterRole"))
                .and_then(|r| r.get("name"))
                .and_then(Value::as_str)
                .and_then(ProjectRole::of_cluster_role);
            Some(Binding {
                project: object.metadata.namespace?,
                uid: object.metadata.uid,
                username,
                role,
            })
        })
        .collect();
    bindings.sort_by(|a, b| (&a.username, &a.project).cmp(&(&b.username, &b.project)));
    bindings
}

/// Every member's project bindings; `None` when the RBAC API is not served.
pub(crate) async fn read(cluster: &dyn Cluster) -> Result<Option<Vec<Binding>>, ClusterError> {
    Ok(cluster
        .list_objects(&rolebindings(), None)
        .await?
        .map(bindings_of))
}

/// The refusal key of the read, as `unreadable` names it.
pub(crate) fn key() -> String {
    format!("rolebindings.{RBAC_GROUP}")
}

/// Each member's projects, from `bindings`; a binding to another role is left out.
pub(crate) fn grants_by_member(bindings: &[Binding]) -> BTreeMap<String, Vec<ProjectGrant>> {
    let mut grants: BTreeMap<String, Vec<ProjectGrant>> = BTreeMap::new();
    for binding in bindings {
        if let Some(role) = binding.role {
            grants
                .entry(binding.username.clone())
                .or_default()
                .push(ProjectGrant {
                    project: binding.project.clone(),
                    role,
                });
        }
    }
    grants
}

/// One write towards a member's wanted projects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Step {
    Delete(Binding),
    Create(ProjectGrant),
}

/// The writes that take `username`'s `current` bindings to `wanted`: deletions first, so a
/// failure half-way leaves fewer rights, never more.
pub(crate) fn steps(username: &str, current: &[Binding], wanted: &[ProjectGrant]) -> Vec<Step> {
    let mine: Vec<&Binding> = current.iter().filter(|b| b.username == username).collect();
    let mut deletes = Vec::new();
    let mut creates = Vec::new();
    for binding in &mine {
        let keep = wanted
            .iter()
            .any(|w| w.project == binding.project && Some(w.role) == binding.role);
        if !keep {
            deletes.push(Step::Delete((*binding).clone()));
        }
    }
    for grant in wanted {
        let held = mine
            .iter()
            .any(|b| b.project == grant.project && b.role == Some(grant.role));
        if !held {
            creates.push(Step::Create(grant.clone()));
        }
    }
    deletes.extend(creates);
    deletes
}

/// The wanted projects, checked: each a namespace name, each once.
pub(crate) fn check(projects: &[ProjectGrant]) -> Result<Vec<ProjectGrant>, String> {
    let mut seen = BTreeMap::new();
    for grant in projects {
        if !zeabur_plan::checks::is_dns_label(&grant.project) {
            return Err(format!("{:?} is not a project's name.", grant.project));
        }
        if seen.insert(grant.project.clone(), grant.role).is_some() {
            return Err(format!("{} is named twice.", grant.project));
        }
    }
    Ok(seen
        .into_iter()
        .map(|(project, role)| ProjectGrant { project, role })
        .collect())
}

fn target(kind: &str, namespace: &str, name: &str) -> ObjectTarget {
    ObjectTarget {
        api_version: RBAC_VERSION.to_owned(),
        kind: kind.to_owned(),
        plural: format!("{}s", kind.to_lowercase()),
        namespace: namespace.to_owned(),
        name: name.to_owned(),
    }
}

async fn send(
    cluster: &dyn Cluster,
    target: ObjectTarget,
    body: WriteBody,
) -> Result<(), ApiError> {
    let verb = match &body {
        WriteBody::Create(_) => "create",
        WriteBody::Delete(_) => "delete",
        WriteBody::JsonPatch(_) | WriteBody::EphemeralContainers(_) => "patch",
    };
    let what = if target.namespace.is_empty() {
        format!("{} {}", target.kind, target.name)
    } else {
        format!("{} {} in {}", target.kind, target.name, target.namespace)
    };
    let denied = Denied {
        verb: verb.to_owned(),
        group: RBAC_GROUP.to_owned(),
        resource: target.plural.clone(),
        name: (verb != "create").then(|| target.name.clone()),
        namespace: (!target.namespace.is_empty()).then(|| target.namespace.clone()),
    };
    let write = WriteRequest {
        target,
        body,
        dry_run: false,
    };
    match cluster.write(&write).await {
        Ok(_) => Ok(()),
        Err(error) => Err(read_problem(error, &what, denied)),
    }
}

/// Carries out `steps` for `username`; a binding already gone is fine.
pub(crate) async fn apply(
    cluster: &dyn Cluster,
    username: &str,
    steps: Vec<Step>,
) -> Result<(), ApiError> {
    let name = binding_name(username);
    for step in steps {
        match step {
            Step::Delete(binding) => {
                let body = WriteBody::Delete(Preconditions {
                    uid: binding.uid,
                    resource_version: None,
                });
                match send(
                    cluster,
                    target("RoleBinding", &binding.project, &name),
                    body,
                )
                .await
                {
                    Err(error) if error.0.kind == Kind::NotFound => {}
                    other => other?,
                }
            }
            Step::Create(grant) => {
                let body = WriteBody::Create(role_binding(&grant.project, username, grant.role));
                send(cluster, target("RoleBinding", &grant.project, &name), body).await?;
            }
        }
    }
    Ok(())
}

/// Gives `username` the baseline; one already there is fine.
pub(crate) async fn ensure_baseline(cluster: &dyn Cluster, username: &str) -> Result<(), ApiError> {
    let body = WriteBody::Create(baseline_binding(username));
    match send(
        cluster,
        target("ClusterRoleBinding", "", &binding_name(username)),
        body,
    )
    .await
    {
        Err(error) if error.0.kind == Kind::Conflict => Ok(()),
        other => other,
    }
}

/// Reads `username`'s project bindings, or the problem that names the read refused.
pub(crate) async fn bindings_for(
    cluster: &dyn Cluster,
    username: &str,
) -> Result<Vec<Binding>, ApiError> {
    let bindings = read(cluster).await.map_err(|error| {
        read_problem(
            error,
            &format!("the RoleBindings of {username}"),
            list_read(RBAC_GROUP, "rolebindings", None),
        )
    })?;
    Ok(bindings
        .unwrap_or_default()
        .into_iter()
        .filter(|b| b.username == username)
        .collect())
}

/// Gives `username` exactly `wanted`: the baseline, and one binding per project.
pub(crate) async fn set(
    cluster: &dyn Cluster,
    username: &str,
    wanted: &[ProjectGrant],
) -> Result<(), ApiError> {
    let current = bindings_for(cluster, username).await?;
    ensure_baseline(cluster, username).await?;
    apply(cluster, username, steps(username, &current, wanted)).await
}

/// Takes every binding Zeabur made for `username` away: the projects', then the baseline.
pub(crate) async fn remove_all(cluster: &dyn Cluster, username: &str) -> Result<(), ApiError> {
    let current = bindings_for(cluster, username).await?;
    apply(cluster, username, steps(username, &current, &[])).await?;
    let body = WriteBody::Delete(Preconditions::default());
    match send(
        cluster,
        target("ClusterRoleBinding", "", &binding_name(username)),
        body,
    )
    .await
    {
        Err(error) if error.0.kind == Kind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use zeabur_core::names::{
        ANNOTATION_DESCRIPTION, ANNOTATION_DISPLAY_NAME, ANNOTATION_ICON, LABEL_PROJECT,
        MANAGE_VERB,
    };
    use zeabur_kube::{AccessRule, AccessRules};

    use super::*;

    fn binding(username: &str, project: &str, role: Option<ProjectRole>) -> Binding {
        Binding {
            username: username.to_owned(),
            project: project.to_owned(),
            role,
            uid: Some(format!("uid-{project}")),
        }
    }

    fn grant(project: &str, role: ProjectRole) -> ProjectGrant {
        ProjectGrant {
            project: project.to_owned(),
            role,
        }
    }

    #[test]
    fn a_binding_names_the_user_as_kubernetes_sees_them_and_carries_the_member_label() {
        let object = role_binding("shop", "ada", ProjectRole::Viewer);
        assert_eq!(object["metadata"]["name"], "zeabur-member-ada");
        assert_eq!(object["metadata"]["namespace"], "shop");
        assert_eq!(object["metadata"]["labels"][MEMBER_LABEL], "ada");
        assert_eq!(object["roleRef"]["name"], "zeabur-project-viewer");
        assert_eq!(object["subjects"][0]["kind"], "User");
        assert_eq!(object["subjects"][0]["name"], "zeabur:ada");
        let baseline = baseline_binding("ada");
        assert_eq!(baseline["kind"], "ClusterRoleBinding");
        assert_eq!(baseline["metadata"].get("namespace"), None);
        assert_eq!(baseline["roleRef"]["name"], BASELINE_ROLE);
    }

    #[test]
    fn only_bindings_named_and_labelled_as_zeabur_makes_them_are_read() {
        let object = |name: &str, label: &str, role: &str| -> DynamicObject {
            serde_json::from_value(json!({
                "apiVersion": RBAC_VERSION, "kind": "RoleBinding",
                "metadata": {"name": name, "namespace": "shop", "labels": {MEMBER_LABEL: label}},
                "roleRef": {"apiGroup": RBAC_GROUP, "kind": "ClusterRole", "name": role},
                "subjects": [],
            }))
            .unwrap()
        };
        let read = bindings_of(vec![
            object("zeabur-member-ada", "ada", "zeabur-project-developer"),
            object("someone-elses", "ada", "zeabur-project-developer"),
            object("zeabur-member-bob", "bob", "admin"),
        ]);
        assert_eq!(
            read.iter()
                .map(|b| (b.username.as_str(), b.role))
                .collect::<Vec<_>>(),
            [("ada", Some(ProjectRole::Developer)), ("bob", None)]
        );
        let grants = grants_by_member(&read);
        assert_eq!(grants["ada"], [grant("shop", ProjectRole::Developer)]);
        assert!(!grants.contains_key("bob"));
    }

    #[test]
    fn a_changed_role_is_deleted_and_created_again_and_deletions_come_first() {
        let current = [
            binding("ada", "shop", Some(ProjectRole::Viewer)),
            binding("ada", "blog", Some(ProjectRole::Developer)),
            binding("ada", "old", Some(ProjectRole::Viewer)),
            binding("bob", "shop", Some(ProjectRole::Viewer)),
        ];
        let wanted = [
            grant("blog", ProjectRole::Developer),
            grant("new", ProjectRole::Viewer),
            grant("shop", ProjectRole::Developer),
        ];
        assert_eq!(
            steps("ada", &current, &wanted),
            [
                Step::Delete(binding("ada", "shop", Some(ProjectRole::Viewer))),
                Step::Delete(binding("ada", "old", Some(ProjectRole::Viewer))),
                Step::Create(grant("new", ProjectRole::Viewer)),
                Step::Create(grant("shop", ProjectRole::Developer)),
            ]
        );
        assert_eq!(steps("ada", &current, &[]).len(), 3);
        assert_eq!(steps("cy", &current, &[]), []);
    }

    #[test]
    fn projects_are_names_each_given_once() {
        assert!(check(&[grant("Shop", ProjectRole::Viewer)]).is_err());
        assert!(
            check(&[
                grant("shop", ProjectRole::Viewer),
                grant("shop", ProjectRole::Developer)
            ])
            .is_err()
        );
        assert_eq!(
            check(&[
                grant("shop", ProjectRole::Viewer),
                grant("blog", ProjectRole::Developer)
            ])
            .unwrap()[0]
                .project,
            "blog"
        );
    }

    #[derive(Clone, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Rule {
        api_groups: Vec<String>,
        resources: Vec<String>,
        verbs: Vec<String>,
    }

    /// The chart's rule sets, by name.
    fn rule_sets() -> BTreeMap<String, Vec<Rule>> {
        let file = include_str!("../../../../deploy/chart/zeabur/files/member-roles.yaml");
        serde_saphyr::from_str(file).unwrap()
    }

    /// The rule sets each aggregated role gathers.
    fn gathered() -> BTreeMap<String, Vec<String>> {
        let file = include_str!("../../../../deploy/chart/zeabur/files/member-role-sets.yaml");
        serde_saphyr::from_str(file).unwrap()
    }

    /// The roles Zeabur binds, as `SelfSubjectRulesReview` would report them once Kubernetes has
    /// aggregated them.
    fn roles() -> BTreeMap<String, AccessRules> {
        let mut sets = rule_sets();
        let gathered = gathered();
        for name in gathered.values().flatten() {
            assert!(sets.contains_key(name), "{name} is not a rule set");
        }
        let mut roles: BTreeMap<String, Vec<Rule>> = gathered
            .iter()
            .map(|(role, names)| {
                let rules = names.iter().flat_map(|n| sets[n].clone()).collect();
                (role.clone(), rules)
            })
            .collect();
        sets.retain(|name, _| !gathered.values().flatten().any(|n| n == name));
        roles.extend(sets);
        roles
            .into_iter()
            .map(|(name, listed)| {
                let rules = listed
                    .into_iter()
                    .map(|r| AccessRule {
                        verbs: r.verbs,
                        groups: r.api_groups,
                        resources: r.resources,
                        names: Vec::new(),
                    })
                    .collect();
                (
                    name,
                    AccessRules {
                        rules,
                        incomplete: false,
                    },
                )
            })
            .collect()
    }

    fn allows(rules: &AccessRules, verb: &str, resource: &str) -> bool {
        let (resource, group) = resource.split_once('.').unwrap_or((resource, ""));
        rules.allows(verb, &ResourceRef::new(group, "v1", resource, ""), "")
            == zeabur_kube::Access::Allowed
    }

    /// What a project page lists (`namespace_objects`, events, releases).
    const PROJECT_READS: &[&str] = &[
        "deployments.apps",
        "statefulsets.apps",
        "daemonsets.apps",
        "replicasets.apps",
        "controllerrevisions.apps",
        "jobs.batch",
        "cronjobs.batch",
        "pods",
        "services",
        "configmaps",
        "persistentvolumeclaims",
        "ingresses.networking.k8s.io",
        "networkpolicies.networking.k8s.io",
        "httproutes.gateway.networking.k8s.io",
        "events.events.k8s.io",
    ];

    #[test]
    fn the_chart_ships_the_roles_zeabur_binds_with_no_wildcard() {
        let roles = roles();
        let names: Vec<&str> = roles.keys().map(String::as_str).collect();
        assert_eq!(
            names,
            [
                BASELINE_ROLE,
                ProjectRole::Developer.cluster_role(),
                ProjectRole::Viewer.cluster_role()
            ]
        );
        for (name, role) in &roles {
            for rule in &role.rules {
                for value in rule.verbs.iter().chain(&rule.groups).chain(&rule.resources) {
                    assert_ne!(value, "*", "{name}");
                }
            }
        }
    }

    #[test]
    fn both_project_roles_read_what_a_project_page_lists_and_only_developers_change_it() {
        let roles = roles();
        let developer = &roles[ProjectRole::Developer.cluster_role()];
        let viewer = &roles[ProjectRole::Viewer.cluster_role()];
        for resource in PROJECT_READS {
            assert!(allows(viewer, "list", resource), "viewer lists {resource}");
            assert!(
                allows(developer, "list", resource),
                "developer lists {resource}"
            );
        }
        assert!(allows(viewer, "get", "pods/log"));
        for (verb, resource) in [
            ("patch", "deployments.apps"),
            ("delete", "pods"),
            ("create", "configmaps"),
            ("patch", "services"),
            ("create", "httproutes.gateway.networking.k8s.io"),
        ] {
            assert!(
                allows(developer, verb, resource),
                "developer {verb} {resource}"
            );
            assert!(!allows(viewer, verb, resource), "viewer {verb} {resource}");
        }
    }

    #[test]
    fn both_project_roles_see_builds_and_developers_build_again() {
        let roles = roles();
        let developer = &roles[ProjectRole::Developer.cluster_role()];
        let viewer = &roles[ProjectRole::Viewer.cluster_role()];
        for resource in ["builds.builder.zeabur.com", "buildruns.builder.zeabur.com"] {
            assert!(allows(viewer, "list", resource), "viewer lists {resource}");
            assert!(
                allows(developer, "list", resource),
                "developer lists {resource}"
            );
        }
        assert!(allows(developer, "patch", "builds.builder.zeabur.com"));
        assert!(!allows(viewer, "patch", "builds.builder.zeabur.com"));
        assert!(!allows(developer, "create", "buildruns.builder.zeabur.com"));
        assert!(!allows(developer, "delete", "builds.builder.zeabur.com"));
    }

    #[test]
    fn secrets_shells_and_patching_the_namespace_are_developers_and_nobody_gets_rbac_or_kubelets() {
        let roles = roles();
        let sensitive = [
            ("list", "secrets"),
            ("get", "secrets"),
            ("create", "pods/exec"),
            ("get", "pods/exec"),
            ("create", "pods/attach"),
            ("create", "pods/portforward"),
            ("patch", "pods/ephemeralcontainers"),
        ];
        for (name, role) in &roles {
            let developer = name == ProjectRole::Developer.cluster_role();
            for (verb, resource) in sensitive {
                assert_eq!(
                    allows(role, verb, resource),
                    developer,
                    "{name} {verb} {resource}"
                );
            }
            for verb in [
                "get", "create", "update", "patch", "delete", "bind", "escalate",
            ] {
                for resource in [
                    "roles.rbac.authorization.k8s.io",
                    "rolebindings.rbac.authorization.k8s.io",
                    "clusterroles.rbac.authorization.k8s.io",
                    "clusterrolebindings.rbac.authorization.k8s.io",
                ] {
                    assert!(!allows(role, verb, resource), "{name} {verb} {resource}");
                }
            }
            assert!(!allows(role, "get", "nodes/proxy"), "{name}");
            assert!(!allows(role, "impersonate", "users"), "{name}");
            assert!(!allows(role, "impersonate", "serviceaccounts"), "{name}");
            assert!(!allows(role, "create", "serviceaccounts/token"), "{name}");
            // The patch reaches only the project's own Namespace (RoleBinding), and the chart's
            // admission policy keeps it to the details without `zeabur-manage`.
            assert_eq!(allows(role, "patch", "namespaces"), developer, "{name}");
            for verb in [
                "create",
                "delete",
                "update",
                "deletecollection",
                MANAGE_VERB,
            ] {
                assert!(!allows(role, verb, "namespaces"), "{name} {verb}");
            }
        }
        let baseline = &roles[BASELINE_ROLE];
        for verb in ["get", "list"] {
            assert!(allows(baseline, verb, "nodes"));
            assert!(allows(baseline, verb, "namespaces"));
        }
        assert!(!allows(baseline, "list", "pods"));
    }

    #[test]
    fn the_namespace_policy_lets_through_exactly_the_details_and_the_manage_verb() {
        let policy =
            include_str!("../../../../deploy/chart/zeabur/templates/namespace-policy.yaml");
        let details = format!(
            "['{ANNOTATION_DISPLAY_NAME}', '{ANNOTATION_DESCRIPTION}', '{ANNOTATION_ICON}']"
        );
        assert!(policy.contains(&details), "{details}");
        assert!(policy.contains(&format!(".check('{MANAGE_VERB}')")));
        assert!(!policy.contains(LABEL_PROJECT));
    }
}
