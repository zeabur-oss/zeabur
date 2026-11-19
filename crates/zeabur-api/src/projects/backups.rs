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

use std::collections::BTreeMap;

use axum::extract::State;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_graph::{ObjectKey, OwnerGraph, Snapshot};
use zeabur_kube::k8s_openapi::api::apps::v1::StatefulSet;
use zeabur_kube::k8s_openapi::api::core::v1::PersistentVolumeClaim;
use zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use zeabur_kube::{Cluster, ClusterError, ClusterExt as _, DynamicObject, ResourceRef};
use zeabur_modules::backups::{self, Coverage, Findings, Run, Schedule, Target};
use zeabur_modules::{Module, Reading};

use crate::cluster::s3_destinations;
use crate::error::{ApiError, list_read, read_problem};
use crate::extract::{Json, Path};
use crate::handlers::{AppState, Person};
use crate::views::UnreadableView;

/// Whether a provider's objects could be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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
    /// The catalogue component that provides it, what to install when it is not installed.
    pub component: Option<zeabur_catalog::ComponentRef>,
}

/// A project's backups across providers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectBackups {
    /// One row per claim, then per database, then the namespace. A database's claims have no
    /// row of their own: they are backed up with the database.
    pub coverage: Vec<Coverage>,
    /// Claims and databases no active schedule covers.
    pub unscheduled: Vec<Target>,
    /// Every schedule that covers something of the project.
    pub schedules: Vec<Schedule>,
    /// The newest runs, newest first, ten at most.
    pub runs: Vec<Run>,
    /// The providers this build has.
    pub providers: Vec<BackupProvider>,
    /// What the cluster's Velero backups do for the project; absent when Velero could not be
    /// read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster: Option<ClusterCoverage>,
    /// The S3 destination each Secret's keys were copied from, by the Secret's name; empty
    /// where the person may not list Secrets.
    pub keys_from: BTreeMap<String, String>,
    /// What could not be read.
    pub unreadable: Vec<UnreadableView>,
}

/// What the cluster's Velero backups do for one project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(tag = "as", rename_all = "camelCase")]
pub enum ClusterCoverage {
    /// Velero is not installed.
    NoVelero,
    /// No active Velero schedule covers the project.
    NotCovered,
    /// A schedule covers it, and no backup of it has run yet.
    Waiting,
    /// The newest Velero backup of the project.
    Last {
        /// When it finished, else when it started (RFC 3339).
        #[serde(skip_serializing_if = "Option::is_none")]
        at: Option<String>,
        /// How it went.
        outcome: backups::Outcome,
    },
}

/// What Velero does for the project, from its namespace's coverage row.
fn cluster_coverage(
    providers: &[BackupProvider],
    coverage: &[Coverage],
) -> Option<ClusterCoverage> {
    let velero = providers
        .iter()
        .find(|p| p.module == zeabur_modules::velero::MODULE.id)?;
    match velero.state {
        ProviderState::Refused => return None,
        ProviderState::NotInstalled => return Some(ClusterCoverage::NoVelero),
        ProviderState::Installed => {}
    }
    let row = coverage
        .iter()
        .find(|c| c.target.kind == backups::TargetKind::Namespace);
    if let Some(run) = row
        .and_then(|r| r.last_run.as_ref())
        .filter(|r| r.provider == zeabur_modules::velero::MODULE.id)
    {
        return Some(ClusterCoverage::Last {
            at: run.time().map(str::to_owned),
            outcome: run.outcome,
        });
    }
    let prefix = format!("{}:", zeabur_modules::velero::MODULE.id);
    Some(
        if row.is_some_and(|r| r.scheduled_by.iter().any(|s| s.starts_with(&prefix))) {
            ClusterCoverage::Waiting
        } else {
            ClusterCoverage::NotCovered
        },
    )
}

