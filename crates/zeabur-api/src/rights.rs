//! The rights the interface gates actions by: the least role that holds each and the Kubernetes
//! rule behind it, and which right changes or deletes objects of a resource.
//!
//! Whether the person holds a right is answered elsewhere, by the reviews of the project, the
//! cluster or the page (`projects::namespace`, `views::ClusterRights`). This table only says what
//! to tell them when the answer is no: a Zeabur Identity member is told the role, anyone else the
//! rule.

use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

/// The least product role that holds a right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum NeededRole {
    /// Changes services, opens terminals and reads secrets.
    Developer,
    /// Also gives others access, and changes the cluster.
    Admin,
}

/// An action the interface gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum Right {
    /// Deploy a service.
    Deploy,
    /// Change a service.
    ChangeServices,
    /// Delete a service.
    DeleteServices,
    /// Change the replicas.
    Scale,
    /// Read secret values.
    ReadSecrets,
    /// Change secrets.
    ChangeSecrets,
    /// Change config maps.
    ChangeConfig,
    /// Add or change domains.
    ChangeDomains,
    /// Create a Kubernetes Service.
    CreateServices,
    /// Add or change volumes.
    ChangeStorage,
    /// Add or change databases.
    ChangeDatabases,
    /// Add or change object storage.
    ChangeObjectStorage,
    /// Back up a database.
    BackUpDatabases,
    /// Back up volumes.
    BackUpVolumes,
    /// Restore volumes.
    RestoreVolumes,
    /// Open a terminal in a pod.
    OpenTerminal,
    /// Forward a port to a pod.
    PortForward,
    /// Read logs.
    ReadLogs,
    /// Build again.
    Build,
    /// Deploy from Git.
    DeployFromGit,
    /// Change network policies.
    ChangeNetworkPolicies,
    /// Give others access.
    ChangeMembers,
    /// Make or remove API tokens.
    ChangeApiTokens,
    /// Change nodes.
    ChangeNodes,
    /// Change cluster backups.
    ChangeBackups,
    /// Restore a cluster backup.
    RestoreBackups,
    /// Change the domain suffixes.
    ChangeDomainSuffixes,
    /// Change a project's notifications.
    ChangeNotifications,
    /// Send a test notification.
    TestNotifications,
    /// Change the cluster's notifications.
    ChangeClusterNotifications,
    /// Send a test notification for the cluster.
    TestClusterNotifications,
    #[serde(rename = "changeS3Destinations")]
    /// Change S3 destinations.
    ChangeS3Destinations,
    #[serde(rename = "changeGitHubApp")]
    /// Set up the GitHub App.
    ChangeGitHubApp,
    /// Change a project's build limits.
    ChangeBuildLimits,
}

impl Right {
    const ALL: [Self; 34] = [
        Self::Deploy,
        Self::ChangeServices,
        Self::DeleteServices,
        Self::Scale,
        Self::ReadSecrets,
        Self::ChangeSecrets,
        Self::ChangeConfig,
        Self::ChangeDomains,
        Self::CreateServices,
        Self::ChangeStorage,
        Self::ChangeDatabases,
        Self::ChangeObjectStorage,
        Self::BackUpDatabases,
        Self::BackUpVolumes,
        Self::RestoreVolumes,
        Self::OpenTerminal,
        Self::PortForward,
        Self::ReadLogs,
        Self::Build,
        Self::DeployFromGit,
        Self::ChangeNetworkPolicies,
        Self::ChangeMembers,
        Self::ChangeApiTokens,
        Self::ChangeNodes,
        Self::ChangeBackups,
        Self::RestoreBackups,
        Self::ChangeDomainSuffixes,
        Self::ChangeNotifications,
        Self::TestNotifications,
        Self::ChangeClusterNotifications,
        Self::TestClusterNotifications,
        Self::ChangeS3Destinations,
        Self::ChangeGitHubApp,
        Self::ChangeBuildLimits,
    ];

