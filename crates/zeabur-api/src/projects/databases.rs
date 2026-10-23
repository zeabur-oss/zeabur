//! The databases capability in the API: `GET /projects/{p}/databases`.
//!
//! It lists the database clusters an operator runs in the project (CloudNativePG for PostgreSQL,
//! the Percona Operator for MongoDB, MOCO for MySQL, mariadb-operator for MariaDB, the Dragonfly
//! Operator for Dragonfly; see `zeabur_modules::databases`), with their state, where to connect
//! and their backups.
//!
//! MOCO and mariadb-operator take each backup in a Job. The project's Jobs are read once, and only
//! when one of their databases is there.
//!
//! Each operator's objects are read as the person, in the project's namespace. An operator whose
//! API is not served is "not installed"; a refused read is named in `unreadable` and its engine
//! is "refused". The rest of the answer stands either way. No Secret is read: the connection
//! details come from the objects, and the interface reveals the password, as the person, only
//! when asked (`POST /projects/{p}/secrets/{name}/reveal`).
//!
//! Each database carries its members' pods, which its logs, events and terminal are read from; a
//! refused pod read leaves them empty.
//!
//! Databases are created, changed and backed up through plans like any other change: the kinds
//! are in `zeabur_plan::CREATABLE`, and the writes go with the person's credential.

use axum::Json;
use axum::extract::Path;
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::api::core::v1::Pod;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{Cluster, ClusterExt, DynamicObject};
use zeabur_modules::databases::{self, Database, Engine};
use zeabur_modules::{Module, cloudnative_pg, dragonfly, mariadb_operator, moco, percona_mongodb};

use crate::error::ApiError;
use crate::handlers::Person;
use crate::projects::backups::{ProviderState, read_all};
use crate::views::UnreadableView;

/// Whether an engine's operator, and its backup component, could be read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseOperator {
    /// The engine.
    pub engine: Engine,
    /// The operator's module (`cloudnative-pg`).
    pub module: String,
    /// Whether its objects could be read.
    pub state: ProviderState,
    /// Whether what backups need could be read: the Barman Cloud plugin's `ObjectStore`s for
    /// PostgreSQL; the operator itself for MongoDB.
    pub backups: ProviderState,
}

/// A project's databases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDatabases {
    /// One per engine this build knows.
    pub operators: Vec<DatabaseOperator>,
    /// The databases, by engine, then name.
    pub items: Vec<Database>,
    /// What could not be read.
    pub unreadable: Vec<UnreadableView>,
}

