//! The backups capability in the API: `GET /projects/{p}/backups`, one route for every backup
//! provider this build has (K8up, Velero, CloudNativePG; see `zeabur_modules::backups`).
//!
//! Each provider's objects are read as the person: K8up's and CloudNativePG's in the project's
//! namespace, Velero's cluster-wide (they live in Velero's own namespace). A provider whose API
//! is not served is "not installed"; a refused read is named in `unreadable` and its provider is
//! "refused". The rest of the answer stands either way.
//!
//! This is its own route, not part of the project document, so that pages which do not show
//! backups do not pay for up to seven more LISTs on every refresh.

use axum::Json;
use axum::extract::Path;
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use zeabur_kube::{Cluster, ClusterError, DynamicObject, ResourceRef};
use zeabur_modules::backups::{self, Coverage, Findings, Run, Schedule, Target};
use zeabur_modules::{Module, Reading};

use crate::error::{ApiError, list_read, read_problem};
use crate::handlers::Person;
use crate::views::UnreadableView;

/// Whether a provider's objects could be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderState {
    /// Read.
    Installed,
    /// Its API is not served.
    NotInstalled,
    /// The person may not read its objects.
    Refused,
}

/// One backup provider of this build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BackupProvider {
    /// The module (`k8up`).
    pub module: String,
    /// Whether it could be read.
    pub state: ProviderState,
}

/// A project's backups across providers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectBackups {
    /// One row per claim, then per database, then the namespace.
    pub coverage: Vec<Coverage>,
    /// Claims and databases no active schedule covers.
    pub unscheduled: Vec<Target>,
    /// Every schedule that covers something of the project.
    pub schedules: Vec<Schedule>,
    /// The newest runs, newest first, ten at most.
    pub runs: Vec<Run>,
    /// The providers this build has.
    pub providers: Vec<BackupProvider>,
    /// What could not be read.
    pub unreadable: Vec<UnreadableView>,
}

/// Reads `resources` of one module; the objects of each, or the state that stopped it.
async fn read_all<const N: usize>(
    cluster: &dyn Cluster,
    reads: [&zeabur_modules::Read; N],
    namespace: Option<&str>,
    unreadable: &mut Vec<UnreadableView>,
) -> Result<Result<[Vec<DynamicObject>; N], ProviderState>, ClusterError> {
    let mut out: [Vec<DynamicObject>; N] = std::array::from_fn(|_| Vec::new());
    for (slot, read) in out.iter_mut().zip(reads) {
        match crate::cluster::modules::read_in(cluster, read, namespace).await? {
            Reading::Objects(objects) => *slot = objects,
            Reading::NotInstalled => return Ok(Err(ProviderState::NotInstalled)),
            Reading::Refused(refusal) => {
                unreadable.push(refusal);
                return Ok(Err(ProviderState::Refused));
            }
        }
    }
    Ok(Ok(out))
}

fn provider(module: &Module, state: ProviderState) -> BackupProvider {
    BackupProvider {
        module: module.id.to_owned(),
        state,
    }
}

/// What the providers found, and their states.
#[derive(Default)]
struct Gathered {
    findings: Vec<Findings>,
    providers: Vec<BackupProvider>,
    databases: Vec<Target>,
    unreadable: Vec<UnreadableView>,
}

