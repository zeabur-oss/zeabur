//! Zeabur Builder in the API: `GET /projects/{p}/builds`, the project's `Build`s and their runs
//! (see `zeabur_modules::zeabur_builder`), read as the person.
//!
//! Which service a `Build` builds for is the page's to decide, by image repository; the answer
//! gives each run the pod whose logs show it, when the pod is still there (its Job is deleted
//! ten minutes after it finishes).

use axum::Json;
use axum::extract::Path;
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::api::core::v1::Pod;
use zeabur_kube::{Cluster, ClusterError, ClusterExt as _, ResourceRef};
use zeabur_modules::Reading;
use zeabur_modules::zeabur_builder::{
    self, BUILD_RUNS, BUILDS, RUN_LABEL, ZeaburBuilderBuild, ZeaburBuilderRun,
};

use crate::error::ApiError;
use crate::handlers::Person;
use crate::projects::backups::ProviderState;
use crate::views::UnreadableView;

/// A run and the pod that ran it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectBuildRun {
    /// The run.
    #[serde(flatten)]
    pub run: ZeaburBuilderRun,
    /// Its pod, while it is there.
    pub pod: Option<String>,
    /// The pod's containers, in the order they run (`fetch`, `plan`, `build`).
    pub containers: Vec<String>,
}

/// A project's builds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectBuilds {
    /// Whether Zeabur Builder could be read.
    pub state: ProviderState,
    /// The `Build`s, by name.
    pub builds: Vec<ZeaburBuilderBuild>,
    /// Their runs, newest first.
    pub runs: Vec<ProjectBuildRun>,
    /// What could not be read.
    pub unreadable: Vec<UnreadableView>,
}

/// The project's Zeabur Builder `Build`s and their runs, with the pod of each run, read as the
/// person. `state` is `not-installed` when the cluster does not serve `builder.zeabur.com`.
#[utoipa::path(get, path = "/api/v1/projects/{project}/builds", tag = "projects",
    operation_id = "getProjectBuilds",
    params(("project" = String, Path, description = "The project's namespace")),
    responses((status = 200, body = ProjectBuilds), (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    Person(cluster): Person,
    Path(project): Path<String>,
) -> Result<Json<ProjectBuilds>, ApiError> {
    read(cluster.as_ref(), &project)
        .await
        .map(Json)
        .map_err(|e| ApiError::from(e.into_problem(&format!("builds of {project}"))))
}

async fn read(cluster: &dyn Cluster, project: &str) -> Result<ProjectBuilds, ClusterError> {
    let mut unreadable = Vec::new();
    let answer = |state| ProjectBuilds {
        state,
        builds: Vec::new(),
        runs: Vec::new(),
        unreadable: Vec::new(),
    };
    let builds = match crate::cluster::modules::read_in(cluster, &BUILDS, Some(project)).await? {
        Reading::Objects(objects) => objects,
        Reading::NotInstalled => return Ok(answer(ProviderState::NotInstalled)),
        Reading::Refused(refusal) => {
            let mut out = answer(ProviderState::Refused);
            out.unreadable.push(refusal);
            return Ok(out);
        }
    };
    let runs = match crate::cluster::modules::read_in(cluster, &BUILD_RUNS, Some(project)).await? {
        Reading::Objects(objects) => objects,
        Reading::NotInstalled => Vec::new(),
        Reading::Refused(refusal) => {
            unreadable.push(refusal);
            Vec::new()
        }
    };
    let pods: Vec<Pod> = match cluster.list(Some(project)).await {
        Ok(pods) => pods,
        Err(error) => {
            let key = ResourceRef::of::<Pod>().key();
            unreadable.push(error.into_unreadable(&key)?.into());
            Vec::new()
        }
    };
    let runs = zeabur_builder::runs(&runs)
        .into_iter()
        .map(|run| {
            let pod = pods
                .iter()
                .filter(|p| {
                    p.metadata
                        .labels
                        .as_ref()
                        .and_then(|l| l.get(RUN_LABEL))
                        .is_some_and(|name| *name == run.name)
                })
                .max_by(|a, b| {
                    a.metadata
                        .creation_timestamp
                        .cmp(&b.metadata.creation_timestamp)
                });
            let containers = pod.and_then(|p| p.spec.as_ref()).map_or_default(|spec| {
                spec.init_containers
                    .iter()
                    .flatten()
                    .chain(&spec.containers)
                    .map(|c| c.name.clone())
                    .collect()
            });
            ProjectBuildRun {
                run,
                pod: pod.and_then(|p| p.metadata.name.clone()),
                containers,
            }
        })
        .collect();
    Ok(ProjectBuilds {
        state: ProviderState::Installed,
        builds: zeabur_builder::builds(&builds),
        runs,
        unreadable,
    })
}
