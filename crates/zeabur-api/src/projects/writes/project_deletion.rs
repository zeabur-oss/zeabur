//! Deleting a project (`deleteProject`): what its plan reads, and the order its apply writes in.
//! See `zeabur_plan::ProjectDeletion` for what the plan says.
//!
//! The plan reads as the person: the Namespace, the objects in it, the database objects of each
//! operator this build knows, and the `StorageClass`es and the bound PersistentVolumes (for what
//! happens to the data), and the project's backups with the S3 endpoint each writes to (the
//! `databases` and `backups` routes' reads).
//! A read the person may not make leaves its part out; a refused read of database objects is told
//! as a right the person lacks.
//!
//! The apply dry-runs every write, then deletes the database objects in the foreground, waits up
//! to [`DATABASE_WAIT`] for them and their pods to go, deletes the Namespace, and
//! last deletes the person's tools pods for the project with Zeabur's service account. The wait
//! ends early when they are gone; past the deadline the Namespace is deleted all the
//! same, and the namespace controller deletes what is left.
//!
//! A force deletion (`forceDeleteProject`) reads, as the person, the Namespace and the objects
//! with finalizers the namespace controller waits on (the `namespace::deleting` module). Its plan
//! patches only those objects and is applied like any other.

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::Value;
use zeabur_core::problem::{Denied, Kind, Problem};
use zeabur_kube::k8s_openapi::api::core::v1::Pod;
use zeabur_kube::k8s_openapi::api::storage::v1::StorageClass;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{Cluster, ClusterError, ClusterExt as _, ResourceRef, WriteRequest};
use zeabur_modules::{
    Reading, backups, cloudnative_pg, dragonfly, k8up, mariadb_operator, moco, percona_mongodb,
    velero,
};
use zeabur_plan::terminal::{self as plans, ToolsScope};
use zeabur_plan::{
    BackupKind, BackupStore, DeletionInput, Input, Plan, PlanRequest, ProjectBackup, ProjectObject,
    Warning, Write,
};

use super::terminal::{self, Planner};
use super::{Applied, plan_problem, refusal, refused, write_request};
use crate::error::{ApiError, get_read, project_read, read_problem};
use crate::projects::namespace::held_objects;
use crate::projects::snapshot;
use crate::settings::Settings;

/// The longest the apply waits for the database objects to go before it deletes the Namespace.
const DATABASE_WAIT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_secs(1);

/// The database objects a project can hold, one read per operator.
const DATABASES: [&zeabur_modules::Read; 5] = [
    &cloudnative_pg::CLUSTERS,
    &percona_mongodb::CLUSTERS,
    &moco::CLUSTERS,
    &mariadb_operator::SERVERS,
    &dragonfly::STORES,
];

/// Zeabur's own namespaces, which are never deleted as a project: its own, its members', its
/// terminals'.
pub(super) fn zeabur_namespaces(settings: &Settings) -> Vec<String> {
    let mut out: Vec<String> = settings.namespace.iter().cloned().collect();
    out.push(settings.identity.namespace.clone());
    out.push(settings.terminal.namespace.clone());
    out
}

