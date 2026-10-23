//! What the person may change in a project, and in the cluster, before anything is planned.
//!
//! One `SelfSubjectRulesReview` per namespace, the `kubectl auth can-i --list` of the person,
//! answers every right below; an interface uses them to disable what the person's role cannot do
//! and say which role can. They are hints: an incomplete review (another authorizer may allow
//! more) or a failed one answers `unknown`, and the plan's dry run decides.

use serde::Serialize;
use utoipa::ToSchema;
use zeabur_kube::{AccessRules, ResourceRef};

use super::namespace::Permission;

/// What the person may change in one project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRights {
    /// Create services: `create` on deployments and services.
    pub deploy: Permission,
    /// Change, restart and redeploy services: `patch` on a workload kind.
    pub change_services: Permission,
    /// Delete services: `delete` on a workload kind.
    pub delete_services: Permission,
    /// Change the number of replicas: `patch` on `deployments/scale` or `statefulsets/scale`.
    pub scale: Permission,
    /// Read Secret values: `get` on secrets.
    pub read_secrets: Permission,
    /// Create and change Secrets: `create` and `patch` on secrets.
    pub change_secrets: Permission,
    /// Create and change ConfigMaps: `create` and `patch` on configmaps.
    pub change_config: Permission,
    /// Add, change and remove domains: `create`, `patch` and `delete` on ingresses or HTTPRoutes.
    pub change_domains: Permission,
    /// Add and change volumes: `create` and `patch` on persistentvolumeclaims.
    pub change_storage: Permission,
    /// Create, change and delete databases of any operator Zeabur knows.
    pub change_databases: Permission,
    /// Back up databases: `create` on an operator's backups.
    pub back_up_databases: Permission,
    /// Create, change and delete object storage: `create`, `patch` and `delete` on the `SeaweedFS`
    /// Operator's seaweeds.
    pub change_object_storage: Permission,
    /// Back up volumes: `create` on K8up's schedules, backups and pod configs.
    pub back_up_volumes: Permission,
    /// Open a terminal in a container: `create` on `pods/exec`.
    pub open_terminal: Permission,
    /// Read the pods' logs: `get` on `pods/log`.
    pub read_logs: Permission,
    /// Build again: `patch` on Zeabur Builder's builds.
    pub build: Permission,
    /// Deploy a service from Git: `create` on deployments, services and Zeabur Builder's builds.
    pub deploy_from_git: Permission,
    /// Change network policies: `create` and `patch` on networkpolicies.
    pub change_network_policies: Permission,
    /// Give people access to the project: `create` on rolebindings.
    pub change_members: Permission,
}

/// What the person may change in the cluster itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterRights {
    /// Install components with Zeabur: `create` on `helmcharts.helm.cattle.io` in `kube-system`.
    pub install_components: Permission,
    /// Change nodes (labels, taints, cordon): `patch` on nodes.
    pub change_nodes: Permission,
    /// Set up Velero backups: `create` on `schedules.velero.io`. The backups page asks again in
    /// Velero's own namespace.
    pub change_backups: Permission,
}

fn any(answers: impl IntoIterator<Item = Permission>) -> Permission {
    answers
        .into_iter()
        .fold(Permission::Denied, |sum, answer| match (sum, answer) {
            (Permission::Allowed, _) | (_, Permission::Allowed) => Permission::Allowed,
            (Permission::Unknown, _) | (_, Permission::Unknown) => Permission::Unknown,
            _ => Permission::Denied,
        })
}

fn all(answers: impl IntoIterator<Item = Permission>) -> Permission {
    answers
        .into_iter()
        .fold(Permission::Allowed, |sum, answer| match (sum, answer) {
            (Permission::Denied, _) | (_, Permission::Denied) => Permission::Denied,
            (Permission::Unknown, _) | (_, Permission::Unknown) => Permission::Unknown,
            _ => Permission::Allowed,
        })
}

struct Ask<'a>(&'a AccessRules);

impl Ask<'_> {
    fn can(&self, verb: &str, group: &str, resource: &str) -> Permission {
        self.0
            .allows(verb, &ResourceRef::new(group, "v1", resource, ""), "")
            .into()
    }

    fn every(&self, verbs: &[&str], group: &str, resource: &str) -> Permission {
        all(verbs.iter().map(|verb| self.can(verb, group, resource)))
    }

    fn workloads(&self, verb: &str) -> Permission {
        any(["deployments", "statefulsets", "daemonsets"]
            .map(|resource| self.can(verb, "apps", resource)))
    }
}