    /// The least role that holds the right, and the rule that grants it.
    const fn needs(self) -> (NeededRole, &'static str) {
        use NeededRole::{Admin, Developer};
        match self {
            Self::Deploy => (Developer, "create deployments.apps"),
            Self::ChangeServices => (Developer, "patch deployments.apps"),
            Self::DeleteServices => (Developer, "delete deployments.apps"),
            Self::Scale => (Developer, "patch deployments/scale.apps"),
            Self::ReadSecrets => (Developer, "get secrets"),
            Self::ChangeSecrets => (Developer, "patch secrets"),
            Self::ChangeConfig => (Developer, "patch configmaps"),
            Self::ChangeDomains => (Developer, "create ingresses.networking.k8s.io"),
            Self::CreateServices => (Developer, "create services"),
            Self::ChangeStorage => (Developer, "create persistentvolumeclaims"),
            Self::ChangeDatabases => (Developer, "create clusters.postgresql.cnpg.io"),
            Self::ChangeObjectStorage => (Developer, "create seaweeds.seaweed.seaweedfs.com"),
            Self::BackUpDatabases => (Developer, "create backups.postgresql.cnpg.io"),
            Self::BackUpVolumes => (Developer, "create schedules.k8up.io"),
            Self::RestoreVolumes => (Developer, "create restores.k8up.io"),
            Self::OpenTerminal => (Developer, "create pods/exec"),
            Self::PortForward => (Developer, "create pods/portforward"),
            Self::ReadLogs => (Developer, "get pods/log"),
            Self::Build => (Developer, "patch builds.builder.zeabur.com"),
            Self::DeployFromGit => (Developer, "create builds.builder.zeabur.com"),
            Self::ChangeNetworkPolicies => (Developer, "patch networkpolicies.networking.k8s.io"),
            Self::ChangeMembers => (Admin, "create rolebindings.rbac.authorization.k8s.io"),
            Self::ChangeApiTokens => (Admin, "create serviceaccounts -n zeabur-api-tokens"),
            Self::ChangeNodes => (Admin, "patch nodes"),
            Self::ChangeBackups => (Admin, "create schedules.velero.io"),
            Self::RestoreBackups => (Admin, "create restores.velero.io"),
            Self::ChangeDomainSuffixes => (Admin, "patch configmaps/zeabur-settings"),
            Self::ChangeNotifications => (
                Developer,
                "create vmalertmanagerconfigs.operator.victoriametrics.com",
            ),
            Self::TestNotifications => (Developer, "create jobs.batch"),
            Self::ChangeClusterNotifications => (
                Admin,
                "create vmalertmanagerconfigs.operator.victoriametrics.com",
            ),
            Self::TestClusterNotifications => (Admin, "create jobs.batch"),
            Self::ChangeS3Destinations => (Admin, "create secrets -n zeabur-s3-destinations"),
            Self::ChangeGitHubApp => (Admin, "create secrets -n builder-system"),
            Self::ChangeBuildLimits => (Admin, "patch and zeabur-manage namespaces"),
        }
    }
}

/// What holds a right.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct RightNeeds {
    /// The right.
    pub right: Right,
    /// The least product role that holds it.
    pub role: NeededRole,
    /// The Kubernetes rule that grants it, for someone who is not a Zeabur Identity member.
    pub rule: &'static str,
}

/// The rights that change and delete objects of one resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ResourceRights {
    /// The resource (plural, `deployments`).
    pub resource: &'static str,
    /// The right to change one.
    pub edit: Right,
    /// The right to delete one.
    pub delete: Right,
}

/// Every right, and the resources a right covers. A resource not listed is left to the plan's
/// dry run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct Rights {
    /// Every right, with what holds it.
    pub rights: Vec<RightNeeds>,
    /// The resources a right covers.
    pub resources: Vec<ResourceRights>,
}

const WORKLOADS: [&str; 3] = ["deployments", "statefulsets", "daemonsets"];

const COVERED: [(&str, Right); 13] = [
    ("secrets", Right::ChangeSecrets),
    ("configmaps", Right::ChangeConfig),
    ("ingresses", Right::ChangeDomains),
    ("httproutes", Right::ChangeDomains),
    ("persistentvolumeclaims", Right::ChangeStorage),
    ("networkpolicies", Right::ChangeNetworkPolicies),
    ("clusters", Right::ChangeDatabases),
    ("perconaservermongodbs", Right::ChangeDatabases),
    ("mysqlclusters", Right::ChangeDatabases),
    ("mariadbs", Right::ChangeDatabases),
    ("dragonflies", Right::ChangeDatabases),
    ("seaweeds", Right::ChangeObjectStorage),
    ("buckets", Right::ChangeObjectStorage),
];

fn rights() -> Rights {
    let rights = Right::ALL
        .into_iter()
        .map(|right| {
            let (role, granted_by) = right.needs();
            RightNeeds {
                right,
                role,
                rule: granted_by,
            }
        })
        .collect();
    let workloads = WORKLOADS.into_iter().map(|resource| ResourceRights {
        resource,
        edit: Right::ChangeServices,
        delete: Right::DeleteServices,
    });
    let others = COVERED.into_iter().map(|(resource, right)| ResourceRights {
        resource,
        edit: right,
        delete: right,
    });
    Rights {
        rights,
        resources: workloads.chain(others).collect(),
    }
}

/// The rights the interface gates actions by, what holds each, and the resources each covers.
/// The same for everyone; whether you hold one is in the project's, the cluster's or the page's
/// answer.
#[utoipa::path(get, path = "/api/v1/rights", tag = "identity", operation_id = "listRights",
    responses((status = 200, body = Rights)))]
pub(crate) async fn list() -> Json<Rights> {
    Json(rights())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_workload_takes_the_service_rights_and_nodes_need_an_admin() {
        let listed = rights();
        let statefulsets = listed
            .resources
            .iter()
            .find(|r| r.resource == "statefulsets")
            .unwrap();
        assert_eq!(statefulsets.delete, Right::DeleteServices);
        let nodes = listed
            .rights
            .iter()
            .find(|r| r.right == Right::ChangeNodes)
            .unwrap();
        assert_eq!((nodes.role, nodes.rule), (NeededRole::Admin, "patch nodes"));
    }
}
