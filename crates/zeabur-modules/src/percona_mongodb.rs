//! Percona Operator for MongoDB: MongoDB replica sets from `PerconaServerMongoDB` objects, and
//! their Percona Backup for MongoDB (PBM) backups [psmdb-cr].
//!
//! # As a database
//!
//! A `PerconaServerMongoDB` runs one StatefulSet per replica set, `<cluster>-<replset>`, behind
//! the Service of the same name on port 27017 [psmdb-expose]. Zeabur shows the first replica set
//! (its forms make one, `rs0`, without sharding). `status.state` is the operator's word for the
//! cluster (`initializing`, `ready`, `error`, `stopping`, `paused`), and
//! `status.replsets.<name>.ready` counts the members ready [psmdb-cr]. Each member's data is in the
//! claim `mongod-data-<cluster>-<replset>-<n>`, from the StatefulSet's claim template. The
//! operator's system users and their passwords are in the Secret `spec.secrets.users`
//! (`<cluster>-secrets` by default); applications connect as the database admin
//! (`MONGODB_DATABASE_ADMIN_USER`, `…_PASSWORD`) [psmdb-users].
//!
//! # Backups
//!
//! The backup agent runs next to each member once `spec.backup.enabled` is true. Where backups go
//! is a storage of `spec.backup.storages`; scheduled backups are `spec.backup.tasks`, each with a
//! five-field cron schedule and a retention count; an on-demand backup is a
//! `PerconaServerMongoDBBackup` naming the cluster and the storage, whose `status.state` is
//! `waiting`, `requested`, `running`, `ready`, `error` or `rejected` [psmdb-backups].
//!
//! [psmdb-cr]: https://docs.percona.com/percona-operator-for-mongodb/operator.html
//! [psmdb-expose]: https://docs.percona.com/percona-operator-for-mongodb/expose.html
//! [psmdb-users]: https://docs.percona.com/percona-operator-for-mongodb/users.html
//! [psmdb-backups]: https://docs.percona.com/percona-operator-for-mongodb/backups.html

use serde_json::Value;
use zeabur_kube::DynamicObject;

use crate::backups::{Outcome, Run, Schedule, Target, TargetKind, newest};
use crate::databases::{self, Backups, Connection, Database, Engine, State, Store};
use crate::{Module, Read, text};

/// The module.
pub static MODULE: Module = Module {
    id: "percona-server-mongodb",
    capability: "mongodb",
    reads: &[CLUSTERS, BACKUPS],
    http: &[],
};

/// `psmdb.percona.com/v1` `PerconaServerMongoDB`, in the project's namespace.
pub const CLUSTERS: Read = Read {
    group: "psmdb.percona.com",
    version: "v1",
    resource: "perconaservermongodbs",
    kind: "PerconaServerMongoDB",
};

/// `psmdb.percona.com/v1` `PerconaServerMongoDBBackup`, in the project's namespace.
pub const BACKUPS: Read = Read {
    group: "psmdb.percona.com",
    version: "v1",
    resource: "perconaservermongodbbackups",
    kind: "PerconaServerMongoDBBackup",
};

/// The storage name Zeabur gives a database's backup target.
pub const STORAGE: &str = "backups";

fn state(word: Option<&str>, members: i64, ready: i64) -> State {
    match word {
        Some("ready") => databases::by_members(members, ready),
        Some("error") => State::Failed,
        Some("paused" | "stopping") => State::Paused,
        Some("initializing") => State::Progressing,
        Some(_) | None => State::Unknown,
    }
}

fn outcome(word: Option<&str>) -> Outcome {
    match word {
        Some("ready") => Outcome::Succeeded,
        Some("error" | "rejected") => Outcome::Failed,
        Some("waiting" | "requested" | "running") => Outcome::Running,
        Some(_) | None => Outcome::Unknown,
    }
}

fn first_replset(object: &DynamicObject) -> Option<&Value> {
    object.data.pointer("/spec/replsets/0")
}