const CNPG: &str = "postgresql.cnpg.io";
const PSMDB: &str = "psmdb.percona.com";
const MOCO: &str = "moco.cybozu.com";
const MARIADB: &str = "k8s.mariadb.com";
const DRAGONFLY: &str = "dragonflydb.io";
const SEAWEED: &str = "seaweed.seaweedfs.com";
const K8UP: &str = "k8up.io";

impl ProjectRights {
    /// Every right unknown: the review could not be made.
    #[must_use]
    pub const fn unknown() -> Self {
        let u = Permission::Unknown;
        Self {
            deploy: u,
            change_services: u,
            delete_services: u,
            scale: u,
            read_secrets: u,
            change_secrets: u,
            change_config: u,
            change_domains: u,
            change_storage: u,
            change_databases: u,
            back_up_databases: u,
            change_object_storage: u,
            back_up_volumes: u,
            open_terminal: u,
            read_logs: u,
            build: u,
            deploy_from_git: u,
            change_network_policies: u,
            change_members: u,
        }
    }

    /// The rights `rules` give in their namespace.
    #[must_use]
    pub fn of(rules: &AccessRules) -> Self {
        let ask = Ask(rules);
        let change = ["create", "patch", "delete"];
        Self {
            deploy: all([
                ask.can("create", "apps", "deployments"),
                ask.can("create", "", "services"),
            ]),
            change_services: ask.workloads("patch"),
            delete_services: ask.workloads("delete"),
            scale: any([
                ask.can("patch", "apps", "deployments/scale"),
                ask.can("patch", "apps", "statefulsets/scale"),
            ]),
            read_secrets: ask.can("get", "", "secrets"),
            change_secrets: ask.every(&["create", "patch"], "", "secrets"),
            change_config: ask.every(&["create", "patch"], "", "configmaps"),
            change_domains: any([
                ask.every(&change, "networking.k8s.io", "ingresses"),
                ask.every(&change, "gateway.networking.k8s.io", "httproutes"),
            ]),
            change_storage: ask.every(&["create", "patch"], "", "persistentvolumeclaims"),
            change_databases: any([
                ask.every(&change, CNPG, "clusters"),
                ask.every(&change, PSMDB, "perconaservermongodbs"),
                ask.every(&change, MOCO, "mysqlclusters"),
                ask.every(&change, MARIADB, "mariadbs"),
                ask.every(&change, DRAGONFLY, "dragonflies"),
            ]),
            back_up_databases: any([
                ask.can("create", CNPG, "backups"),
                ask.can("create", PSMDB, "perconaservermongodbbackups"),
                ask.can("create", MOCO, "backuppolicies"),
                ask.can("create", MARIADB, "backups"),
            ]),
            change_object_storage: ask.every(&change, SEAWEED, "seaweeds"),
            back_up_volumes: all([
                ask.can("create", K8UP, "schedules"),
                ask.can("create", K8UP, "backups"),
                ask.can("create", K8UP, "podconfigs"),
            ]),
            open_terminal: ask.can("create", "", "pods/exec"),
            read_logs: ask.can("get", "", "pods/log"),
            build: ask.can("patch", "builder.zeabur.com", "builds"),
            deploy_from_git: all([
                ask.can("create", "apps", "deployments"),
                ask.can("create", "", "services"),
                ask.can("create", "builder.zeabur.com", "builds"),
            ]),
            change_network_policies: ask.every(
                &["create", "patch"],
                "networking.k8s.io",
                "networkpolicies",
            ),
            change_members: ask.can("create", "rbac.authorization.k8s.io", "rolebindings"),
        }
    }
}

impl ClusterRights {
    /// The rights `rules`, reviewed in `kube-system`, give. A `ClusterRoleBinding`'s rules are in
    /// every namespace's review, so cluster-scoped kinds such as nodes are answered too.
    #[must_use]
    pub fn of(rules: Option<&AccessRules>) -> Self {
        let Some(rules) = rules else {
            return Self {
                install_components: Permission::Unknown,
                change_nodes: Permission::Unknown,
                change_backups: Permission::Unknown,
            };
        };
        let ask = Ask(rules);
        Self {
            install_components: ask.can("create", "helm.cattle.io", "helmcharts"),
            change_nodes: ask.can("patch", "", "nodes"),
            change_backups: ask.can("create", "velero.io", "schedules"),
        }
    }
}

