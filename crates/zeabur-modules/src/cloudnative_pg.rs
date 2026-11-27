//! CloudNativePG: PostgreSQL base backups the operator takes for each `Cluster`, from its
//! `ScheduledBackup` and `Backup` objects [cnpg-api].
//!
//! A `ScheduledBackup` names its cluster (`spec.cluster.name`) and a six-field cron expression
//! whose first field is seconds, unlike a Kubernetes CronJob's [cnpg-api]; `spec.suspend` pauses
//! it. A `Backup` names its cluster too, and reports `status.phase` [cnpg-phases] with
//! `startedAt`, `stoppedAt` and `error`.
//!
//! CloudNativePG's pods belong to its `Cluster`, not to a Deployment or StatefulSet, so a database
//! is a target of its own here, named after the `Cluster`.
//!
//! # As a database
//!
//! [`database`] reads a `Cluster` as the shared database view: `status.phase` in CloudNativePG's
//! words [cnpg-cluster-phases], `instances` and `readyInstances`, `currentPrimary`, the image, and
//! the application user and database the `initdb` bootstrap made (`app` by default), whose
//! credentials are in the Secret `<cluster>-app` [cnpg-app-secret]. Applications connect through
//! the read-write Service `<cluster>-rw` on port 5432 [cnpg-services]. Each instance's
//! PersistentVolumeClaim is named after the instance (`status.instanceNames`).
//!
//! The kstatus generic rule over the `Cluster`'s conditions decides its state [kstatus-generic],
//! `Ready` among them [cnpg-cluster-phases]; the phase and `phaseReason` are only shown. A
//! hibernated `Cluster` has the condition `cnpg.io/hibernation` `True` [cnpg-hibernation].
//!
//! Backups go through the Barman Cloud plugin: the `Cluster` names the plugin
//! (`barman-cloud.cloudnative-pg.io`) with `isWALArchiver: true` and the `ObjectStore` to use
//! (`parameters.barmanObjectName`), and `ScheduledBackup`s and `Backup`s ask for
//! `method: plugin` [barman-cloud-usage]. The operator's in-tree `barmanObjectStore` is
//! deprecated and removed in 1.32, so Zeabur only writes the plugin's form.
//!
//! # Recovery window
//!
//! A new database can start from any time between the end of the oldest backup in the store and
//! the newest write-ahead log segment there [barman-cloud-api]. The plugin reports the first as
//! the `ObjectStore`'s `status.serverRecoveryWindow.<server>.firstRecoverabilityPoint`, refreshed
//! after each backup and each catalog check. It does not report the second, and the `Cluster`'s
//! own recoverability fields are not set for backup plugins [cnpg-api]. While the `Cluster`'s
//! `ContinuousArchiving` condition is true, PostgreSQL switches to a new segment at least every
//! `archive_timeout` (5 minutes unless the `Cluster` sets it) [cnpg-config], so the archive holds
//! every write older than that. Zeabur offers times up to that lag before now, and none when
//! archiving fails. The new `Cluster` names the time as `recoveryTarget.targetTime` in RFC 3339,
//! and CloudNativePG picks the newest backup that ended before it [cnpg-recovery].
//!
//! [cnpg-api]: https://cloudnative-pg.io/docs/devel/cloudnative-pg.v1
//! [cnpg-phases]: https://github.com/cloudnative-pg/cloudnative-pg/blob/main/api/v1/backup_types.go
//! [cnpg-cluster-phases]: https://github.com/cloudnative-pg/cloudnative-pg/blob/v1.30.1/api/v1/cluster_types.go
//! [cnpg-app-secret]: https://cloudnative-pg.io/documentation/current/applications/#secrets
//! [cnpg-services]: https://cloudnative-pg.io/documentation/current/service_management/
//! [barman-cloud-usage]: https://cloudnative-pg.io/plugin-barman-cloud/docs/usage/
//! [barman-cloud-api]: https://github.com/cloudnative-pg/plugin-barman-cloud/blob/v0.15.1/api/v1/objectstore_types.go
//! [cnpg-config]: https://github.com/cloudnative-pg/cloudnative-pg/blob/v1.30.1/pkg/postgres/configuration.go
//! [cnpg-recovery]: https://cloudnative-pg.io/documentation/current/recovery/
//! [cnpg-hibernation]: https://github.com/cloudnative-pg/cloudnative-pg/blob/v1.30.1/docs/src/declarative_hibernation.md
//! [kstatus-generic]: https://github.com/kubernetes-sigs/cli-utils/blob/v0.37.2/pkg/kstatus/status/generic.go

use k8s_openapi::jiff::Timestamp;
use zeabur_kube::DynamicObject;
use zeabur_model::readiness::{self, Reason, Status};

use crate::backups::{Findings, Outcome, Run, Schedule, Target, TargetKind, newest};
use crate::databases::{
    self, AppUser, ArchiveFailure, Backups, Connection, Database, Engine, RecoveryWindow, State,
    Store,
};
use crate::{Module, Read, strings, text, time};