/// The plan that deletes `project`.
pub(super) async fn compute(
    planner: &Planner<'_>,
    cluster: &dyn Cluster,
    project: &str,
    request: &PlanRequest,
    now: &str,
    read_at: Timestamp,
    zeabur_namespaces: Vec<String>,
) -> Result<Plan, ApiError> {
    let namespaces = ResourceRef::new("", "v1", "namespaces", "Namespace");
    let what = format!("namespace {project}");
    let namespace = cluster
        .get_object(&namespaces, None, project)
        .await
        .map_err(|e| read_problem(e, &what, get_read("", "namespaces", None, project)))?
        .map(serde_json::to_value)
        .transpose()
        .map_err(|e| ApiError::from(ClusterError::Decode(e.to_string()).into_problem(&what)))?;
    let objects = cluster
        .namespace_objects(project)
        .await
        .map_err(|e| read_problem(e, &format!("project {project}"), project_read(project)))?;
    let (snapshot, unread_objects) = snapshot(objects, read_at);
    let mut unreadable = Vec::new();
    let services_unread = unread_objects.iter().any(|u| u.resource == "services");
    unreadable.extend(
        unread_objects
            .iter()
            .filter(|u| u.resource == "services")
            .map(|u| refused_list(u, Some(project))),
    );
    let databases = databases(cluster, project, &mut unreadable).await?;
    let (class_reclaim, default_class) = classes(cluster).await?;
    let reclaim_policies = volume_reclaim(cluster, &snapshot).await?;
    let tools_namespace = planner.settings.namespace.clone();
    let tools_pods = tools_pods(planner, cluster, project)
        .await
        .iter()
        .filter_map(|p| p.pointer("/metadata/name").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    let backups = backups(cluster, project, read_at, &mut unreadable).await?;
    let deletion = DeletionInput {
        zeabur_namespaces,
        databases,
        class_reclaim,
        default_class,
        tools_namespace,
        tools_pods,
        backups,
        services_unread,
        ..DeletionInput::default()
    };
    let mut plan = zeabur_plan::plan(
        Input {
            namespace: namespace.as_ref(),
            deletion: Some(&deletion),
            reclaim_policies: &reclaim_policies,
            ..Input::new(&snapshot, now)
        },
        request,
    )
    .map_err(plan_problem)?;
    let mut wanted = vec![Denied {
        verb: "delete".to_owned(),
        group: String::new(),
        resource: "namespaces".to_owned(),
        name: Some(project.to_owned()),
        namespace: None,
    }];
    wanted.extend(deletion.databases.iter().map(|d| {
        Denied {
            verb: "delete".to_owned(),
            group: d.resource.group.clone(),
            resource: d.resource.resource.clone(),
            name: d
                .object
                .pointer("/metadata/name")
                .and_then(Value::as_str)
                .map(str::to_owned),
            namespace: Some(project.to_owned()),
        }
    }));
    let mut missing = terminal::denied(cluster, wanted).await;
    missing.extend(unreadable);
    if !missing.is_empty() {
        plan.warnings
            .push(Warning::RightsMissing { rights: missing });
    }
    Ok(plan)
}

/// The plan that removes the finalizers holding `project`'s Namespace.
pub(super) async fn compute_force(
    cluster: &dyn Cluster,
    project: &str,
    request: &PlanRequest,
    now: &str,
    zeabur_namespaces: Vec<String>,
) -> Result<Plan, ApiError> {
    let namespaces = ResourceRef::new("", "v1", "namespaces", "Namespace");
    let what = format!("namespace {project}");
    let Some(found) = cluster
        .get_object(&namespaces, None, project)
        .await
        .map_err(|e| read_problem(e, &what, get_read("", "namespaces", None, project)))?
    else {
        return Err(ApiError::from(Problem::new(
            Kind::NotFound,
            format!("There is no namespace named {project}."),
        )));
    };
    let objects = held_objects(cluster, project, &found).await;
    let mut held = Vec::new();
    let mut wanted = Vec::new();
    for (resource, object) in objects {
        let name = object.metadata.name.clone();
        let mut object = serde_json::to_value(object).map_err(|e| {
            ApiError::from(ClusterError::Decode(e.to_string()).into_problem(&resource.kind))
        })?;
        if let Some(map) = object.as_object_mut() {
            map.entry("kind")
                .or_insert_with(|| resource.kind.clone().into());
        }
        wanted.push(Denied {
            verb: "patch".to_owned(),
            group: resource.group.clone(),
            resource: resource.resource.clone(),
            name,
            namespace: Some(project.to_owned()),
        });
        held.push(ProjectObject {
            resource: zeabur_plan::ResourceRef {
                group: resource.group,
                version: resource.version,
                resource: resource.resource,
            },
            object,
        });
    }
    let namespace = serde_json::to_value(found)
        .map_err(|e| ApiError::from(ClusterError::Decode(e.to_string()).into_problem(&what)))?;
    let deletion = DeletionInput {
        zeabur_namespaces,
        held,
        ..DeletionInput::default()
    };
    let snapshot = zeabur_graph::Snapshot::default();
    let mut plan = zeabur_plan::plan(
        Input {
            namespace: Some(&namespace),
            deletion: Some(&deletion),
            ..Input::new(&snapshot, now)
        },
        request,
    )
    .map_err(plan_problem)?;
    let missing = terminal::denied(cluster, wanted).await;
    if !missing.is_empty() {
        plan.warnings
            .push(Warning::RightsMissing { rights: missing });
    }
    Ok(plan)
}

/// The project's database objects. A kind whose list is
/// refused is left out, and its `list` right named in `unreadable`.
async fn databases(
    cluster: &dyn Cluster,
    project: &str,
    unreadable: &mut Vec<Denied>,
) -> Result<Vec<ProjectObject>, ApiError> {
    let mut out = Vec::new();
    for read in DATABASES {
        let objects = match crate::cluster::modules::read_in(cluster, read, Some(project))
            .await
            .map_err(|e| ApiError::from(e.into_problem(&format!("databases of {project}"))))?
        {
            Reading::Objects(objects) => objects,
            Reading::NotInstalled => continue,
            Reading::Refused(_) => {
                unreadable.push(Denied {
                    verb: "list".to_owned(),
                    group: read.group.to_owned(),
                    resource: read.resource.to_owned(),
                    name: None,
                    namespace: Some(project.to_owned()),
                });
                continue;
            }
        };
        if objects.is_empty() {
            continue;
        }
        for object in objects {
            let mut object = serde_json::to_value(object).map_err(|e| {
                ApiError::from(ClusterError::Decode(e.to_string()).into_problem(read.kind))
            })?;
            if let Some(map) = object.as_object_mut() {
                map.entry("kind").or_insert_with(|| read.kind.into());
            }
            out.push(ProjectObject {
                resource: zeabur_plan::ResourceRef {
                    group: read.group.to_owned(),
                    version: read.version.to_owned(),
                    resource: read.resource.to_owned(),
                },
                object,
            });
        }
    }
    Ok(out)
}

/// The project's backups that exist (a run that stored one, [`stored`]): a database's, once per
/// store it may be in (none when no store could be read, so an unread store never counts as kept
/// outside); a volume's K8up backups, once per endpoint of each K8up schedule that covers it (a
/// run does not name its schedule); the project's own Velero backups. A refused read leaves its
/// backups out, and its right is named in `unreadable`.
async fn backups(
    cluster: &dyn Cluster,
    project: &str,
    now: Timestamp,
    unreadable: &mut Vec<Denied>,
) -> Result<Vec<ProjectBackup>, ApiError> {
    let mut out = Vec::new();
    if let Some(databases) = allowed(
        crate::projects::databases::read(cluster, &[], project, now).await,
        unreadable,
    )? {
        unreadable.extend(
            databases
                .unreadable
                .iter()
                .map(|u| refused_list(u, Some(project))),
        );
        for database in databases.items {
            if !database.backups.runs.iter().any(stored) {
                continue;
            }
            let stores: Vec<BackupStore> = if database.backups.endpoints.is_empty() {
                vec![BackupStore::Unknown]
            } else {
                database
                    .backups
                    .endpoints
                    .into_iter()
                    .map(|e| e.map_or(BackupStore::Aws, BackupStore::Endpoint))
                    .collect()
            };
            out.extend(stores.into_iter().map(|store| ProjectBackup {
                kind: BackupKind::Database,
                name: database.name.clone(),
                store,
            }));
        }
    }
    let Some(found) = allowed(
        crate::projects::backups::read(cluster, &[], project).await,
        unreadable,
    )?
    else {
        return Ok(out);
    };
    unreadable.extend(found.unreadable.iter().map(|u| {
        let velero = u.resource.ends_with(".velero.io");
        refused_list(u, (!velero).then_some(project))
    }));
    let run_of = |provider: &str, target: &backups::Target| {
        found
            .runs
            .iter()
            .chain(found.coverage.iter().filter_map(|c| c.last_run.as_ref()))
            .any(|r| r.provider == provider && r.targets.contains(target) && stored(r))
    };
    for schedule in found
        .schedules
        .iter()
        .filter(|s| s.provider == k8up::MODULE.id)
    {
        for target in &schedule.targets {
            let backup = ProjectBackup {
                kind: BackupKind::Volume,
                name: target.name.clone(),
                store: schedule
                    .endpoint
                    .clone()
                    .map_or(BackupStore::Unknown, BackupStore::Endpoint),
            };
            if target.kind == backups::TargetKind::Volume
                && run_of(k8up::MODULE.id, target)
                && !out.contains(&backup)
            {
                out.push(backup);
            }
        }
    }
    let namespace = backups::Target {
        kind: backups::TargetKind::Namespace,
        name: project.to_owned(),
    };
    if run_of(velero::MODULE.id, &namespace) {
        out.push(ProjectBackup {
            kind: BackupKind::Project,
            name: project.to_owned(),
            store: BackupStore::Unknown,
        });
    }
    Ok(out)
}

/// Whether a run left a backup in its store: it succeeded, or some items failed.
const fn stored(run: &backups::Run) -> bool {
    matches!(
        run.outcome,
        backups::Outcome::Succeeded | backups::Outcome::PartiallyFailed
    )
}

/// The `list` right a partial read was refused, from its resource
/// (`objectstores.barmancloud.cnpg.io`), in `namespace` or cluster-wide.
fn refused_list(unread: &crate::views::UnreadableView, namespace: Option<&str>) -> Denied {
    let (resource, group) = unread
        .resource
        .split_once('.')
        .unwrap_or((unread.resource.as_str(), ""));
    Denied {
        verb: "list".to_owned(),
        group: group.to_owned(),
        resource: resource.to_owned(),
        name: None,
        namespace: namespace.map(str::to_owned),
    }
}

/// What a read found, or nothing when it was refused, its right named in `unreadable`.
fn allowed<T>(
    result: Result<T, ApiError>,
    unreadable: &mut Vec<Denied>,
) -> Result<Option<T>, ApiError> {
    match result {
        Ok(found) => Ok(Some(found)),
        Err(ApiError(problem)) if problem.kind == Kind::Forbidden => {
            unreadable.extend(problem.denied);
            Ok(None)
        }
        Err(other) => Err(other),
    }
}

/// The `reclaimPolicy` of each `StorageClass` (`Delete` when it sets none, as kube-apiserver
/// defaults it), and the default class; nothing when the person may not list them.
async fn classes(
    cluster: &dyn Cluster,
) -> Result<(BTreeMap<String, String>, Option<String>), ApiError> {
    let classes = match cluster.list::<StorageClass>(None).await {
        Ok(classes) => classes,
        Err(error) if error.is_refused() => return Ok((BTreeMap::new(), None)),
        Err(error) => return Err(ApiError::from(error.into_problem("the StorageClasses"))),
    };
    let default = classes
        .iter()
        .find(|c| crate::projects::storage::is_default(c))
        .and_then(|c| c.metadata.name.clone());
    let reclaim = classes
        .iter()
        .filter_map(|c| {
            c.metadata.name.clone().map(|name| {
                let policy = c.reclaim_policy.clone().unwrap_or_else(|| "Delete".into());
                (name, policy)
            })
        })
        .collect();
    Ok((reclaim, default))
}

/// The `persistentVolumeReclaimPolicy` of each bound PersistentVolume the person may read.
async fn volume_reclaim(
    cluster: &dyn Cluster,
    snapshot: &zeabur_graph::Snapshot,
) -> Result<BTreeMap<String, String>, ApiError> {
    let resource = ResourceRef::new("", "v1", "persistentvolumes", "PersistentVolume");
    let mut out = BTreeMap::new();
    for volume in snapshot
        .persistent_volume_claims
        .iter()
        .filter_map(|c| c.spec.as_ref().and_then(|s| s.volume_name.clone()))
    {
        match cluster.get_object(&resource, None, &volume).await {
            Ok(Some(found)) => {
                if let Some(policy) = found
                    .data
                    .pointer("/spec/persistentVolumeReclaimPolicy")
                    .and_then(Value::as_str)
                {
                    out.insert(volume, policy.to_owned());
                }
            }
            Ok(None) => {}
            Err(error) if error.is_refused() || error.is_not_found() => {}
            Err(other) => {
                return Err(ApiError::from(
                    other.into_problem(&format!("PersistentVolume {volume}")),
                ));
            }
        }
    }
    Ok(out)
}

/// The person's tools pods for `project`, read with Zeabur's service account, and only when the
/// person may open a terminal in the project; none otherwise (they stop at their deadline).
async fn tools_pods(planner: &Planner<'_>, cluster: &dyn Cluster, project: &str) -> Vec<Value> {
    let Some(zeabur) = planner.zeabur else {
        return Vec::new();
    };
    if crate::tools::Scope::Project(project.to_owned())
        .allowed(cluster)
        .await
        .is_err()
    {
        return Vec::new();
    }
    let key = plans::person_key(planner.person);
    terminal::list_pods(zeabur, &planner.settings.namespace)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|p| plans::is_tools_pod_of(p, &key, ToolsScope::Project(project)))
        .collect()
}