/// Reads every provider this build has, as the person.
async fn gather(
    cluster: &dyn Cluster,
    project: &str,
    claims: &[ObjectMeta],
    names: &[String],
) -> Result<Gathered, ClusterError> {
    let mut g = Gathered::default();
    let unreadable = &mut g.unreadable;
    {
        use zeabur_modules::k8up;
        let state = match read_all(
            cluster,
            [&k8up::SCHEDULES, &k8up::BACKUPS],
            Some(project),
            unreadable,
        )
        .await?
        {
            Ok([schedules, runs]) => {
                let volumes: Vec<k8up::Volume> = claims
                    .iter()
                    .map(|c| k8up::Volume {
                        name: c.name.clone().unwrap_or_default(),
                        labels: c.labels.clone().unwrap_or_default(),
                        annotations: c.annotations.clone().unwrap_or_default(),
                    })
                    .collect();
                g.findings
                    .push(k8up::findings(project, &schedules, &runs, &volumes));
                ProviderState::Installed
            }
            Err(state) => state,
        };
        g.providers.push(provider(&k8up::MODULE, state));
    }
    {
        use zeabur_modules::velero;
        let state = match read_all(
            cluster,
            [&velero::SCHEDULES, &velero::BACKUPS],
            None,
            unreadable,
        )
        .await?
        {
            Ok([schedules, runs]) => {
                g.findings
                    .push(velero::findings(project, &schedules, &runs, names));
                ProviderState::Installed
            }
            Err(state) => state,
        };
        g.providers.push(provider(&velero::MODULE, state));
    }
    {
        use zeabur_modules::cloudnative_pg;
        let state = match read_all(
            cluster,
            [
                &cloudnative_pg::CLUSTERS,
                &cloudnative_pg::SCHEDULED_BACKUPS,
                &cloudnative_pg::BACKUPS,
            ],
            Some(project),
            unreadable,
        )
        .await?
        {
            Ok([clusters, scheduled, runs]) => {
                g.databases = cloudnative_pg::databases(&clusters);
                g.findings
                    .push(cloudnative_pg::findings(project, &scheduled, &runs));
                ProviderState::Installed
            }
            Err(state) => state,
        };
        g.providers.push(provider(&cloudnative_pg::MODULE, state));
    }

    Ok(g)
}

/// Backups of a project: which claims, databases and the namespace a schedule covers, and how
/// the latest backups went, from every backup provider this build has (K8up, Velero,
/// CloudNativePG), read as the person. Outcomes are as each provider reports them; Zeabur does
/// not test restores.
#[utoipa::path(get, path = "/api/v1/projects/{project}/backups", tag = "projects",
    operation_id = "getProjectBackups",
    params(("project" = String, Path, description = "The project's namespace")),
    responses((status = 200, body = ProjectBackups), (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    Person(cluster): Person,
    Path(project): Path<String>,
) -> Result<Json<ProjectBackups>, ApiError> {
    read(cluster.as_ref(), &project).await.map(Json)
}

/// The backups of `project`, read as the person (the answer of [`get`]; also what the backup
/// gap rules look at).
pub(crate) async fn read(cluster: &dyn Cluster, project: &str) -> Result<ProjectBackups, ApiError> {
    let project = project.to_owned();
    let failed = |e: ClusterError| ApiError::from(e.into_problem(&format!("backups of {project}")));
    let claims = cluster
        .list_metadata(
            &ResourceRef::new("", "v1", "persistentvolumeclaims", "PersistentVolumeClaim"),
            Some(&project),
        )
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("backups of {project}"),
                list_read("", "persistentvolumeclaims", Some(&project)),
            )
        })?
        .unwrap_or_default();
    let names: Vec<String> = claims.iter().filter_map(|c| c.name.clone()).collect();
    let Gathered {
        findings,
        providers,
        databases,
        unreadable,
    } = gather(cluster, &project, &claims, &names)
        .await
        .map_err(failed)?;

    let targets = backups::targets(&project, &names, &databases);
    let coverage = backups::coverage(&targets, &findings);
    let unscheduled = backups::unscheduled(&coverage)
        .into_iter()
        .map(|c| c.target.clone())
        .collect();
    let mut schedules: Vec<Schedule> = findings.iter().flat_map(|f| f.schedules.clone()).collect();
    schedules.sort_by(|a, b| (&a.provider, &a.name).cmp(&(&b.provider, &b.name)));
    let mut runs: Vec<Run> = findings.into_iter().flat_map(|f| f.runs).collect();
    backups::newest(&mut runs, 10);
    Ok(ProjectBackups {
        coverage,
        unscheduled,
        schedules,
        runs,
        providers,
        unreadable,
    })
}