/// The module.
pub static MODULE: Module = Module {
    id: "cloudnative-pg",
    component: Some("cloudnative-pg"),
    capability: "postgres",
    reads: &[CLUSTERS, SCHEDULED_BACKUPS, BACKUPS],
    http: &[],
};

/// The Barman Cloud plugin's module: the `ObjectStore`s that say where a `Cluster`'s backups go.
pub static BARMAN_CLOUD: Module = Module {
    id: "barman-cloud",
    component: Some("cloudnative-pg"),
    capability: "postgres",
    reads: &[OBJECT_STORES],
    http: &[],
};

/// `barmancloud.cnpg.io/v1` `ObjectStore`, in the project's namespace.
pub const OBJECT_STORES: Read = Read {
    group: "barmancloud.cnpg.io",
    version: "v1",
    resource: "objectstores",
    kind: "ObjectStore",
};

/// The Barman Cloud plugin's name in a `Cluster`'s `spec.plugins`.
pub const BARMAN_CLOUD_PLUGIN: &str = "barman-cloud.cloudnative-pg.io";

/// `postgresql.cnpg.io/v1` `Cluster`, in the project's namespace.
pub const CLUSTERS: Read = Read {
    group: "postgresql.cnpg.io",
    version: "v1",
    resource: "clusters",
    kind: "Cluster",
};

/// `postgresql.cnpg.io/v1` `ScheduledBackup`, in the project's namespace.
pub const SCHEDULED_BACKUPS: Read = Read {
    group: "postgresql.cnpg.io",
    version: "v1",
    resource: "scheduledbackups",
    kind: "ScheduledBackup",
};

/// `postgresql.cnpg.io/v1` `Backup`, in the project's namespace.
pub const BACKUPS: Read = Read {
    group: "postgresql.cnpg.io",
    version: "v1",
    resource: "backups",
    kind: "Backup",
};

fn target_of(object: &DynamicObject) -> Option<Target> {
    text(object, "/spec/cluster/name").map(|name| Target {
        kind: TargetKind::Database,
        name,
    })
}

/// The databases (one per `Cluster`), sorted by name.
#[must_use]
pub fn databases(clusters: &[DynamicObject]) -> Vec<Target> {
    let mut out: Vec<Target> = clusters
        .iter()
        .filter_map(|c| c.metadata.name.clone())
        .map(|name| Target {
            kind: TargetKind::Database,
            name,
        })
        .collect();
    out.sort();
    out
}

const HIBERNATION: &str = "cnpg.io/hibernation";

/// Whether CloudNativePG reports a healthy, fully ready Cluster for an image change.
#[must_use]
pub fn ready_for_image_update(cluster: &DynamicObject) -> bool {
    let instances = databases::int(cluster, "/spec/instances").unwrap_or(1);
    let ready = databases::int(cluster, "/status/readyInstances").unwrap_or(0);
    instances > 0 && instances == ready && state(cluster, instances, ready) == State::Ready
}

/// The state of a `Cluster` by the kstatus generic rule and its ready instances. A hibernated
/// `Cluster` is paused, and one with a phase but no `Ready` condition yet is progressing.
fn state(cluster: &DynamicObject, members: i64, ready: i64) -> State {
    let status = databases::generic_status(cluster);
    if status
        .conditions
        .iter()
        .any(|c| c.type_ == HIBERNATION && c.status == "True")
    {
        return State::Paused;
    }
    let readiness = readiness::generic(&cluster.metadata, &status).readiness;
    match readiness.status {
        Status::Failed => State::Failed,
        Status::InProgress => State::Progressing,
        Status::Current if readiness.reason == Reason::Ready => {
            databases::by_members(members, ready)
        }
        Status::Current if cluster.data.pointer("/status/phase").is_some() => State::Progressing,
        Status::Current => State::Unknown,
    }
}

/// The Barman Cloud plugin's entry of this `Cluster`'s `spec.plugins`, when it is enabled.
fn plugin(cluster: &DynamicObject) -> Option<&serde_json::Value> {
    cluster
        .data
        .pointer("/spec/plugins")?
        .as_array()?
        .iter()
        .find(|p| p["name"] == BARMAN_CLOUD_PLUGIN && p["enabled"] != false)
}

