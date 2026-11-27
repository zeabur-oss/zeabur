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
//! operator's system users and their passwords are in the Secret `spec.secrets.users`, which
//! the operator defaults to `percona-server-mongodb-users`, one name for every cluster of a
//! namespace (`psmdb_defaults.go`, v1.23.1); Zeabur's form names `<cluster>-secrets`. Applications
//! connect as the database admin (`MONGODB_DATABASE_ADMIN_USER`, `…_PASSWORD`) [psmdb-users].
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

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::PersistentVolumeClaimSpec;
use serde::Deserialize;
use serde_json::Value;
use zeabur_kube::DynamicObject;

use crate::backups::{Outcome, Run, Schedule, Target, TargetKind, newest};
use crate::databases::{self, AppUser, Backups, Connection, Database, Engine, State, Store};
use crate::{Module, Read, filled, filled_time, read_as};

/// The module.
pub static MODULE: Module = Module {
    id: "percona-server-mongodb",
    component: Some("percona-mongodb"),
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

/// The fields of a `PerconaServerMongoDB` that Zeabur reads [psmdb-cr].
#[derive(Default, Deserialize)]
#[serde(default)]
struct ClusterView {
    spec: ClusterSpec,
    status: ClusterStatus,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct ClusterSpec {
    image: Option<String>,
    replsets: Vec<Replset>,
    secrets: Secrets,
    backup: BackupSpec,
    users: Vec<User>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Replset {
    name: Option<String>,
    size: Option<i64>,
    volume_spec: Option<VolumeSpec>,
    resources: Option<Value>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct VolumeSpec {
    persistent_volume_claim: Option<PersistentVolumeClaimSpec>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Secrets {
    users: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct BackupSpec {
    enabled: bool,
    tasks: Vec<Task>,
    storages: BTreeMap<String, Storage>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Task {
    name: Option<String>,
    schedule: Option<String>,
    enabled: Option<bool>,
    storage_name: Option<String>,
    retention: Option<Retention>,
    keep: Option<i64>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Retention {
    count: Option<i64>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Storage {
    s3: S3,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct S3 {
    bucket: Option<String>,
    prefix: Option<String>,
    endpoint_url: Option<String>,
    credentials_secret: Option<String>,
    region: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct User {
    name: Option<String>,
    db: Option<String>,
    password_secret_ref: Option<PasswordRef>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct PasswordRef {
    name: Option<String>,
    key: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ClusterStatus {
    state: Option<String>,
    message: Option<String>,
    mongo_version: Option<String>,
    observed_generation: Option<i64>,
    replsets: BTreeMap<String, ReplsetStatus>,
    conditions: Vec<Condition>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct ReplsetStatus {
    ready: Option<i64>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Condition {
    #[serde(rename = "type")]
    type_: String,
    status: String,
}

/// The fields of a `PerconaServerMongoDBBackup` that Zeabur reads [psmdb-backups].
#[derive(Default, Deserialize)]
#[serde(default)]
struct BackupView {
    spec: BackupObjectSpec,
    status: BackupStatus,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct BackupObjectSpec {
    cluster_name: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct BackupStatus {
    state: Option<String>,
    start: Option<String>,
    completed: Option<String>,
    error: Option<String>,
    destination: Option<String>,
}

fn s3_store(name: &str, storage: &Storage, enabled: bool, retention: Option<String>) -> Store {
    let s3 = &storage.s3;
    let destination = filled(s3.bucket.as_ref()).map(|bucket| {
        filled(s3.prefix.as_ref()).map_or_else(
            || format!("s3://{bucket}"),
            |prefix| format!("s3://{bucket}/{prefix}"),
        )
    });
    Store {
        name: name.to_owned(),
        destination,
        endpoint: filled(s3.endpoint_url.as_ref()),
        secret: filled(s3.credentials_secret.as_ref()),
        region: filled(s3.region.as_ref()),
        retention,
        in_use: enabled,
    }
}

/// The cluster's backups: where they go, the scheduled tasks, and the newest twenty runs.
fn backups_of(
    namespace: &str,
    cluster: &DynamicObject,
    view: &ClusterView,
    name: &str,
    backups: &[DynamicObject],
) -> Backups {
    let backup = &view.spec.backup;
    let enabled = backup.enabled;
    let target = Target {
        kind: TargetKind::Database,
        name: name.to_owned(),
    };
    let mut schedules: Vec<Schedule> = backup
        .tasks
        .iter()
        .filter_map(|task| {
            let cron = task.schedule.as_ref()?;
            Some(Schedule {
                name: format!(
                    "{namespace}/{name}/{}",
                    task.name.as_deref().unwrap_or_default()
                ),
                provider: MODULE.id.to_owned(),
                cron: cron.clone(),
                paused: !enabled || task.enabled == Some(false),
                targets: vec![target.clone()],
                note: None,
                ..Schedule::default()
            })
        })
        .collect();
    schedules.sort_by(|a, b| a.name.cmp(&b.name));
    let store = backup
        .storages
        .get_key_value(STORAGE)
        .or_else(|| backup.storages.iter().next())
        .map(|(storage_name, storage)| {
            let retention = backup
                .tasks
                .iter()
                .find(|t| t.storage_name.as_ref() == Some(storage_name))
                .and_then(|t| t.retention.as_ref().and_then(|r| r.count).or(t.keep))
                .map(|count| count.to_string());
            s3_store(storage_name, storage, enabled, retention)
        });
    let mut runs: Vec<Run> = backups
        .iter()
        .filter_map(|b| {
            let run: BackupView = read_as(b);
            (run.spec.cluster_name.as_deref() == Some(name)).then_some((b, run.status))
        })
        .map(|(b, status)| {
            let outcome = outcome(status.state.as_deref());
            Run {
                name: b.metadata.name.clone().unwrap_or_default(),
                provider: MODULE.id.to_owned(),
                targets: vec![target.clone()],
                outcome,
                started_at: filled_time(status.start.as_ref())
                    .or_else(|| b.metadata.creation_timestamp.as_ref().map(|t| t.0)),
                finished_at: filled_time(status.completed.as_ref()),
                message: filled(status.error.as_ref()),
                restore_from: (outcome == Outcome::Succeeded)
                    .then(|| filled(status.destination.as_ref()))
                    .flatten(),
            }
        })
        .collect();
    let stored = runs.iter().any(Run::stored);
    newest(&mut runs, 20);
    // The operator has seen this spec, restarted the members with the backup agents and given
    // them the storage: `PBMReady` is "PBMConfigurationIsUpToDate".
    let seen = cluster.metadata.generation.is_some()
        && view.status.observed_generation == cluster.metadata.generation;
    let pbm = view
        .status
        .conditions
        .iter()
        .any(|c| c.type_ == "PBMReady" && c.status == "True");
    let ready = enabled && seen && pbm && view.status.state.as_deref() == Some("ready");
    Backups {
        endpoints: backup
            .storages
            .values()
            .map(|s| filled(s.s3.endpoint_url.as_ref()))
            .collect(),
        store,
        schedules,
        runs,
        stored,
        ready,
        recovery_window: None,
        archive_failure: None,
    }
}

/// The first user of `spec.users`. Without `passwordSecretRef` the operator keeps its password in
/// `<cluster>-custom-user-secret` under the user's name, letters and digits only
/// (`pkg/controller/perconaservermongodb/custom_users.go`, `pkg/psmdb/secret/secret.go`, v1.23.1).
fn app_user(view: &ClusterView, name: &str) -> Option<AppUser> {
    let user = view.spec.users.first()?;
    let username = user.name.clone()?;
    let database = Some(user.db.clone().unwrap_or_else(|| "admin".to_owned()));
    let reference = user.password_secret_ref.as_ref();
    let secret = reference
        .and_then(|r| r.name.clone())
        .unwrap_or_else(|| format!("{name}-custom-user-secret"));
    let password_key = reference.map_or_else(
        || username.clone(),
        |r| r.key.clone().unwrap_or_else(|| "password".to_owned()),
    );
    Some(AppUser {
        username,
        database,
        secret,
        password_key,
    })
}

/// The users Secret of a cluster that does not set `spec.secrets.users`.
const OPERATOR_USERS_SECRET: &str = "percona-server-mongodb-users";

/// One `PerconaServerMongoDB` as a database, with its backups. Runs are the newest twenty.
#[must_use]
pub fn database(namespace: &str, cluster: &DynamicObject, backups: &[DynamicObject]) -> Database {
    let name = cluster.metadata.name.clone().unwrap_or_default();
    let read: ClusterView = read_as(cluster);
    let replset = read.spec.replsets.first();
    let rs = replset
        .and_then(|r| r.name.clone())
        .unwrap_or_else(|| "rs0".to_owned());
    let members = replset.and_then(|r| r.size).unwrap_or(3);
    let ready = read
        .status
        .replsets
        .get(&rs)
        .and_then(|r| r.ready)
        .unwrap_or(0);
    let word = filled(read.status.state.as_ref());
    let image = filled(read.spec.image.as_ref());
    let version = filled(read.status.mongo_version.as_ref())
        .or_else(|| image.as_deref().and_then(databases::tag).map(str::to_owned));
    let secret = filled(read.spec.secrets.users.as_ref())
        .unwrap_or_else(|| OPERATOR_USERS_SECRET.to_owned());
    let claim = replset
        .and_then(|r| r.volume_spec.as_ref())
        .and_then(|v| v.persistent_volume_claim.as_ref());
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
        api_version: format!("{}/{}", CLUSTERS.group, CLUSTERS.version),
        api_group: CLUSTERS.group.to_owned(),
        api_group_version: CLUSTERS.version.to_owned(),
        kind: CLUSTERS.kind.to_owned(),
        resource: CLUSTERS.resource.to_owned(),
        state: state(word.as_deref(), members, ready),
        phase: word,
        message: filled(read.status.message.as_ref()),
        members,
        ready,
        primary: None,
        version,
        image,
        storage: claim
            .and_then(|c| c.resources.as_ref())
            .and_then(|r| r.requests.as_ref())
            .and_then(|r| r.get("storage"))
            .map(|q| q.0.clone()),
        storage_class: claim.and_then(|c| c.storage_class_name.clone()),
        claims,
        resources: replset.and_then(|r| r.resources.clone()),
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
        app: app_user(&read, &name),
        backups: backups_of(namespace, cluster, &read, &name, backups),
        created_at: cluster.metadata.creation_timestamp.as_ref().map(|t| t.0),
        object: databases::written(cluster),
        pods: Vec::new(),
        restored: databases::restored(cluster),
        image_updates: None,
        unmanaged: false,
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
    fn its_mongod_pods_are_members_and_its_other_pods_are_not() {
        let mongo = database(
            "shop",
            &object(serde_json::json!({
                "apiVersion": "psmdb.percona.com/v1", "kind": "PerconaServerMongoDB",
                "metadata": {"name": "mongo", "namespace": "shop"}, "spec": {}
            })),
            &[],
        );
        let labels = |instance: &str, component: &str| {
            [
                ("app.kubernetes.io/instance", instance),
                ("app.kubernetes.io/component", component),
            ]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
        };
        assert!(mongo.is_member(&labels("mongo", "mongod")));
        assert!(!mongo.is_member(&labels("mongo", "cfg")));
        assert!(!mongo.is_member(&labels("other", "mongod")));
    }

    #[test]
    fn a_single_member_replica_set_with_backups_to_s3() {
        let cluster = object(serde_json::json!({
            "apiVersion": "psmdb.percona.com/v1", "kind": "PerconaServerMongoDB",
            "metadata": {"name": "mongo", "namespace": "shop", "generation": 2},
            "spec": {
                "image": "percona/percona-server-mongodb:8.0.32-14",
                "unsafeFlags": {"replsetSize": true},
                "replsets": [{"name": "rs0", "size": 1, "volumeSpec": {"persistentVolumeClaim": {
                    "storageClassName": "local-path",
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
            "status": {"state": "ready", "mongoVersion": "8.0.32-14", "observedGeneration": 2,
                       "conditions": [{"type": "PBMReady", "status": "True"}],
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
        assert_eq!(db.storage_class.as_deref(), Some("local-path"));
        assert_eq!(db.connection.host, "mongo-rs0.shop.svc");
        assert_eq!(db.connection.secret, "percona-server-mongodb-users");
        let store = db.backups.store.unwrap();
        assert_eq!(store.destination.as_deref(), Some("s3://b/shop"));
        assert_eq!(store.retention.as_deref(), Some("7"));
        assert!(store.in_use);
        assert!(db.backups.ready);
        assert_eq!(db.backups.schedules[0].cron, "0 3 * * *");
        assert!(!db.backups.schedules[0].paused);
        assert_eq!(db.backups.runs[0].outcome, Outcome::Failed);
        assert_eq!(db.backups.runs[0].message.as_deref(), Some("access denied"));

        assert_eq!(db.backups.endpoints, [Some("http://s3:8333".to_owned())]);

        // Every storage is a place its backups may be, AWS S3's without an endpoint.
        let mut two = cluster.clone();
        two.data["spec"]["backup"]["storages"]["aws"] =
            serde_json::json!({"type": "s3", "s3": {"bucket": "a"}});
        assert_eq!(
            database("shop", &two, &[]).backups.endpoints,
            [None, Some("http://s3:8333".to_owned())]
        );

        // Backups just set up: the operator has not seen the spec yet.
        let mut changed = cluster;
        changed.metadata.generation = Some(3);
        assert!(!database("shop", &changed, &[]).backups.ready);
    }

    #[test]
    fn the_connection_reads_the_users_secret_the_cluster_names() {
        let cluster = object(serde_json::json!({
            "apiVersion": "psmdb.percona.com/v1", "kind": "PerconaServerMongoDB",
            "metadata": {"name": "mongo", "namespace": "shop"},
            "spec": {"image": "x:1", "replsets": [{"name": "rs0", "size": 3}],
                     "secrets": {"users": "mongo-secrets"}},
        }));
        assert_eq!(
            database("shop", &cluster, &[]).connection.secret,
            "mongo-secrets"
        );
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
        assert_eq!(db.app, None);
        assert!(db.backups.schedules[0].paused);
        assert!(db.backups.store.is_none());
        assert!(!db.backups.ready);
    }

    #[test]
    fn the_app_user_reads_its_password_from_the_custom_user_secret() {
        let with = |users: Value| {
            database(
                "shop",
                &object(serde_json::json!({
                    "apiVersion": "psmdb.percona.com/v1", "kind": "PerconaServerMongoDB",
                    "metadata": {"name": "mongo", "namespace": "shop"},
                    "spec": {"replsets": [{"name": "rs0", "size": 1}], "users": users}
                })),
                &[],
            )
            .app
        };
        let app = with(serde_json::json!([{"name": "app", "db": "app",
            "roles": [{"name": "readWrite", "db": "app"}]}]))
        .unwrap();
        assert_eq!(
            (app.username.as_str(), app.database.as_deref()),
            ("app", Some("app"))
        );
        assert_eq!(
            (app.secret.as_str(), app.password_key.as_str()),
            ("mongo-custom-user-secret", "app")
        );
        let own = with(serde_json::json!([{"name": "shop", "db": "shop",
            "passwordSecretRef": {"name": "shop-login", "key": "pw"}}]))
        .unwrap();
        assert_eq!(
            (own.secret.as_str(), own.password_key.as_str()),
            ("shop-login", "pw")
        );
    }
}