/// The databases of a project, read as the person, from every database operator this build
/// knows (CloudNativePG, the Percona Operator for MongoDB, MOCO, mariadb-operator, the Dragonfly
/// Operator).
#[utoipa::path(get, path = "/api/v1/projects/{project}/databases", tag = "projects",
    operation_id = "getProjectDatabases",
    params(("project" = String, Path, description = "The project's namespace")),
    responses((status = 200, body = ProjectDatabases), (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    Person(cluster): Person,
    Path(project): Path<String>,
) -> Result<Json<ProjectDatabases>, ApiError> {
    read(cluster.as_ref(), &project).await.map(Json)
}

/// An operator whose backups are read with the database objects.
fn operator(engine: Engine, module: &Module, state: ProviderState) -> DatabaseOperator {
    DatabaseOperator {
        engine,
        module: module.id.to_owned(),
        state,
        backups: state,
    }
}

async fn read(cluster: &dyn Cluster, project: &str) -> Result<ProjectDatabases, ApiError> {
    let failed = |e: zeabur_kube::ClusterError| {
        ApiError::from(e.into_problem(&format!("databases of {project}")))
    };
    let mut unreadable = Vec::new();
    let mut items = Vec::new();
    let mut operators = Vec::new();

    let postgres = read_all(
        cluster,
        [
            &cloudnative_pg::CLUSTERS,
            &cloudnative_pg::SCHEDULED_BACKUPS,
            &cloudnative_pg::BACKUPS,
        ],
        Some(project),
        &mut unreadable,
    )
    .await
    .map_err(failed)?;
    let stores = read_all(
        cluster,
        [&cloudnative_pg::OBJECT_STORES],
        Some(project),
        &mut unreadable,
    )
    .await
    .map_err(failed)?;
    let (stores, store_state) = match stores {
        Ok([stores]) => (stores, ProviderState::Installed),
        Err(state) => (Vec::new(), state),
    };
    let state = match postgres {
        Ok([clusters, scheduled, backups]) => {
            items.extend(
                clusters
                    .iter()
                    .map(|c| cloudnative_pg::database(project, c, &stores, &scheduled, &backups)),
            );
            ProviderState::Installed
        }
        Err(state) => state,
    };
    operators.push(DatabaseOperator {
        engine: Engine::Postgresql,
        module: cloudnative_pg::MODULE.id.to_owned(),
        state,
        backups: store_state,
    });

    let mongo = read_all(
        cluster,
        [&percona_mongodb::CLUSTERS, &percona_mongodb::BACKUPS],
        Some(project),
        &mut unreadable,
    )
    .await
    .map_err(failed)?;
    let state = match mongo {
        Ok([clusters, backups]) => {
            items.extend(
                clusters
                    .iter()
                    .map(|c| percona_mongodb::database(project, c, &backups)),
            );
            ProviderState::Installed
        }
        Err(state) => state,
    };
    operators.push(operator(Engine::Mongodb, &percona_mongodb::MODULE, state));

    read_moco_mariadb_dragonfly(
        cluster,
        project,
        &mut items,
        &mut operators,
        &mut unreadable,
    )
    .await
    .map_err(failed)?;

    if !items.is_empty() {
        let pods = match cluster.list::<Pod>(Some(project)).await {
            Ok(pods) => pods,
            Err(error) if error.code() == Some(403) => Vec::new(),
            Err(error) => return Err(failed(error)),
        };
        add_members(project, &mut items, &pods);
    }
    items.sort_by(|a, b| (a.engine, &a.name).cmp(&(b.engine, &b.name)));
    Ok(ProjectDatabases {
        operators,
        items,
        unreadable,
    })
}

/// Gives each database its member pods, and Dragonfly its master.
fn add_members(project: &str, items: &mut [Database], pods: &[Pod]) {
    let now = Some(Timestamp::now());
    let empty = std::collections::BTreeMap::new();
    for database in items {
        let members: Vec<&Pod> = pods
            .iter()
            .filter(|p| database.is_member(p.metadata.labels.as_ref().unwrap_or(&empty)))
            .collect();
        // The Dragonfly Operator names its master only with a pod label.
        if database.engine == Engine::Dragonfly {
            database.primary = members
                .iter()
                .find(|p| {
                    p.metadata
                        .labels
                        .as_ref()
                        .and_then(|l| l.get("role"))
                        .is_some_and(|r| r == "master")
                })
                .and_then(|p| p.metadata.name.clone());
        }
        database.pods = members
            .iter()
            .map(|p| zeabur_graph::pod_summary(project, p, now))
            .collect();
        database.pods.sort_by(|a, b| a.name.cmp(&b.name));
    }
}

/// The databases of MOCO, mariadb-operator and the Dragonfly Operator. The project's Jobs are
/// read only when MOCO or mariadb-operator has a database there.
async fn read_moco_mariadb_dragonfly(
    cluster: &dyn Cluster,
    project: &str,
    items: &mut Vec<Database>,
    operators: &mut Vec<DatabaseOperator>,
    unreadable: &mut Vec<UnreadableView>,
) -> Result<(), zeabur_kube::ClusterError> {
    let mysql = read_all(
        cluster,
        [&moco::CLUSTERS, &moco::BACKUP_POLICIES],
        Some(project),
        unreadable,
    )
    .await?;
    let mariadb = read_all(
        cluster,
        [&mariadb_operator::SERVERS, &mariadb_operator::BACKUPS],
        Some(project),
        unreadable,
    )
    .await?;
    let has = |read: &Result<[Vec<DynamicObject>; 2], ProviderState>| {
        read.as_ref().is_ok_and(|[objects, _]| !objects.is_empty())
    };
    let jobs = if has(&mysql) || has(&mariadb) {
        match read_all(cluster, [&databases::JOBS], Some(project), unreadable).await? {
            Ok([jobs]) => jobs,
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };
    let state = match mysql {
        Ok([clusters, policies]) => {
            items.extend(
                clusters
                    .iter()
                    .map(|c| moco::database(project, c, &policies, &jobs)),
            );
            ProviderState::Installed
        }
        Err(state) => state,
    };
    operators.push(operator(Engine::Mysql, &moco::MODULE, state));
    let state = match mariadb {
        Ok([servers, backups]) => {
            items.extend(
                servers
                    .iter()
                    .map(|s| mariadb_operator::database(project, s, &backups, &jobs)),
            );
            ProviderState::Installed
        }
        Err(state) => state,
    };
    operators.push(operator(Engine::Mariadb, &mariadb_operator::MODULE, state));
    let state = match read_all(cluster, [&dragonfly::STORES], Some(project), unreadable).await? {
        Ok([stores]) => {
            items.extend(stores.iter().map(|s| dragonfly::database(project, s)));
            ProviderState::Installed
        }
        Err(state) => state,
    };
    operators.push(operator(Engine::Dragonfly, &dragonfly::MODULE, state));

    Ok(())
}