/// The `ObjectStore` this `Cluster` archives to through the Barman Cloud plugin.
fn plugin_store(cluster: &DynamicObject) -> Option<String> {
    plugin(cluster)
        .and_then(|p| p.pointer("/parameters/barmanObjectName"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

/// A PostgreSQL time setting such as `5min` or `300` (seconds) in seconds.
fn seconds(setting: &str) -> Option<u64> {
    let setting = setting.trim();
    let split = setting
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(setting.len());
    let (number, unit) = setting.split_at(split);
    let number: u64 = number.parse().ok()?;
    let factor = match unit.trim() {
        "" | "s" => 1,
        "min" => 60,
        "h" => 3600,
        "d" => 86_400,
        "ms" => return Some(number.div_ceil(1000)),
        _ => return None,
    };
    number.checked_mul(factor)
}

/// The times a new database can start from: see the module's "Recovery window".
fn recovery_window(
    cluster: &DynamicObject,
    store: Option<&DynamicObject>,
) -> Option<RecoveryWindow> {
    true_since(cluster, "ContinuousArchiving")?;
    let first = server_window(cluster, store?)?
        .get("firstRecoverabilityPoint")?
        .as_str()?
        .parse()
        .ok()?;
    let lag_seconds = text(cluster, "/spec/postgresql/parameters/archive_timeout")
        .map_or(Some(300), |t| seconds(&t))
        .filter(|s| *s > 0)?;
    Some(RecoveryWindow {
        earliest_at: first,
        lag_seconds,
    })
}

/// The `ObjectStore`'s recovery window of the `Cluster`'s server: the plugin's `serverName`, else
/// the `Cluster`'s name.
fn server_window<'a>(
    cluster: &DynamicObject,
    store: &'a DynamicObject,
) -> Option<&'a serde_json::Value> {
    let server = plugin(cluster)
        .and_then(|p| p.pointer("/parameters/serverName"))
        .and_then(serde_json::Value::as_str)
        .or(cluster.metadata.name.as_deref())?;
    store
        .data
        .pointer("/status/serverRecoveryWindow")?
        .get(server)
}

/// The store holds a backup of the `Cluster`: its recovery window has a first point or a last
/// successful backup [barman-cloud-api].
fn store_holds_backup(cluster: &DynamicObject, store: &DynamicObject) -> bool {
    server_window(cluster, store).is_some_and(|w| {
        ["firstRecoverabilityPoint", "lastSuccessfulBackupTime"]
            .iter()
            .any(|key| w.get(key).is_some_and(|t| !t.is_null()))
    })
}

/// When the condition `kind` last turned true, if it is true.
fn true_since(cluster: &DynamicObject, kind: &str) -> Option<Timestamp> {
    cluster
        .data
        .pointer("/status/conditions")?
        .as_array()?
        .iter()
        .find(|c| c["type"] == kind && c["status"] == "True")
        .and_then(|c| c["lastTransitionTime"].as_str())
        .and_then(|t| t.parse().ok())
}

/// Archiving fails: `ContinuousArchiving` is `False`, with the operator's message and the
/// destination the `Cluster` archives to. The message is the archiver's exit status; the S3 error
/// itself is only in the plugin's log, which no Kubernetes object carries.
fn archive_failure(cluster: &DynamicObject, store: Option<&Store>) -> Option<ArchiveFailure> {
    let failing = cluster
        .data
        .pointer("/status/conditions")?
        .as_array()?
        .iter()
        .find(|c| c["type"] == "ContinuousArchiving" && c["status"] == "False")?;
    Some(ArchiveFailure {
        message: databases::condition_message(cluster, "ContinuousArchiving"),
        destination: store.and_then(|s| s.destination.clone()),
        since: failing["lastTransitionTime"]
            .as_str()
            .and_then(|t| t.parse().ok()),
    })
}

/// A backup can start: the instances run with the plugin's sidecar. `ContinuousArchiving` is
/// true even without an archiver, so the cluster must also have become ready again after the
/// store was made (the operator restarts the instances to add the sidecar), or have finished a
/// backup since.
fn backups_ready(cluster: &DynamicObject, store_created: Option<Timestamp>, runs: &[Run]) -> bool {
    let Some(created) = store_created else {
        return true_since(cluster, "ContinuousArchiving").is_some();
    };
    true_since(cluster, "ContinuousArchiving").is_some()
        && (true_since(cluster, "Ready").is_some_and(|t| t > created)
            || runs
                .iter()
                .any(|r| r.outcome == Outcome::Succeeded && r.time().is_some_and(|t| t > created)))
}

fn store(object: &DynamicObject, in_use: bool) -> Store {
    Store {
        name: object.metadata.name.clone().unwrap_or_default(),
        destination: text(object, "/spec/configuration/destinationPath"),
        endpoint: text(object, "/spec/configuration/endpointURL"),
        secret: text(
            object,
            "/spec/configuration/s3Credentials/secretAccessKey/name",
        ),
        region: None,
        retention: text(object, "/spec/retentionPolicy"),
        in_use,
    }
}

/// The name Zeabur gives the `ObjectStore` of a database's backups.
#[must_use]
pub fn store_name(cluster: &str) -> String {
    format!("{cluster}-backups")
}

/// Only the Secret CloudNativePG makes has `uri` and the other keys a service reads.
fn app_user(name: &str, own: Option<&str>, username: &str, database: &str) -> Option<AppUser> {
    own.is_none().then(|| AppUser {
        username: username.to_owned(),
        database: Some(database.to_owned()),
        secret: format!("{name}-app"),
        password_key: "password".to_owned(),
    })
}

/// One `Cluster` as a database, with its backups from the namespace's `ObjectStore`s,
/// `ScheduledBackup`s and `Backup`s. Runs are the newest twenty.
///
/// The store is the `ObjectStore` the `Cluster` names; else the one Zeabur names for it
/// ([`store_name`]), not yet in use, so a half-applied setup shows as such.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "one object's fields, read top to bottom"
)]
pub fn database(
    namespace: &str,
    cluster: &DynamicObject,
    stores: &[DynamicObject],
    scheduled: &[DynamicObject],
    backups: &[DynamicObject],
) -> Database {
    let name = cluster.metadata.name.clone().unwrap_or_default();
    let members = databases::int(cluster, "/spec/instances").unwrap_or(1);
    let ready = databases::int(cluster, "/status/readyInstances").unwrap_or(0);
    let phase = text(cluster, "/status/phase");
    let image = text(cluster, "/status/image").or_else(|| text(cluster, "/spec/imageName"));
    let version = image
        .as_deref()
        .and_then(databases::tag)
        .and_then(|t| t.split('-').next())
        .filter(|v| v.starts_with(|c: char| c.is_ascii_digit()))
        .map(str::to_owned);
    let database =
        text(cluster, "/spec/bootstrap/initdb/database").unwrap_or_else(|| "app".to_owned());
    let username =
        text(cluster, "/spec/bootstrap/initdb/owner").unwrap_or_else(|| database.clone());
    let own_secret = text(cluster, "/spec/bootstrap/initdb/secret/name");
    let app = app_user(&name, own_secret.as_deref(), &username, &database);
    let secret = own_secret.unwrap_or_else(|| format!("{name}-app"));
    let service = text(cluster, "/status/writeService").unwrap_or_else(|| format!("{name}-rw"));
    let named = |wanted: &str| {
        stores
            .iter()
            .find(|s| s.metadata.name.as_deref() == Some(wanted))
    };
    let store = plugin_store(cluster).map_or_else(
        || named(&store_name(&name)).map(|s| store(s, false)),
        |wanted| {
            Some(named(&wanted).map_or_else(
                || Store {
                    name: wanted.clone(),
                    destination: None,
                    endpoint: None,
                    secret: None,
                    region: None,
                    retention: None,
                    in_use: true,
                },
                |s| store(s, true),
            ))
        },
    );
    let mine = |o: &&DynamicObject| text(o, "/spec/cluster/name").as_deref() == Some(&name);
    let scheduled: Vec<DynamicObject> = scheduled.iter().filter(mine).cloned().collect();
    let backups: Vec<DynamicObject> = backups.iter().filter(mine).cloned().collect();
    let mut found = all_findings(namespace, &scheduled, &backups);
    let in_use = plugin_store(cluster).and_then(|wanted| named(&wanted));
    let has_backup = found.runs.iter().any(Run::stored)
        || in_use.is_some_and(|s| store_holds_backup(cluster, s));
    newest(&mut found.runs, 20);
    let state = state(cluster, members, ready);
    let store_created = in_use
        .and_then(|s| s.metadata.creation_timestamp.as_ref())
        .map(|t| t.0);
    let recovery_window = recovery_window(cluster, in_use);
    let archive_failure = archive_failure(cluster, store.as_ref());
    Database {
        origin: zeabur_model::Origin::complete(
            "postgresql.cnpg.io/v1",
            "Cluster",
            Some(namespace),
            &name,
        ),
        engine: Engine::Postgresql,
        provider: MODULE.id.to_owned(),
        api_version: format!("{}/{}", CLUSTERS.group, CLUSTERS.version),
        api_group: CLUSTERS.group.to_owned(),
        api_group_version: CLUSTERS.version.to_owned(),
        kind: CLUSTERS.kind.to_owned(),
        resource: CLUSTERS.resource.to_owned(),
        state,
        message: text(cluster, "/status/phaseReason"),
        phase,
        members,
        ready,
        primary: text(cluster, "/status/currentPrimary"),
        version,
        image,
        storage: text(cluster, "/spec/storage/size"),
        storage_class: text(cluster, "/spec/storage/storageClass"),
        claims: strings(cluster, "/status/instanceNames"),
        resources: cluster.data.pointer("/spec/resources").cloned(),
        connection: Connection {
            host: format!("{service}.{namespace}.svc"),
            port: 5432,
            database: Some(database),
            username: Some(username),
            secret,
            username_key: Some("username".to_owned()),
            password_key: "password".to_owned(),
            options: None,
        },
        app,
        backups: Backups {
            ready: state == State::Ready
                && store.as_ref().is_some_and(|s| s.in_use)
                && backups_ready(cluster, store_created, &found.runs),
            endpoints: store
                .iter()
                .filter(|s| s.destination.is_some())
                .map(|s| s.endpoint.clone())
                .collect(),
            store,
            schedules: found.schedules,
            runs: found.runs,
            stored: has_backup,
            recovery_window,
            archive_failure,
        },
        created_at: cluster.metadata.creation_timestamp.as_ref().map(|t| t.0),
        object: databases::written(cluster),
        pods: Vec::new(),
        restored: databases::restored(cluster),
        image_updates: zeabur_model::database_image_updates(
            &cluster.metadata,
            cluster.data.get("spec").unwrap_or(&serde_json::Value::Null),
        ),
        unmanaged: false,
        name,
    }
}

