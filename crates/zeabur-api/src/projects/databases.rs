//! The databases capability in the API: `GET /projects/{p}/databases`.
//!
//! It lists the database clusters an operator runs in the project (CloudNativePG for PostgreSQL,
//! the Percona Operator for MongoDB; see `zeabur_modules::databases`), with their state, where to
//! connect and their backups.
//!
//! Each operator's objects are read as the person, in the project's namespace. An operator whose
//! API is not served is "not installed"; a refused read is named in `unreadable` and its engine
//! is "refused". The rest of the answer stands either way. No Secret is read: the connection
//! details come from the objects, and the interface reveals the password, as the person, only
//! when asked (`POST /projects/{p}/secrets/{name}/reveal`).
//!
//! Databases are created, changed and backed up through plans like any other change: the kinds
//! are in `zeabur_plan::CREATABLE`, and the writes go with the person's credential.

use axum::Json;
use axum::extract::Path;
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::Cluster;
use zeabur_modules::databases::{Database, Engine};
use zeabur_modules::{cloudnative_pg, percona_mongodb};

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
/// knows (CloudNativePG, the Percona Operator for MongoDB).
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
    operators.push(DatabaseOperator {
        engine: Engine::Mongodb,
        module: percona_mongodb::MODULE.id.to_owned(),
        state,
        backups: state,
    });

    items.sort_by(|a, b| (a.engine, &a.name).cmp(&(b.engine, &b.name)));
    Ok(ProjectDatabases {
        operators,
        items,
        unreadable,
    })
}