/// Carries out a project's deletion plan, in the order the module says.
pub(super) async fn apply(
    planner: &Planner<'_>,
    cluster: &dyn Cluster,
    project: &str,
    plan: &Plan,
) -> Result<Vec<Applied>, ApiError> {
    let writes: Vec<(zeabur_plan::Target, WriteRequest)> = plan
        .objects
        .iter()
        .map(|o| {
            (
                o.target.clone(),
                write_request(project, &o.target, &o.write),
            )
        })
        .collect();
    for (target, write) in &writes {
        let dry = WriteRequest {
            dry_run: true,
            ..write.clone()
        };
        if let Err(error) = cluster.write(&dry).await {
            let problem = refused(cluster, project, target, write, error).await;
            return Err(ApiError::from(refusal(problem, plan, target, &[])));
        }
    }
    let (databases, namespace): (Vec<_>, Vec<_>) = writes
        .into_iter()
        .partition(|(target, _)| target.namespaced());
    let mut applied = Vec::new();
    for (target, write) in &databases {
        write_one(cluster, project, plan, target, write, &mut applied).await?;
    }
    let pending: Vec<&(zeabur_plan::Target, WriteRequest)> = databases.iter().collect();
    wait_gone(cluster, project, &pending).await;
    for (target, write) in &namespace {
        write_one(cluster, project, plan, target, write, &mut applied).await?;
    }
    delete_tools_pods(planner, cluster, project).await;
    Ok(applied)
}