/// What CloudNativePG's objects of one namespace say about its databases. Runs are the newest
/// ten.
#[must_use]
pub fn findings(
    namespace: &str,
    scheduled: &[DynamicObject],
    backups: &[DynamicObject],
) -> Findings {
    let mut out = all_findings(namespace, scheduled, backups);
    newest(&mut out.runs, 10);
    out
}

/// Every schedule and run; the runs unsorted.
fn all_findings(
    namespace: &str,
    scheduled: &[DynamicObject],
    backups: &[DynamicObject],
) -> Findings {
    let mut out = Findings::default();
    for schedule in scheduled {
        let (Some(cron), Some(target)) = (text(schedule, "/spec/schedule"), target_of(schedule))
        else {
            continue;
        };
        out.schedules.push(Schedule {
            name: format!(
                "{namespace}/{}",
                schedule.metadata.name.as_deref().unwrap_or_default()
            ),
            provider: MODULE.id.to_owned(),
            cron,
            paused: schedule
                .data
                .pointer("/spec/suspend")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            targets: vec![target],
            note: None,
            ..Schedule::default()
        });
    }
    out.schedules.sort_by(|a, b| a.name.cmp(&b.name));
    for backup in backups {
        let Some(target) = target_of(backup) else {
            continue;
        };
        let outcome = match text(backup, "/status/phase").as_deref() {
            Some("completed") => Outcome::Succeeded,
            Some("failed" | "walArchivingFailing" | "invalid backup definition") => Outcome::Failed,
            Some("pending" | "started" | "running" | "finalizing") => Outcome::Running,
            _ => Outcome::Unknown,
        };
        out.runs.push(Run {
            name: backup.metadata.name.clone().unwrap_or_default(),
            provider: MODULE.id.to_owned(),
            targets: vec![target],
            outcome,
            started_at: time(backup, "/status/startedAt"),
            finished_at: time(backup, "/status/stoppedAt"),
            message: text(backup, "/status/error"),
            restore_from: (outcome == Outcome::Succeeded)
                .then(|| text(backup, "/status/backupId"))
                .flatten(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(value: serde_json::Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn its_instances_are_members_and_its_jobs_and_other_clusters_are_not() {
        let pg = database(
            "shop",
            &object(serde_json::json!({
                "apiVersion": "postgresql.cnpg.io/v1", "kind": "Cluster",
                "metadata": {"name": "pg", "namespace": "shop"}, "spec": {"instances": 1}
            })),
            &[],
            &[],
            &[],
        );
        let labels = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect()
        };
        assert!(pg.is_member(&labels(&[
            ("cnpg.io/cluster", "pg"),
            ("cnpg.io/instanceName", "pg-1")
        ])));
        assert!(!pg.is_member(&labels(&[
            ("cnpg.io/cluster", "pg"),
            ("cnpg.io/jobRole", "initdb")
        ])));
        assert!(!pg.is_member(&labels(&[
            ("cnpg.io/cluster", "pg2"),
            ("cnpg.io/instanceName", "pg2-1")
        ])));
    }

    #[test]
    fn a_suspended_schedule_and_a_completed_backup() {
        let schedule = object(serde_json::json!({
            "apiVersion": "postgresql.cnpg.io/v1", "kind": "ScheduledBackup",
            "metadata": {"name": "daily", "namespace": "shop"},
            "spec": {"schedule": "0 0 2 * * *", "cluster": {"name": "pg"}, "suspend": true}
        }));
        let backup = object(serde_json::json!({
            "apiVersion": "postgresql.cnpg.io/v1", "kind": "Backup",
            "metadata": {"name": "daily-1", "namespace": "shop"},
            "spec": {"cluster": {"name": "pg"}},
            "status": {"phase": "completed", "startedAt": "2026-10-14T02:00:00Z", "stoppedAt": "2026-10-14T02:05:00Z"}
        }));
        let found = findings("shop", &[schedule], &[backup]);
        assert!(found.schedules[0].paused);
        assert_eq!(found.schedules[0].targets[0].name, "pg");
        assert_eq!(found.runs[0].outcome, Outcome::Succeeded);
    }

    #[test]
    fn a_healthy_cluster_with_its_plugin_store_and_its_own_backups() {
        let pg = object(serde_json::json!({
            "apiVersion": "postgresql.cnpg.io/v1", "kind": "Cluster",
            "metadata": {"name": "pg", "namespace": "shop"},
            "spec": {
                "instances": 2,
                "storage": {"size": "5Gi", "storageClass": "longhorn"},
                "plugins": [{"name": "barman-cloud.cloudnative-pg.io", "isWALArchiver": true,
                             "parameters": {"barmanObjectName": "pg-backups"}}]
            },
            "status": {
                "phase": "Cluster in healthy state", "readyInstances": 2, "currentPrimary": "pg-1",
                "image": "ghcr.io/cloudnative-pg/postgresql:18-standard-trixie",
                "instanceNames": ["pg-1", "pg-2"],
                "conditions": [
                    {"type": "ContinuousArchiving", "status": "True", "lastTransitionTime": "2026-10-22T16:02:32Z"},
                    {"type": "Ready", "status": "True", "lastTransitionTime": "2026-10-22T16:02:41Z"}
                ]
            },
        }));
        let store = object(serde_json::json!({
            "apiVersion": "barmancloud.cnpg.io/v1", "kind": "ObjectStore",
            "metadata": {"name": "pg-backups", "namespace": "shop",
                         "creationTimestamp": "2026-10-22T16:02:24Z"},
            "spec": {"retentionPolicy": "30d", "configuration": {
                "destinationPath": "s3://bucket/shop", "endpointURL": "http://s3:8333",
                "s3Credentials": {"secretAccessKey": {"name": "pg-backups-s3", "key": "secret"}}}}
        }));
        let other = object(serde_json::json!({
            "apiVersion": "postgresql.cnpg.io/v1", "kind": "Backup",
            "metadata": {"name": "other-1", "namespace": "shop"},
            "spec": {"cluster": {"name": "other"}}, "status": {"phase": "completed"}
        }));
        let mine = object(serde_json::json!({
            "apiVersion": "postgresql.cnpg.io/v1", "kind": "Backup",
            "metadata": {"name": "pg-1", "namespace": "shop"},
            "spec": {"cluster": {"name": "pg"}}, "status": {"phase": "running"}
        }));
        let db = database("shop", &pg, &[store], &[], &[other, mine]);
        assert_eq!(db.state, State::Ready);
        assert_eq!(db.version.as_deref(), Some("18"));
        assert!(db.object.get("status").is_none());
        assert_eq!(db.object["spec"]["instances"], 2);
        assert_eq!(db.primary.as_deref(), Some("pg-1"));
        assert_eq!(db.claims, ["pg-1", "pg-2"]);
        assert_eq!(db.storage_class.as_deref(), Some("longhorn"));
        assert_eq!(db.connection.host, "pg-rw.shop.svc");
        assert_eq!(db.connection.secret, "pg-app");
        assert_eq!(db.connection.username.as_deref(), Some("app"));
        assert_eq!(
            db.app
                .as_ref()
                .map(|a| (a.secret.as_str(), a.database.as_deref())),
            Some(("pg-app", Some("app")))
        );
        let store = db.backups.store.unwrap();
        assert!(store.in_use);
        assert_eq!(store.retention.as_deref(), Some("30d"));
        assert_eq!(store.secret.as_deref(), Some("pg-backups-s3"));
        assert_eq!(db.backups.runs.len(), 1);
        assert_eq!(db.backups.runs[0].outcome, Outcome::Running);
        assert!(db.backups.ready);
        assert_eq!(db.backups.endpoints, [Some("http://s3:8333".to_owned())]);

        // An ObjectStore that could not be read: where the backups are is not known.
        let unread = database("shop", &pg, &[], &[], &[]);
        assert_eq!(unread.backups.endpoints, []);

        // Before the operator restarts the instances, archiving reads true without an archiver.
        let mut before = pg;
        before.data["status"]["conditions"][1]["lastTransitionTime"] =
            serde_json::json!("2026-10-22T16:02:02Z");
        let store = object(serde_json::json!({
            "apiVersion": "barmancloud.cnpg.io/v1", "kind": "ObjectStore",
            "metadata": {"name": "pg-backups", "namespace": "shop",
                         "creationTimestamp": "2026-10-22T16:02:24Z"}
        }));
        let db = database("shop", &before, std::slice::from_ref(&store), &[], &[]);
        assert!(!db.backups.ready);
        let done = object(serde_json::json!({
            "apiVersion": "postgresql.cnpg.io/v1", "kind": "Backup",
            "metadata": {"name": "pg-2", "namespace": "shop"},
            "spec": {"cluster": {"name": "pg"}},
            "status": {"phase": "completed", "stoppedAt": "2026-10-22T17:00:00Z"}
        }));
        assert!(
            database("shop", &before, &[store], &[], &[done])
                .backups
                .ready
        );
    }

    #[test]
    fn the_recovery_window_starts_at_the_stores_first_point_and_needs_working_archiving() {
        let cluster = |parameters: serde_json::Value, archiving: &str| {
            object(serde_json::json!({
                "apiVersion": "postgresql.cnpg.io/v1", "kind": "Cluster",
                "metadata": {"name": "pg", "namespace": "shop"},
                "spec": {"postgresql": {"parameters": parameters},
                         "plugins": [{"name": "barman-cloud.cloudnative-pg.io", "isWALArchiver": true,
                                      "parameters": {"barmanObjectName": "pg-backups",
                                                     "serverName": "pg-v2"}}]},
                "status": {"phase": "Cluster in healthy state", "readyInstances": 1,
                           "conditions": [{"type": "ContinuousArchiving", "status": archiving,
                                           "lastTransitionTime": "2026-10-22T16:02:32Z"}]}
            }))
        };
        let store = object(serde_json::json!({
            "apiVersion": "barmancloud.cnpg.io/v1", "kind": "ObjectStore",
            "metadata": {"name": "pg-backups", "namespace": "shop"},
            "status": {"serverRecoveryWindow": {
                "pg": {"firstRecoverabilityPoint": "2026-10-01T03:00:00Z"},
                "pg-v2": {"firstRecoverabilityPoint": "2026-10-20T03:05:00Z",
                          "lastSuccessfulBackupTime": "2026-10-22T03:05:00Z"}}}
        }));
        let window = |pg: &DynamicObject| {
            database("shop", pg, std::slice::from_ref(&store), &[], &[])
                .backups
                .recovery_window
                .map(|w| (w.earliest_at, w.lag_seconds))
        };
        let first: Timestamp = "2026-10-20T03:05:00Z".parse().unwrap();
        let cases = [
            (
                "default archive_timeout",
                serde_json::json!({}),
                "True",
                Some((first, 300)),
            ),
            (
                "its own archive_timeout",
                serde_json::json!({"archive_timeout": "1h"}),
                "True",
                Some((first, 3600)),
            ),
            (
                "archive_timeout in seconds",
                serde_json::json!({"archive_timeout": "90"}),
                "True",
                Some((first, 90)),
            ),
            (
                "no archive_timeout",
                serde_json::json!({"archive_timeout": "0"}),
                "True",
                None,
            ),
            ("archiving fails", serde_json::json!({}), "False", None),
        ];
        for (case, parameters, archiving, want) in cases {
            assert_eq!(window(&cluster(parameters, archiving)), want, "{case}");
        }
    }

    #[test]
    fn failing_archiving_carries_the_operators_message_and_is_not_starting() {
        let cluster = |condition: serde_json::Value| {
            object(serde_json::json!({
                "apiVersion": "postgresql.cnpg.io/v1", "kind": "Cluster",
                "metadata": {"name": "pg", "namespace": "shop"},
                "spec": {"plugins": [{"name": "barman-cloud.cloudnative-pg.io", "isWALArchiver": true,
                                      "parameters": {"barmanObjectName": "pg-backups"}}]},
                "status": {"phase": "Cluster in healthy state", "readyInstances": 1,
                           "conditions": [condition,
                                          {"type": "Ready", "status": "True",
                                           "lastTransitionTime": "2026-10-22T16:02:41Z"}]}
            }))
        };
        let store = object(serde_json::json!({
            "apiVersion": "barmancloud.cnpg.io/v1", "kind": "ObjectStore",
            "metadata": {"name": "pg-backups", "namespace": "shop"},
            "spec": {"configuration": {"destinationPath": "s3://bucket/shop"}},
        }));
        let backups = |condition| {
            database(
                "shop",
                &cluster(condition),
                std::slice::from_ref(&store),
                &[],
                &[],
            )
            .backups
        };

        let message = "unexpected failure invoking barman-cloud-wal-archive: exit status 4";
        let failing = backups(serde_json::json!({
            "type": "ContinuousArchiving", "status": "False", "message": message,
            "lastTransitionTime": "2026-10-22T16:02:32Z"
        }));
        assert!(!failing.ready);
        assert_eq!(
            failing.archive_failure,
            Some(ArchiveFailure {
                message: Some(message.to_owned()),
                destination: Some("s3://bucket/shop".to_owned()),
                since: Some("2026-10-22T16:02:32Z".parse().unwrap()),
            })
        );

        let working = backups(serde_json::json!({
            "type": "ContinuousArchiving", "status": "True",
            "lastTransitionTime": "2026-10-22T16:02:32Z"
        }));
        assert!(working.ready);
        assert_eq!(working.archive_failure, None);

        let unreported = backups(serde_json::json!({"type": "Other", "status": "True"}));
        assert!(!unreported.ready);
        assert_eq!(unreported.archive_failure, None);
    }

    #[test]
    fn conditions_and_ready_instances_decide_the_state() {
        let cluster = |status: serde_json::Value| {
            object(serde_json::json!({
                "apiVersion": "postgresql.cnpg.io/v1", "kind": "Cluster",
                "metadata": {"name": "pg", "namespace": "shop"},
                "spec": {"instances": 3},
                "status": status
            }))
        };
        let ready = |status: &str| serde_json::json!({"type": "Ready", "status": status});
        let healthy = cluster(serde_json::json!({
            "phase": "Cluster in healthy state", "conditions": [ready("True")]
        }));
        assert_eq!(state(&healthy, 3, 2), State::Degraded);
        let unrecoverable = cluster(serde_json::json!({
            "phase": "Cluster is unrecoverable and needs manual intervention",
            "conditions": [ready("False")]
        }));
        assert_eq!(state(&unrecoverable, 3, 0), State::Progressing);
        let hibernated = cluster(serde_json::json!({
            "phase": "Cluster in healthy state",
            "conditions": [ready("False"), {"type": "cnpg.io/hibernation", "status": "True"}]
        }));
        assert_eq!(state(&hibernated, 3, 0), State::Paused);
        let setting_up = cluster(serde_json::json!({"phase": "Setting up primary"}));
        assert_eq!(state(&setting_up, 3, 0), State::Progressing);
        assert_eq!(state(&cluster(serde_json::json!({})), 3, 0), State::Unknown);
    }

    #[test]
    fn a_store_not_yet_named_by_the_cluster_is_shown_not_in_use() {
        let pg = object(serde_json::json!({
            "apiVersion": "postgresql.cnpg.io/v1", "kind": "Cluster",
            "metadata": {"name": "pg", "namespace": "shop"},
            "spec": {"instances": 1}
        }));
        let store = object(serde_json::json!({
            "apiVersion": "barmancloud.cnpg.io/v1", "kind": "ObjectStore",
            "metadata": {"name": "pg-backups", "namespace": "shop"},
            "spec": {"configuration": {"destinationPath": "s3://b/p"}}
        }));
        let db = database("shop", &pg, &[store], &[], &[]);
        assert_eq!(db.state, State::Unknown);
        assert!(!db.backups.ready);
        assert!(!db.backups.store.unwrap().in_use);
    }

    #[test]
    fn a_disabled_plugin_entry_leaves_its_store_not_in_use() {
        let pg = object(serde_json::json!({
            "apiVersion": "postgresql.cnpg.io/v1", "kind": "Cluster",
            "metadata": {"name": "pg", "namespace": "shop"},
            "spec": {"instances": 1, "plugins": [{
                "name": "barman-cloud.cloudnative-pg.io", "enabled": false, "isWALArchiver": true,
                "parameters": {"barmanObjectName": "pg-backups", "serverName": "pg-v2"}}]}
        }));
        let store = object(serde_json::json!({
            "apiVersion": "barmancloud.cnpg.io/v1", "kind": "ObjectStore",
            "metadata": {"name": "pg-backups", "namespace": "shop"},
            "spec": {"configuration": {"destinationPath": "s3://b/p"}}
        }));
        let db = database("shop", &pg, &[store], &[], &[]);
        let store = db.backups.store.unwrap();
        assert_eq!(store.name, "pg-backups");
        assert!(!store.in_use);
    }

    #[test]
    fn a_backup_older_than_the_runs_shown_or_only_in_the_store_is_still_stored() {
        let pg = object(serde_json::json!({
            "apiVersion": "postgresql.cnpg.io/v1", "kind": "Cluster",
            "metadata": {"name": "pg", "namespace": "shop"},
            "spec": {"plugins": [{"name": "barman-cloud.cloudnative-pg.io", "isWALArchiver": true,
                                  "parameters": {"barmanObjectName": "pg-backups"}}]}
        }));
        let run = |n: u32, phase: &str| {
            object(serde_json::json!({
                "apiVersion": "postgresql.cnpg.io/v1", "kind": "Backup",
                "metadata": {"name": format!("pg-{n}"), "namespace": "shop"},
                "spec": {"cluster": {"name": "pg"}},
                "status": {"phase": phase,
                           "stoppedAt": format!("2026-10-{:02}T03:00:00Z", n)}
            }))
        };
        let mut runs: Vec<DynamicObject> = (2..=25).map(|n| run(n, "failed")).collect();
        runs.push(run(1, "completed"));
        let store = |status: serde_json::Value| {
            object(serde_json::json!({
                "apiVersion": "barmancloud.cnpg.io/v1", "kind": "ObjectStore",
                "metadata": {"name": "pg-backups", "namespace": "shop"},
                "status": status
            }))
        };
        let empty = store(serde_json::json!({}));
        let db = database("shop", &pg, std::slice::from_ref(&empty), &[], &runs);
        assert_eq!(db.backups.runs.len(), 20);
        assert!(db.backups.runs.iter().all(|r| r.outcome == Outcome::Failed));
        assert!(db.backups.stored);

        let failed: Vec<DynamicObject> = (2..=25).map(|n| run(n, "failed")).collect();
        assert!(
            !database("shop", &pg, std::slice::from_ref(&empty), &[], &failed)
                .backups
                .stored
        );
        let holding = store(serde_json::json!({"serverRecoveryWindow": {
            "pg": {"lastSuccessfulBackupTime": "2026-09-01T03:00:00Z"}}}));
        assert!(database("shop", &pg, &[holding], &[], &[]).backups.stored);
        let other_server = store(serde_json::json!({"serverRecoveryWindow": {
            "pg-old": {"firstRecoverabilityPoint": "2026-09-01T03:00:00Z"}}}));
        assert!(
            !database("shop", &pg, &[other_server], &[], &[])
                .backups
                .stored
        );
    }
}
