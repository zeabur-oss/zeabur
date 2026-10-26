//! Database operators: which workloads they run, and their own controller Deployments.

use std::collections::BTreeMap;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

use crate::adapters::group::{COMPONENT_LABEL, NAME_LABEL};
use crate::adapters::label;

/// A database engine an operator runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Engine {
    /// PostgreSQL.
    PostgreSql,
    /// MongoDB.
    MongoDb,
    /// MySQL.
    MySql,
    /// MariaDB.
    MariaDb,
    /// Dragonfly, a Redis-compatible store.
    Dragonfly,
    /// `SeaweedFS`, an S3-compatible object store.
    SeaweedFs,
}

impl Engine {
    /// The engine's name as people write it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PostgreSql => "PostgreSQL",
            Self::MongoDb => "MongoDB",
            Self::MySql => "MySQL",
            Self::MariaDb => "MariaDB",
            Self::Dragonfly => "Dragonfly",
            Self::SeaweedFs => "SeaweedFS",
        }
    }
}

/// A controller owner reference that makes a workload part of a database cluster.
struct DatabaseOwner {
    /// The owner's API group.
    group: &'static str,
    /// The owner's kind.
    kind: &'static str,
    /// The kind people create, which is not always the owner's: Percona's `PerconaPGCluster`
    /// creates a `PostgresCluster` of the same name, and that owns each StatefulSet.
    shown_kind: &'static str,
    engine: Engine,
    /// The label marking the workload that holds the data, shown under the cluster's name.
    data_label: Option<&'static str>,
}

/// The Crunchy Data label on each StatefulSet holding a PostgreSQL instance's data.
const CRUNCHY_DATA_LABEL: &str = "postgres-operator.crunchydata.com/data";

const DATABASE_OWNERS: [DatabaseOwner; 10] = [
    // CloudNativePG's pods and Jobs belong to the `Cluster` itself, with no StatefulSet between.
    DatabaseOwner {
        group: "postgresql.cnpg.io",
        kind: "Cluster",
        shown_kind: "Cluster",
        engine: Engine::PostgreSql,
        data_label: None,
    },
    DatabaseOwner {
        group: "upstream.pgv2.percona.com",
        kind: "PostgresCluster",
        shown_kind: "PerconaPGCluster",
        engine: Engine::PostgreSql,
        data_label: Some(CRUNCHY_DATA_LABEL),
    },
    DatabaseOwner {
        group: "postgres-operator.crunchydata.com",
        kind: "PostgresCluster",
        shown_kind: "PostgresCluster",
        engine: Engine::PostgreSql,
        data_label: Some(CRUNCHY_DATA_LABEL),
    },
    DatabaseOwner {
        group: "mongodbcommunity.mongodb.com",
        kind: "MongoDBCommunity",
        shown_kind: "MongoDBCommunity",
        engine: Engine::MongoDb,
        data_label: None,
    },
    DatabaseOwner {
        group: "psmdb.percona.com",
        kind: "PerconaServerMongoDB",
        shown_kind: "PerconaServerMongoDB",
        engine: Engine::MongoDb,
        data_label: None,
    },
    DatabaseOwner {
        group: "pxc.percona.com",
        kind: "PerconaXtraDBCluster",
        shown_kind: "PerconaXtraDBCluster",
        engine: Engine::MySql,
        data_label: None,
    },
    DatabaseOwner {
        group: "moco.cybozu.com",
        kind: "MySQLCluster",
        shown_kind: "MySQLCluster",
        engine: Engine::MySql,
        data_label: None,
    },
    DatabaseOwner {
        group: "k8s.mariadb.com",
        kind: "MariaDB",
        shown_kind: "MariaDB",
        engine: Engine::MariaDb,
        data_label: None,
    },
    DatabaseOwner {
        group: "dragonflydb.io",
        kind: "Dragonfly",
        shown_kind: "Dragonfly",
        engine: Engine::Dragonfly,
        data_label: None,
    },
    DatabaseOwner {
        group: "seaweed.seaweedfs.com",
        kind: "Seaweed",
        shown_kind: "Seaweed",
        engine: Engine::SeaweedFs,
        data_label: None,
    },
];

/// Labels on the controller Deployments of known database operators, whose charts set no
/// component label on the Deployment.
const OPERATOR_LABELS: [&str; 2] = [
    "pgv2.percona.com/control-plane",
    "postgres-operator.crunchydata.com/control-plane",
];

/// `app.kubernetes.io/name` values of known database operators.
const OPERATOR_NAMES: [&str; 14] = [
    "cloudnative-pg",
    "plugin-barman-cloud",
    "psmdb-operator",
    "mongodb-kubernetes-operator",
    "percona-server-mongodb-operator",
    "percona-xtradb-cluster-operator",
    "pg-operator",
    "percona-postgresql-operator",
    "moco",
    "mariadb-operator",
    "mariadb-operator-webhook",
    "mariadb-operator-cert-controller",
    "dragonfly-operator",
    "seaweedfs-operator",
];

/// The database cluster a workload belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseCluster<'a> {
    /// The kind people create, such as `PerconaPGCluster`.
    pub kind: &'static str,
    /// The cluster's name.
    pub name: &'a str,
    /// Its engine.
    pub engine: Engine,
    /// The workload holds the cluster's data, so it is shown under the cluster's name.
    pub holds_data: bool,
}

/// The database cluster whose operator controls this object, from its controller owner
/// reference.
///
/// Only the controller reference counts: any object may list other owners (for garbage
/// collection), and that does not hand it to their operator.
#[must_use]
pub fn database_cluster(meta: &ObjectMeta) -> Option<DatabaseCluster<'_>> {
    crate::adapters::controller_of(meta).and_then(|owner| {
        let group = api_group(&owner.api_version);
        DATABASE_OWNERS
            .iter()
            .find(|rule| rule.group == group && rule.kind == owner.kind)
            .map(|rule| DatabaseCluster {
                kind: rule.shown_kind,
                name: &owner.name,
                engine: rule.engine,
                holds_data: rule
                    .data_label
                    .is_some_and(|key| label(meta.labels.as_ref(), key).is_some()),
            })
    })
}

/// Whether a Deployment with these labels, and these pod template labels, is an operator's
/// controller.
///
/// The pod template's component label counts only when the Deployment sets none itself: some
/// charts label only the pods.
#[must_use]
pub fn is_operator(
    labels: Option<&BTreeMap<String, String>>,
    pod_labels: Option<&BTreeMap<String, String>>,
) -> bool {
    OPERATOR_LABELS
        .iter()
        .any(|key| label(labels, key).is_some())
        || label(labels, NAME_LABEL).is_some_and(|name| OPERATOR_NAMES.contains(&name))
        || label(labels, COMPONENT_LABEL).map_or_else(
            || label(pod_labels, COMPONENT_LABEL) == Some("operator"),
            |component| component == "operator",
        )
}

/// The API group of an `apiVersion`: what precedes the `/`, or empty for the core group.
fn api_group(api_version: &str) -> &str {
    api_version
        .split_once('/')
        .map_or("", |(group, _version)| group)
}
