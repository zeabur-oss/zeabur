//! mariadb-operator: MariaDB servers from `MariaDB` objects, and their logical backups to S3
//! [mdb-api].
//!
//! # As a database
//!
//! A `MariaDB` runs the StatefulSet of its name, whose pods the operator labels with
//! `app.kubernetes.io/name: mariadb` and `app.kubernetes.io/instance: <name>`. Clients connect
//! through the Service of its name on `spec.port` (3306), or `<name>-primary` with replication.
//! Each member keeps its data in the claim `storage-<name>-<n>`. The `Ready` condition says how
//! it is, and `status.currentPrimary` names the primary. `spec.database`, `spec.username` and
//! `spec.passwordSecretKeyRef` make the user services connect as [mdb-api].
//!
//! # Backups
//!
//! A `Backup` names the server (`spec.mariaDbRef`), the bucket (`spec.storage.s3`), and how long
//! to keep backups (`spec.maxRetention`). With `spec.schedule` the operator runs it as a CronJob
//! of the same name; without, as one Job. Each Job is one backup. A new `MariaDB` restores one
//! with `spec.bootstrapFrom`: the closest backup at or before `targetRecoveryTime`
//! [mdb-backup].
//!
//! [mdb-api]: https://github.com/mariadb-operator/mariadb-operator/blob/v26.10.1/docs/api_reference.md
//! [mdb-backup]: https://github.com/mariadb-operator/mariadb-operator/blob/v26.10.1/docs/logical_backup.md

use serde_json::Value;
use zeabur_kube::DynamicObject;

use crate::backups::{Schedule, Target, TargetKind, newest};
use crate::databases::{self, AppUser, Backups, Connection, Database, Engine, State, Store};
use crate::{Module, Read, text};

/// The module.
pub static MODULE: Module = Module {
    id: "mariadb-operator",
    capability: "mariadb",
    reads: &[SERVERS, BACKUPS],
    http: &[],
};

/// `k8s.mariadb.com/v1alpha1` `MariaDB`, in the project's namespace.
pub const SERVERS: Read = Read {
    group: "k8s.mariadb.com",
    version: "v1alpha1",
    resource: "mariadbs",
    kind: "MariaDB",
};

/// `k8s.mariadb.com/v1alpha1` `Backup`, in the project's namespace.
pub const BACKUPS: Read = Read {
    group: "k8s.mariadb.com",
    version: "v1alpha1",
    resource: "backups",
    kind: "Backup",
};

fn state(server: &DynamicObject) -> State {
    let ready = server
        .data
        .pointer("/status/conditions")
        .and_then(Value::as_array)
        .and_then(|all| all.iter().find(|c| c["type"] == "Ready"));
    if server.data.pointer("/spec/suspend") == Some(&Value::Bool(true)) {
        return State::Paused;
    }
    match ready {
        None => State::Unknown,
        Some(c) if c["status"] == "True" => State::Ready,
        Some(_) => State::Progressing,
    }
}

fn backups_of(
    namespace: &str,
    name: &str,
    state: State,
    backups: &[DynamicObject],
    jobs: &[DynamicObject],
) -> Backups {
    let mine: Vec<&DynamicObject> = backups
        .iter()
        .filter(|b| text(b, "/spec/mariaDbRef/name").as_deref() == Some(name))
        .collect();
    let target = Target {
        kind: TargetKind::Database,
        name: name.to_owned(),
    };
    let mut schedules: Vec<Schedule> = mine
        .iter()
        .filter_map(|b| {
            Some(Schedule {
                name: format!(
                    "{namespace}/{}",
                    b.metadata.name.as_deref().unwrap_or_default()
                ),
                provider: MODULE.id.to_owned(),
                cron: text(b, "/spec/schedule/cron")?,
                paused: b.data.pointer("/spec/schedule/suspend") == Some(&Value::Bool(true)),
                targets: vec![target.clone()],
                note: None,
                ..Schedule::default()
            })
        })
        .collect();
    schedules.sort_by(|a, b| a.name.cmp(&b.name));
    let store = mine
        .iter()
        .find(|b| b.data.pointer("/spec/schedule").is_some())
        .or_else(|| mine.first())
        .filter(|b| b.data.pointer("/spec/storage/s3").is_some())
        .map(|b| {
            let s3 = |key: &str| text(b, &format!("/spec/storage/s3/{key}"));
            Store {
                name: b.metadata.name.clone().unwrap_or_default(),
                destination: s3("bucket").map(|bucket| {
                    s3("prefix").map_or_else(
                        || format!("s3://{bucket}"),
                        |prefix| format!("s3://{bucket}/{prefix}"),
                    )
                }),
                endpoint: s3("endpoint").map(|host| {
                    let tls =
                        b.data.pointer("/spec/storage/s3/tls/enabled") == Some(&Value::Bool(true));
                    format!("{}://{host}", if tls { "https" } else { "http" })
                }),
                secret: s3("secretAccessKeySecretKeyRef/name"),
                region: s3("region"),
                retention: text(b, "/spec/maxRetention"),
                in_use: true,
            }
        });
    let owners: Vec<String> = mine
        .iter()
        .filter_map(|b| b.metadata.name.clone())
        .collect();
    let mut runs = databases::job_runs(MODULE.id, name, jobs, &owners);
    newest(&mut runs, 20);
    Backups {
        ready: store.is_some() && state == State::Ready,
        store,
        schedules,
        runs,
    }
}