/// Reads `resources` of one module; the objects of each, or the state that stopped it.
pub(crate) async fn read_all<const N: usize>(
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

pub(crate) fn provider(
    catalogue: &[zeabur_catalog::Component],
    module: &Module,
    state: ProviderState,
) -> BackupProvider {
    BackupProvider {
        module: module.id.to_owned(),
        state,
        component: crate::cluster::modules::component(catalogue, module),
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
    catalogue: &[zeabur_catalog::Component],
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
        g.providers.push(provider(catalogue, &k8up::MODULE, state));
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
        g.providers
            .push(provider(catalogue, &velero::MODULE, state));
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
        g.providers
            .push(provider(catalogue, &cloudnative_pg::MODULE, state));
    }

    Ok(g)
}

/// The names of the claims that are volumes and not a database's: a claim whose owners lead to a
/// database operator's object ([`OwnerGraph::database`]) is backed up with that database. When
/// the StatefulSets cannot be read (named in `unreadable`), a claim from a claim template counts
/// as a volume.
async fn volumes(
    cluster: &dyn Cluster,
    project: &str,
    claims: &[ObjectMeta],
    unreadable: &mut Vec<UnreadableView>,
) -> Result<Vec<String>, ClusterError> {
    let stateful_sets = match cluster.list::<StatefulSet>(Some(project)).await {
        Ok(sets) => sets,
        Err(error) => {
            let resource = ResourceRef::of::<StatefulSet>().key();
            unreadable.push(error.into_unreadable(&resource)?.into());
            Vec::new()
        }
    };
    let snapshot = Snapshot {
        namespace: project.to_owned(),
        stateful_sets,
        persistent_volume_claims: claims
            .iter()
            .map(|metadata| PersistentVolumeClaim {
                metadata: metadata.clone(),
                ..PersistentVolumeClaim::default()
            })
            .collect(),
        ..Snapshot::default()
    };
    let graph = OwnerGraph::new(&snapshot);
    Ok(snapshot
        .persistent_volume_claims
        .iter()
        .filter(|claim| graph.database(&ObjectKey::of(*claim)).is_none())
        .filter_map(|claim| claim.metadata.name.clone())
        .collect())
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
    State(state): State<AppState>,
    Person(cluster): Person,
    Path(project): Path<String>,
) -> Result<Json<ProjectBackups>, ApiError> {
    read(cluster.as_ref(), &state.catalogue, &project)
        .await
        .map(Json)
}

/// The backups of `project`, read as the person (the answer of [`get`]; also what the backup
/// gap rules look at).
pub(crate) async fn read(
    cluster: &dyn Cluster,
    catalogue: &[zeabur_catalog::Component],
    project: &str,
) -> Result<ProjectBackups, ApiError> {
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
        mut unreadable,
    } = gather(cluster, catalogue, &project, &claims, &names)
        .await
        .map_err(failed)?;
    let volumes = volumes(cluster, &project, &claims, &mut unreadable)
        .await
        .map_err(failed)?;

    let targets = backups::targets(&project, &volumes, &databases);
    let coverage = backups::coverage(&targets, &findings);
    let unscheduled = backups::unscheduled(&coverage)
        .into_iter()
        .map(|c| c.target.clone())
        .collect();
    let mut schedules: Vec<Schedule> = findings.iter().flat_map(|f| f.schedules.clone()).collect();
    schedules.sort_by(|a, b| (&a.provider, &a.name).cmp(&(&b.provider, &b.name)));
    let mut runs: Vec<Run> = findings.into_iter().flat_map(|f| f.runs).collect();
    backups::newest(&mut runs, 10);
    let keys_from = cluster
        .list_metadata(
            &ResourceRef::new("", "v1", "secrets", "Secret"),
            Some(&project),
        )
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
        .iter()
        .filter_map(|meta| Some((meta.name.clone()?, s3_destinations::copied_from(meta)?)))
        .collect();
    Ok(ProjectBackups {
        cluster: cluster_coverage(&providers, &coverage),
        coverage,
        unscheduled,
        schedules,
        runs,
        providers,
        keys_from,
        unreadable,
    })
}