fn s3_store(name: &str, storage: &Value, enabled: bool, retention: Option<String>) -> Store {
    let s3 = |key: &str| {
        storage
            .pointer(&format!("/s3/{key}"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let destination = s3("bucket").map(|bucket| {
        s3("prefix").map_or_else(
            || format!("s3://{bucket}"),
            |prefix| format!("s3://{bucket}/{prefix}"),
        )
    });
    Store {
        name: name.to_owned(),
        destination,
        endpoint: s3("endpointUrl"),
        secret: s3("credentialsSecret"),
        retention,
        in_use: enabled,
    }
}

/// The cluster's backups: where they go, the scheduled tasks, and the newest twenty runs.
fn backups_of(
    namespace: &str,
    cluster: &DynamicObject,
    name: &str,
    backups: &[DynamicObject],
) -> Backups {
    let enabled = cluster
        .data
        .pointer("/spec/backup/enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let tasks: Vec<&Value> = cluster
        .data
        .pointer("/spec/backup/tasks")
        .and_then(Value::as_array)
        .map_or_else(Vec::new, |t| t.iter().collect());
    let target = Target {
        kind: TargetKind::Database,
        name: name.to_owned(),
    };
    let mut schedules: Vec<Schedule> = tasks
        .iter()
        .filter_map(|task| {
            let cron = task["schedule"].as_str()?;
            Some(Schedule {
                name: format!(
                    "{namespace}/{name}/{}",
                    task["name"].as_str().unwrap_or_default()
                ),
                provider: MODULE.id.to_owned(),
                cron: cron.to_owned(),
                paused: !enabled || task["enabled"] == false,
                targets: vec![target.clone()],
                note: None,
            })
        })
        .collect();
    schedules.sort_by(|a, b| a.name.cmp(&b.name));
    let storages = cluster
        .data
        .pointer("/spec/backup/storages")
        .and_then(Value::as_object);
    let store = storages.and_then(|all| {
        let (storage_name, storage) = all.get_key_value(STORAGE).or_else(|| all.iter().next())?;
        let retention = tasks
            .iter()
            .find(|t| t["storageName"].as_str() == Some(storage_name))
            .and_then(|t| {
                t.pointer("/retention/count")
                    .or_else(|| t.get("keep"))
                    .and_then(Value::as_i64)
            })
            .map(|count| count.to_string());
        Some(s3_store(storage_name, storage, enabled, retention))
    });
    let mut runs: Vec<Run> = backups
        .iter()
        .filter(|b| text(b, "/spec/clusterName").as_deref() == Some(name))
        .map(|b| Run {
            name: b.metadata.name.clone().unwrap_or_default(),
            provider: MODULE.id.to_owned(),
            targets: vec![target.clone()],
            outcome: outcome(text(b, "/status/state").as_deref()),
            started: text(b, "/status/start").or_else(|| {
                b.metadata
                    .creation_timestamp
                    .as_ref()
                    .map(|t| t.0.to_string())
            }),
            finished: text(b, "/status/completed"),
            message: text(b, "/status/error"),
        })
        .collect();
    newest(&mut runs, 20);
    Backups {
        store,
        schedules,
        runs,
    }
}

/// One `PerconaServerMongoDB` as a database, with its backups. Runs are the newest twenty.
#[must_use]
pub fn database(namespace: &str, cluster: &DynamicObject, backups: &[DynamicObject]) -> Database {
    let name = cluster.metadata.name.clone().unwrap_or_default();
    let replset = first_replset(cluster);
    let rs = replset
        .and_then(|r| r["name"].as_str())
        .unwrap_or("rs0")
        .to_owned();
    let members = replset.and_then(|r| r["size"].as_i64()).unwrap_or(3);
    let ready = databases::int(cluster, &format!("/status/replsets/{rs}/ready")).unwrap_or(0);
    let word = text(cluster, "/status/state");
    let image = text(cluster, "/spec/image");
    let version = text(cluster, "/status/mongoVersion")
        .or_else(|| image.as_deref().and_then(databases::tag).map(str::to_owned));
    let secret = text(cluster, "/spec/secrets/users").unwrap_or_else(|| format!("{name}-secrets"));
    let claims = (0..members.max(0))
        .map(|i| format!("mongod-data-{name}-{rs}-{i}"))
        .collect();
    Database {
        origin: zeabur_model::Origin::complete(
            "psmdb.percona.com/v1",
            "PerconaServerMongoDB",
            Some(namespace),
            &name,
        ),
        engine: Engine::Mongodb,
        provider: MODULE.id.to_owned(),
        api_version: "psmdb.percona.com/v1".to_owned(),
        kind: "PerconaServerMongoDB".to_owned(),
        resource: "perconaservermongodbs".to_owned(),
        state: state(word.as_deref(), members, ready),
        phase: word,
        message: text(cluster, "/status/message"),
        members,
        ready,
        primary: None,
        version,
        image,
        storage: replset
            .and_then(|r| r.pointer("/volumeSpec/persistentVolumeClaim/resources/requests/storage"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        claims,
        resources: replset.and_then(|r| r.get("resources")).cloned(),
        connection: Connection {
            host: format!("{name}-{rs}.{namespace}.svc"),
            port: 27017,
            database: Some("admin".to_owned()),
            username: None,
            secret,
            username_key: Some("MONGODB_DATABASE_ADMIN_USER".to_owned()),
            password_key: "MONGODB_DATABASE_ADMIN_PASSWORD".to_owned(),
            options: Some(format!("replicaSet={rs}")),
        },
        backups: backups_of(namespace, cluster, &name, backups),
        created: cluster
            .metadata
            .creation_timestamp
            .as_ref()
            .map(|t| t.0.to_string()),
        object: databases::written(cluster),
        name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(value: Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn a_single_member_replica_set_with_backups_to_s3() {
        let cluster = object(serde_json::json!({
            "apiVersion": "psmdb.percona.com/v1", "kind": "PerconaServerMongoDB",
            "metadata": {"name": "mongo", "namespace": "shop"},
            "spec": {
                "image": "percona/percona-server-mongodb:8.0.32-14",
                "unsafeFlags": {"replsetSize": true},
                "replsets": [{"name": "rs0", "size": 1, "volumeSpec": {"persistentVolumeClaim": {
                    "resources": {"requests": {"storage": "2Gi"}}}}}],
                "backup": {
                    "enabled": true,
                    "storages": {"backups": {"type": "s3", "s3": {
                        "bucket": "b", "prefix": "shop", "endpointUrl": "http://s3:8333",
                        "credentialsSecret": "mongo-backups-s3"}}},
                    "tasks": [{"name": "daily", "enabled": true, "schedule": "0 3 * * *",
                               "storageName": "backups",
                               "retention": {"type": "count", "count": 7, "deleteFromStorage": true}}]
                }
            },
            "status": {"state": "ready", "mongoVersion": "8.0.32-14",
                       "replsets": {"rs0": {"ready": 1, "size": 1, "status": "ready"}}}
        }));
        let backup = object(serde_json::json!({
            "apiVersion": "psmdb.percona.com/v1", "kind": "PerconaServerMongoDBBackup",
            "metadata": {"name": "now-1", "namespace": "shop"},
            "spec": {"clusterName": "mongo", "storageName": "backups"},
            "status": {"state": "error", "error": "access denied", "start": "2026-10-20T00:00:00Z"}
        }));
        let db = database("shop", &cluster, &[backup]);
        assert_eq!(db.state, State::Ready);
        assert_eq!((db.members, db.ready), (1, 1));
        assert_eq!(db.claims, ["mongod-data-mongo-rs0-0"]);
        assert_eq!(db.connection.host, "mongo-rs0.shop.svc");
        assert_eq!(db.connection.secret, "mongo-secrets");
        let store = db.backups.store.unwrap();
        assert_eq!(store.destination.as_deref(), Some("s3://b/shop"));
        assert_eq!(store.retention.as_deref(), Some("7"));
        assert!(store.in_use);
        assert_eq!(db.backups.schedules[0].cron, "0 3 * * *");
        assert!(!db.backups.schedules[0].paused);
        assert_eq!(db.backups.runs[0].outcome, Outcome::Failed);
        assert_eq!(db.backups.runs[0].message.as_deref(), Some("access denied"));
    }

    #[test]
    fn schedules_pause_while_backups_are_off() {
        let cluster = object(serde_json::json!({
            "apiVersion": "psmdb.percona.com/v1", "kind": "PerconaServerMongoDB",
            "metadata": {"name": "mongo", "namespace": "shop"},
            "spec": {"image": "x:1", "replsets": [{"name": "rs0", "size": 3}],
                     "backup": {"enabled": false, "tasks": [{"name": "d", "schedule": "0 3 * * *"}]}},
            "status": {"state": "initializing"}
        }));
        let db = database("shop", &cluster, &[]);
        assert_eq!(db.state, State::Progressing);
        assert!(db.backups.schedules[0].paused);
        assert!(db.backups.store.is_none());
    }
}
