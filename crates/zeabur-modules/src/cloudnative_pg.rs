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

use zeabur_kube::DynamicObject;

use crate::backups::{Findings, Outcome, Run, Schedule, Target, TargetKind, newest};
use crate::databases::{
    self, AppUser, Backups, Connection, Database, Engine, RecoveryWindow, State, Store,
};
use crate::{Module, Read, strings, text};

/// The module.
pub static MODULE: Module = Module {
    id: "cloudnative-pg",
    capability: "postgres",
    reads: &[CLUSTERS, SCHEDULED_BACKUPS, BACKUPS],
    http: &[],
};

/// The Barman Cloud plugin's module: the `ObjectStore`s that say where a `Cluster`'s backups go.
pub static BARMAN_CLOUD: Module = Module {
    id: "barman-cloud",
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

/// The state of a `Cluster` from its phase [cnpg-cluster-phases] and ready instances.
fn state(phase: Option<&str>, members: i64, ready: i64) -> State {
    let Some(phase) = phase else {
        return State::Unknown;
    };
    let lower = phase.to_ascii_lowercase();
    if lower == "cluster in healthy state" {
        databases::by_members(members, ready)
    } else if lower.contains("hibernat") {
        State::Paused
    } else if lower.contains("unrecoverable")
        || lower.contains("cannot")
        || lower.starts_with("unable")
        || lower.contains("failed")
    {
        State::Failed
    } else {
        State::Progressing
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
    let server = plugin(cluster)
        .and_then(|p| p.pointer("/parameters/serverName"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .or_else(|| cluster.metadata.name.clone())?;
    let first = store?
        .data
        .pointer("/status/serverRecoveryWindow")?
        .get(&server)?
        .get("firstRecoverabilityPoint")?
        .as_str()?
        .to_owned();
    let lag_seconds = text(cluster, "/spec/postgresql/parameters/archive_timeout")
        .map_or(Some(300), |t| seconds(&t))
        .filter(|s| *s > 0)?;
    Some(RecoveryWindow { first, lag_seconds })
}

/// When the condition `kind` last turned true, if it is true.
fn true_since(cluster: &DynamicObject, kind: &str) -> Option<String> {
    cluster
        .data
        .pointer("/status/conditions")?
        .as_array()?
        .iter()
        .find(|c| c["type"] == kind && c["status"] == "True")
        .and_then(|c| c["lastTransitionTime"].as_str())
        .map(str::to_owned)
}

/// A backup can start: the instances run with the plugin's sidecar. `ContinuousArchiving` is
/// true even without an archiver, so the cluster must also have become ready again after the
/// store was made (the operator restarts the instances to add the sidecar), or have finished a
/// backup since. Times are the API's second-precision RFC 3339, which sort as text.
fn backups_ready(cluster: &DynamicObject, store_created: Option<&str>, runs: &[Run]) -> bool {
    let Some(created) = store_created else {
        return true_since(cluster, "ContinuousArchiving").is_some();
    };
    true_since(cluster, "ContinuousArchiving").is_some()
        && (true_since(cluster, "Ready").is_some_and(|t| t.as_str() > created)
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
    let found = findings_limited(namespace, &scheduled, &backups, 20);
    let state = state(phase.as_deref(), members, ready);
    let in_use = plugin_store(cluster).and_then(|wanted| named(&wanted));
    let store_created = in_use
        .and_then(|s| s.metadata.creation_timestamp.as_ref())
        .map(|t| t.0.to_string());
    let recovery_window = recovery_window(cluster, in_use);
    Database {
        origin: zeabur_model::Origin::complete(
            "postgresql.cnpg.io/v1",
            "Cluster",
            Some(namespace),
            &name,
        ),
        engine: Engine::Postgresql,
        provider: MODULE.id.to_owned(),
        api_version: "postgresql.cnpg.io/v1".to_owned(),
        kind: "Cluster".to_owned(),
        resource: "clusters".to_owned(),
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
                && backups_ready(cluster, store_created.as_deref(), &found.runs),
            store,
            schedules: found.schedules,
            runs: found.runs,
            recovery_window,
        },
        created: cluster
            .metadata
            .creation_timestamp
            .as_ref()
            .map(|t| t.0.to_string()),
        object: databases::written(cluster),
        pods: Vec::new(),
        restored: databases::restored(cluster),
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
    findings_limited(namespace, scheduled, backups, 10)
}

fn findings_limited(
    namespace: &str,
    scheduled: &[DynamicObject],
    backups: &[DynamicObject],
    limit: usize,
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
            started: text(backup, "/status/startedAt"),
            finished: text(backup, "/status/stoppedAt"),
            message: text(backup, "/status/error"),
            restore_from: (outcome == Outcome::Succeeded)
                .then(|| text(backup, "/status/backupId"))
                .flatten(),
        });
    }
    newest(&mut out.runs, limit);
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
                .map(|w| (w.first, w.lag_seconds))
        };
        let first = "2026-10-20T03:05:00Z".to_owned();
        let cases = [
            (
                "default archive_timeout",
                serde_json::json!({}),
                "True",
                Some((first.clone(), 300)),
            ),
            (
                "its own archive_timeout",
                serde_json::json!({"archive_timeout": "1h"}),
                "True",
                Some((first.clone(), 3600)),
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
    fn phases_and_ready_instances_decide_the_state() {
        assert_eq!(
            state(Some("Cluster in healthy state"), 3, 2),
            State::Degraded
        );
        assert_eq!(state(Some("Setting up primary"), 1, 0), State::Progressing);
        assert_eq!(
            state(
                Some("Cluster is unrecoverable and needs manual intervention"),
                1,
                0
            ),
            State::Failed
        );
        assert_eq!(state(None, 1, 0), State::Unknown);
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
}