async fn write_one(
    cluster: &dyn Cluster,
    project: &str,
    plan: &Plan,
    target: &zeabur_plan::Target,
    write: &WriteRequest,
    applied: &mut Vec<Applied>,
) -> Result<(), ApiError> {
    match cluster.write(write).await {
        Ok(written) => {
            applied.push(Applied {
                target: target.clone(),
                resource_version: written.resource_version,
            });
            Ok(())
        }
        Err(error)
            if error.is_not_found() && matches!(write.body, zeabur_kube::WriteBody::Delete(_)) =>
        {
            Ok(())
        }
        Err(error) => {
            let problem = refused(cluster, project, target, write, error).await;
            Err(ApiError::from(refusal(problem, plan, target, applied)))
        }
    }
}

/// Waits until each of `deleted` is gone, or [`DATABASE_WAIT`] has passed.
async fn wait_gone(
    cluster: &dyn Cluster,
    project: &str,
    deleted: &[&(zeabur_plan::Target, WriteRequest)],
) {
    let deadline = tokio::time::Instant::now() + DATABASE_WAIT;
    loop {
        let mut left = false;
        for (target, write) in deleted {
            let resource = write.target.resource();
            if !matches!(
                cluster
                    .find_object(&resource, Some(project), &target.name)
                    .await,
                Ok(None)
            ) {
                left = true;
                break;
            }
        }
        if !left || tokio::time::Instant::now() >= deadline {
            return;
        }
        tokio::time::sleep(POLL).await;
    }
}

/// Deletes the person's tools pods for `project`; a pod that cannot be deleted stops at its
/// deadline.
async fn delete_tools_pods(planner: &Planner<'_>, cluster: &dyn Cluster, project: &str) {
    let Some(zeabur) = planner.zeabur else {
        return;
    };
    let namespace = planner.settings.namespace.as_str();
    for pod in tools_pods(planner, cluster, project).await {
        let name = pod
            .pointer("/metadata/name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let write = write_request(
            namespace,
            &zeabur_plan::Target::of::<Pod>(name),
            &Write::Delete {
                uid: pod
                    .pointer("/metadata/uid")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                resource_version: pod
                    .pointer("/metadata/resourceVersion")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                foreground: false,
            },
        );
        if let Err(error) = zeabur.write(&write).await {
            tracing::warn!(%error, pod = name, "a tools pod of a deleted project stays");
        }
    }
}