/// One `MariaDB` as a database, with its `Backup`s and the newest twenty backup Jobs.
#[must_use]
pub fn database(
    namespace: &str,
    server: &DynamicObject,
    backups: &[DynamicObject],
    jobs: &[DynamicObject],
) -> Database {
    let name = server.metadata.name.clone().unwrap_or_default();
    let members = databases::int(server, "/spec/replicas").unwrap_or(1);
    let state = state(server);
    let ready = if state == State::Ready { members } else { 0 };
    let condition = server
        .data
        .pointer("/status/conditions")
        .and_then(Value::as_array)
        .and_then(|all| all.iter().find(|c| c["type"] == "Ready"));
    let image = text(server, "/spec/image");
    let replication = server.data.pointer("/spec/replication/enabled") == Some(&Value::Bool(true));
    let service = if replication {
        format!("{name}-primary")
    } else {
        name.clone()
    };
    let username = text(server, "/spec/username");
    let database = text(server, "/spec/database");
    let password = (
        text(server, "/spec/passwordSecretKeyRef/name"),
        text(server, "/spec/passwordSecretKeyRef/key").unwrap_or_else(|| "password".to_owned()),
    );
    let app = match (&username, &password.0) {
        (Some(user), Some(secret)) => Some(AppUser {
            username: user.clone(),
            database: database.clone(),
            secret: secret.clone(),
            password_key: password.1.clone(),
        }),
        _ => None,
    };
    Database {
        origin: zeabur_model::Origin::complete(
            "k8s.mariadb.com/v1alpha1",
            "MariaDB",
            Some(namespace),
            &name,
        ),
        engine: Engine::Mariadb,
        provider: MODULE.id.to_owned(),
        api_version: "k8s.mariadb.com/v1alpha1".to_owned(),
        kind: "MariaDB".to_owned(),
        resource: "mariadbs".to_owned(),
        state,
        phase: condition
            .and_then(|c| c["reason"].as_str())
            .map(str::to_owned),
        message: condition
            .filter(|c| c["status"] != "True")
            .and_then(|c| c["message"].as_str())
            .filter(|m| !m.is_empty())
            .map(str::to_owned),
        members,
        ready,
        primary: text(server, "/status/currentPrimary"),
        version: image
            .as_deref()
            .and_then(databases::tag)
            .map(str::to_owned)
            .or_else(|| text(server, "/status/defaultVersion")),
        image,
        storage: text(server, "/spec/storage/size"),
        storage_class: text(server, "/spec/storage/storageClassName"),
        claims: (0..members.max(0))
            .map(|i| format!("storage-{name}-{i}"))
            .collect(),
        resources: server.data.pointer("/spec/resources").cloned(),
        connection: Connection {
            host: format!("{service}.{namespace}.svc"),
            port: databases::int(server, "/spec/port")
                .and_then(|p| u16::try_from(p).ok())
                .unwrap_or(3306),
            database,
            username,
            secret: password
                .0
                .or_else(|| text(server, "/spec/rootPasswordSecretKeyRef/name"))
                .unwrap_or_else(|| format!("{name}-root")),
            username_key: None,
            password_key: password.1,
            options: None,
        },
        app,
        backups: backups_of(namespace, &name, state, backups, jobs),
        created: server
            .metadata
            .creation_timestamp
            .as_ref()
            .map(|t| t.0.to_string()),
        object: databases::written(server),
        pods: Vec::new(),
        restored: databases::restored(server),
        name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backups::Outcome;

    fn object(value: Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn a_ready_server_with_a_scheduled_backup_and_its_jobs() {
        let server = object(serde_json::json!({
            "apiVersion": "k8s.mariadb.com/v1alpha1", "kind": "MariaDB",
            "metadata": {"name": "maria", "namespace": "shop"},
            "spec": {"image": "docker-registry1.mariadb.com/library/mariadb:11.8.3",
                     "database": "app", "username": "app",
                     "passwordSecretKeyRef": {"name": "maria-app", "key": "password"},
                     "storage": {"size": "5Gi", "storageClassName": "local-path"}},
            "status": {"conditions": [{"type": "Ready", "status": "True", "reason": "StatefulSetReady"}],
                       "currentPrimary": "maria-0"}
        }));
        let backup = |name: &str, schedule: Option<&str>| {
            let mut spec = serde_json::json!({"mariaDbRef": {"name": "maria"},
                "maxRetention": "168h",
                "storage": {"s3": {"bucket": "b", "prefix": "maria", "endpoint": "s3.example.com",
                    "region": "auto", "tls": {"enabled": true},
                    "secretAccessKeySecretKeyRef": {"name": "maria-backups-s3", "key": "k"}}}});
            if let Some(cron) = schedule {
                spec["schedule"] = serde_json::json!({"cron": cron});
            }
            object(serde_json::json!({
                "apiVersion": "k8s.mariadb.com/v1alpha1", "kind": "Backup",
                "metadata": {"name": name, "namespace": "shop"}, "spec": spec
            }))
        };
        let job = |name: &str, owner: &str| {
            object(serde_json::json!({
                "apiVersion": "batch/v1", "kind": "Job",
                "metadata": {"name": name, "namespace": "shop", "ownerReferences": [
                    {"apiVersion": "x", "kind": "CronJob", "name": owner, "uid": "u", "controller": true}]},
                "status": {"startTime": "2026-10-20T03:00:00Z", "completionTime": "2026-10-20T03:01:00Z"}
            }))
        };
        let db = database(
            "shop",
            &server,
            &[
                backup("maria-20261021", None),
                backup("maria-scheduled", Some("0 3 * * *")),
            ],
            &[
                job("maria-scheduled-1", "maria-scheduled"),
                job("maria-20261021", "maria-20261021"),
                job("other-1", "other"),
            ],
        );
        assert_eq!((db.state, db.ready), (State::Ready, 1));
        assert_eq!(db.version.as_deref(), Some("11.8.3"));
        assert_eq!(db.connection.host, "maria.shop.svc");
        assert_eq!(db.connection.secret, "maria-app");
        assert_eq!(db.claims, ["storage-maria-0"]);
        let store = db.backups.store.unwrap();
        assert_eq!(store.name, "maria-scheduled");
        assert_eq!(store.destination.as_deref(), Some("s3://b/maria"));
        assert_eq!(store.retention.as_deref(), Some("168h"));
        assert_eq!(store.endpoint.as_deref(), Some("https://s3.example.com"));
        assert_eq!(store.region.as_deref(), Some("auto"));
        assert_eq!(db.backups.schedules.len(), 1);
        assert_eq!(db.backups.runs.len(), 2);
        assert_eq!(db.backups.runs[0].outcome, Outcome::Succeeded);
        assert_eq!(
            db.backups.runs[0].restore_from.as_deref(),
            Some("2026-10-20T03:01:00Z")
        );
    }

    #[test]
    fn replication_connects_to_the_primary_service() {
        let db = database(
            "shop",
            &object(serde_json::json!({
                "apiVersion": "k8s.mariadb.com/v1alpha1", "kind": "MariaDB",
                "metadata": {"name": "maria", "namespace": "shop"},
                "spec": {"replicas": 3, "replication": {"enabled": true}},
                "status": {"conditions": [{"type": "Ready", "status": "False", "reason": "Initializing"}]}
            })),
            &[],
            &[],
        );
        assert_eq!(db.state, State::Progressing);
        assert_eq!(db.phase.as_deref(), Some("Initializing"));
        assert_eq!(db.connection.host, "maria-primary.shop.svc");
        assert_eq!(db.app, None);
    }
}