#[cfg(test)]
mod tests {
    use zeabur_kube::AccessRule;

    use super::*;
    use crate::members::access::ProjectRole;
    use crate::members::access::tests::roles;

    fn rights(role: ProjectRole) -> ProjectRights {
        ProjectRights::of(&roles()[role.cluster_role()])
    }

    #[test]
    fn a_developer_changes_everything_in_the_project_except_who_has_access() {
        let developer = rights(ProjectRole::Developer);
        let allowed = Permission::Allowed;
        assert_eq!(
            developer,
            ProjectRights {
                change_members: Permission::Denied,
                ..ProjectRights {
                    deploy: allowed,
                    change_services: allowed,
                    delete_services: allowed,
                    scale: allowed,
                    read_secrets: allowed,
                    change_secrets: allowed,
                    change_config: allowed,
                    change_domains: allowed,
                    change_storage: allowed,
                    change_databases: allowed,
                    back_up_databases: allowed,
                    change_object_storage: allowed,
                    back_up_volumes: allowed,
                    open_terminal: allowed,
                    read_logs: allowed,
                    build: allowed,
                    deploy_from_git: allowed,
                    change_network_policies: allowed,
                    change_members: allowed,
                }
            }
        );
    }

    #[test]
    fn a_viewer_changes_nothing_and_reads_no_secrets_or_logs() {
        let viewer = serde_json::to_value(rights(ProjectRole::Viewer)).unwrap();
        for (right, answer) in viewer.as_object().unwrap() {
            assert_eq!(answer, "denied", "{right}");
        }
    }

    #[test]
    fn a_cluster_admin_may_do_everything() {
        let admin = serde_json::to_value(ProjectRights::of(&AccessRules::everything())).unwrap();
        for (right, answer) in admin.as_object().unwrap() {
            assert_eq!(answer, "allowed", "{right}");
        }
        let cluster = ClusterRights::of(Some(&AccessRules::everything()));
        assert_eq!(cluster.install_components, Permission::Allowed);
        assert_eq!(cluster.change_nodes, Permission::Allowed);
        assert_eq!(cluster.change_backups, Permission::Allowed);
    }

    #[test]
    fn an_incomplete_review_leaves_what_it_does_not_allow_unknown() {
        let mut rules = roles()[ProjectRole::Viewer.cluster_role()].clone();
        rules.incomplete = true;
        let viewer = ProjectRights::of(&rules);
        assert_eq!(viewer.deploy, Permission::Unknown);
        assert_eq!(viewer.read_logs, Permission::Unknown);
        assert_eq!(ProjectRights::unknown().change_members, Permission::Unknown);
        assert_eq!(
            ClusterRights::of(None).install_components,
            Permission::Unknown
        );
    }

    #[test]
    fn one_workload_kind_or_one_kind_of_route_is_enough() {
        let rule = |group: &str, resource: &str, verbs: &[&str]| AccessRule {
            verbs: verbs.iter().map(|&v| v.to_owned()).collect(),
            groups: vec![group.to_owned()],
            resources: vec![resource.to_owned()],
            names: Vec::new(),
        };
        let rules = AccessRules {
            rules: vec![
                rule("apps", "statefulsets", &["patch"]),
                rule(
                    "gateway.networking.k8s.io",
                    "httproutes",
                    &["create", "patch", "delete"],
                ),
                rule("networking.k8s.io", "ingresses", &["create"]),
            ],
            incomplete: false,
        };
        let some = ProjectRights::of(&rules);
        assert_eq!(some.change_services, Permission::Allowed);
        assert_eq!(some.delete_services, Permission::Denied);
        assert_eq!(some.change_domains, Permission::Allowed);
    }

    #[test]
    fn the_member_roles_install_no_components_and_change_no_nodes() {
        for role in [ProjectRole::Developer, ProjectRole::Viewer] {
            let mut rules = roles()[role.cluster_role()].clone();
            rules
                .rules
                .extend(roles()[crate::members::access::BASELINE_ROLE].rules.clone());
            let cluster = ClusterRights::of(Some(&rules));
            assert_eq!(cluster.install_components, Permission::Denied, "{role:?}");
            assert_eq!(cluster.change_nodes, Permission::Denied, "{role:?}");
            assert_eq!(cluster.change_backups, Permission::Denied, "{role:?}");
        }
    }
}
