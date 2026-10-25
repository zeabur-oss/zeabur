//! The databases capability: the database clusters an operator runs in a project, how they are,
//! how to connect, and their backups, whichever operator runs them.
//!
//! Each provider ([`crate::cloudnative_pg`] for PostgreSQL, [`crate::percona_mongodb`] for
//! MongoDB, [`crate::moco`] for MySQL, [`crate::mariadb_operator`] for MariaDB and
//! [`crate::dragonfly`] for Dragonfly) turns its own objects into the shared view below. What is
//! shared is what a person asks of any database: is it up, how many members are ready, which one
//! takes writes, which version, where to connect and as whom, and whether it is backed up. The rest
//! (replication settings, extensions, sharding) stays with each provider's objects.
//!
//! Connection details are read from the objects' spec and status, not from a Secret: a viewer
//! may not read Secrets, and the host, port, database and user are not secret. The Secret that
//! holds the password is named, so the interface can reveal it as the person when asked.

use serde::Serialize;
use utoipa::ToSchema;
use zeabur_kube::DynamicObject;

use crate::backups::{Outcome, Run, Schedule, Target, TargetKind};
use crate::{Read, text};

/// `batch/v1` `Job`, in the project's namespace: MOCO and mariadb-operator take each backup in a
/// Job.
pub const JOBS: Read = Read {
    group: "batch",
    version: "v1",
    resource: "jobs",
    kind: "Job",
};

/// The annotation naming the database a restored one was made from.
pub const RESTORED_FROM: &str = "zeabur.com/restored-from";
/// The annotation naming the backup a restored database was made from.
pub const RESTORED_BACKUP: &str = "zeabur.com/restored-backup";
/// The annotation holding when that backup was taken, RFC 3339.
pub const RESTORED_TIME: &str = "zeabur.com/restored-time";

/// A database engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = DatabaseEngine)]
pub enum Engine {
    /// PostgreSQL.
    Postgresql,
    /// MongoDB.
    Mongodb,
    /// MySQL.
    Mysql,
    /// MariaDB.
    Mariadb,
    /// Dragonfly, which speaks the Redis protocol.
    Dragonfly,
}

/// How a database is, from its operator's status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = DatabaseState)]
pub enum State {
    /// Every member is ready.
    Ready,
    /// Being created, scaled, upgraded or switched over.
    Progressing,
    /// Serving, with fewer members ready than asked for.
    Degraded,
    /// The operator reports a failure, or no member is ready.
    Failed,
    /// Paused or stopped on purpose.
    Paused,
    /// The status says nothing Zeabur can read yet.
    Unknown,
}

/// Where and as whom to connect, inside the cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = DatabaseConnection)]
pub struct Connection {
    /// The Service's in-cluster DNS name.
    pub host: String,
    /// The port.
    pub port: u16,
    /// The database to connect to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    /// The user name, when the spec says it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// The Secret holding the credentials, in the project.
    pub secret: String,
    /// The Secret's key holding the user name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username_key: Option<String>,
    /// The Secret's key holding the password.
    pub password_key: String,
    /// Extra connection-string options (`replicaSet=rs0`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<String>,
}

/// A database user for applications, and the Secret key holding its password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = DatabaseAppUser)]
pub struct AppUser {
    /// The user name.
    pub username: String,
    /// The database it may use, and authenticates against; absent when it may make its own
    /// (MOCO's `moco-writable`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    /// The Secret holding its password, in the project.
    pub secret: String,
    /// The Secret's key holding the password.
    pub password_key: String,
}

/// Where backups go: an S3-compatible bucket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = DatabaseBackupStore)]
pub struct Store {
    /// The object naming the target: an `ObjectStore`, or a storage of the database object.
    pub name: String,
    /// The bucket and path (`s3://bucket/path`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
    /// The endpoint URL, when not AWS S3's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// The Secret holding the access keys.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    /// The bucket's region, when the store names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// How long backups are kept, as the provider writes it (`30d`, or a count).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retention: Option<String>,
    /// The database uses this store for its backups (PostgreSQL: its WAL archive).
    pub in_use: bool,
}

/// A database's backups.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = DatabaseBackups)]
pub struct Backups {
    /// Where they go, when set up.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<Store>,
    /// The schedules that back it up.
    pub schedules: Vec<Schedule>,
    /// Its backups, newest first.
    pub runs: Vec<Run>,
    /// A backup can start now: the database runs with its store. False between setting up
    /// backups and the operator restarting the members with them.
    pub ready: bool,
    /// The times a new database can start from, for an engine that archives its write-ahead
    /// log; absent when it does not, or archiving fails.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_window: Option<RecoveryWindow>,
}

/// The times a new database can recover to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = DatabaseRecoveryWindow)]
pub struct RecoveryWindow {
    /// The earliest time, RFC 3339: when the oldest backup in the store finished.
    pub first: String,
    /// The newest time is this many seconds before now: the archive may not hold the most
    /// recent writes yet.
    pub lag_seconds: u64,
}

/// The backup a database was made from, as Zeabur noted it on the object when it planned the
/// restore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = DatabaseRestored)]
pub struct Restored {
    /// The database the backup is of.
    pub from: String,
    /// The backup's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<String>,
    /// When the backup was taken, RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
}

/// One database cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Database {
    /// The object it is read from.
    pub origin: zeabur_model::Origin,
    /// The engine.
    pub engine: Engine,
    /// The provider (module id).
    pub provider: String,
    /// The object's `apiVersion`.
    pub api_version: String,
    /// The object's kind.
    pub kind: String,
    /// The object's plural resource name.
    pub resource: String,
    /// Its name.
    pub name: String,
    /// How it is.
    pub state: State,
    /// The operator's own words for it (`Cluster in healthy state`, `ready`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    /// The operator's message, when it says why.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Members asked for.
    pub members: i64,
    /// Members ready.
    pub ready: i64,
    /// The member taking writes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary: Option<String>,
    /// The server version, as reported, else from the image tag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The server image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// The size of each member's volume, as asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    /// The `StorageClass` of each member's volume, as asked for; absent for the cluster's
    /// default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_class: Option<String>,
    /// The members' PersistentVolumeClaims, as named by the operator's convention.
    pub claims: Vec<String>,
    /// CPU and memory each member asks for and may use, as written.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub resources: Option<serde_json::Value>,
    /// Where and as whom to connect.
    pub connection: Connection,
    /// The user a service connects as; absent when the database has none (a MongoDB without
    /// `spec.users`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<AppUser>,
    /// Its backups.
    pub backups: Backups,
    /// When it was created, RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// The object as written, without `status` and `managedFields`: what an edit (turning
    /// backups on) starts from, so the edit changes only what it adds.
    #[schema(value_type = Object)]
    pub object: serde_json::Value,
    /// Its members' pods, by name; empty when they cannot be read.
    pub pods: Vec<zeabur_model::PodSummary>,
    /// The backup it was restored from, when Zeabur made it from one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restored: Option<Restored>,
}

impl Database {
    /// Whether a pod with `labels` is one of its members. CloudNativePG labels an instance with
    /// `cnpg.io/cluster` and `cnpg.io/instanceName` (its jobs have no instance name); the Percona
    /// Operator labels a replica set member with `app.kubernetes.io/instance` and
    /// `app.kubernetes.io/component: mongod`. MOCO, mariadb-operator and the Dragonfly Operator
    /// label each member with `app.kubernetes.io/name` and `app.kubernetes.io/instance`.
    /// <https://cloudnative-pg.io/documentation/current/labels_annotations/>
    #[must_use]
    pub fn is_member(&self, labels: &std::collections::BTreeMap<String, String>) -> bool {
        let label = |key: &str| labels.get(key).map(String::as_str);
        match self.engine {
            Engine::Postgresql => {
                label("cnpg.io/cluster") == Some(&self.name)
                    && label("cnpg.io/instanceName").is_some()
            }
            Engine::Mongodb => {
                label("app.kubernetes.io/instance") == Some(&self.name)
                    && label("app.kubernetes.io/component") == Some("mongod")
            }
            Engine::Mysql => labelled(labels, "mysql", &self.name),
            Engine::Mariadb => labelled(labels, "mariadb", &self.name),
            Engine::Dragonfly => labelled(labels, "dragonfly", &self.name),
        }
    }
}

/// The backup `object` was restored from, from Zeabur's annotations.
pub(crate) fn restored(object: &DynamicObject) -> Option<Restored> {
    let annotations = object.metadata.annotations.as_ref()?;
    Some(Restored {
        from: annotations.get(RESTORED_FROM)?.clone(),
        backup: annotations.get(RESTORED_BACKUP).cloned(),
        time: annotations.get(RESTORED_TIME).cloned(),
    })
}

/// The runs of the backup Jobs whose owner (a `CronJob` or the backup object) is named in
/// `owners`. The restore point is the time the Job finished.
pub(crate) fn job_runs(
    provider: &str,
    target: &str,
    jobs: &[DynamicObject],
    owners: &[String],
) -> Vec<Run> {
    jobs.iter()
        .filter(|job| {
            job.metadata.owner_references.as_ref().is_some_and(|refs| {
                refs.iter()
                    .any(|r| r.controller == Some(true) && owners.contains(&r.name))
            })
        })
        .map(|job| {
            let finished = text(job, "/status/completionTime");
            let failed = conditions_true(job, "Failed");
            let outcome = if finished.is_some() {
                Outcome::Succeeded
            } else if failed {
                Outcome::Failed
            } else {
                Outcome::Running
            };
            Run {
                name: job.metadata.name.clone().unwrap_or_default(),
                provider: provider.to_owned(),
                targets: vec![Target {
                    kind: TargetKind::Database,
                    name: target.to_owned(),
                }],
                outcome,
                started: text(job, "/status/startTime").or_else(|| {
                    job.metadata
                        .creation_timestamp
                        .as_ref()
                        .map(|t| t.0.to_string())
                }),
                message: failed.then(|| condition_message(job, "Failed")).flatten(),
                restore_from: finished.clone(),
                finished,
            }
        })
        .collect()
}

/// Whether the condition `kind` is true.
pub(crate) fn conditions_true(object: &DynamicObject, kind: &str) -> bool {
    object
        .data
        .pointer("/status/conditions")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|all| {
            all.iter()
                .any(|c| c["type"] == kind && c["status"] == "True")
        })
}

/// The message of the condition `kind`, when it has one.
pub(crate) fn condition_message(object: &DynamicObject, kind: &str) -> Option<String> {
    object
        .data
        .pointer("/status/conditions")?
        .as_array()?
        .iter()
        .find(|c| c["type"] == kind)
        .and_then(|c| c["message"].as_str())
        .filter(|m| !m.is_empty())
        .map(str::to_owned)
}

/// Whether `labels` name the operator's `app` and the database `instance`.
fn labelled(
    labels: &std::collections::BTreeMap<String, String>,
    app: &str,
    instance: &str,
) -> bool {
    labels.get("app.kubernetes.io/instance").map(String::as_str) == Some(instance)
        && labels.get("app.kubernetes.io/name").map(String::as_str) == Some(app)
}

/// `object` without `status` and `managedFields`.
pub(crate) fn written(object: &DynamicObject) -> serde_json::Value {
    let mut value = serde_json::to_value(object).unwrap_or_default();
    if let Some(map) = value.as_object_mut() {
        map.remove("status");
        if let Some(metadata) = map
            .get_mut("metadata")
            .and_then(serde_json::Value::as_object_mut)
        {
            metadata.remove("managedFields");
        }
    }
    value
}

/// An integer field of an object, by JSON pointer.
pub(crate) fn int(object: &DynamicObject, pointer: &str) -> Option<i64> {
    object
        .data
        .pointer(pointer)
        .and_then(serde_json::Value::as_i64)
}

/// The tag of an image reference, when it has one and no digest.
pub(crate) fn tag(image: &str) -> Option<&str> {
    let last = image.rsplit('/').next()?;
    if last.contains('@') {
        return None;
    }
    last.split_once(':').map(|(_, tag)| tag)
}

/// The state of a database from what members are ready, once the operator says it is serving.
pub(crate) const fn by_members(members: i64, ready: i64) -> State {
    if ready <= 0 {
        State::Failed
    } else if ready < members {
        State::Degraded
    } else {
        State::Ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_image_tag_is_read_without_registry_ports_or_digests() {
        assert_eq!(
            tag("ghcr.io/cloudnative-pg/postgresql:18-standard-trixie"),
            Some("18-standard-trixie")
        );
        assert_eq!(tag("registry:5000/pg"), None);
        assert_eq!(tag("pg@sha256:abc"), None);
    }

    #[test]
    fn members_decide_the_state_of_a_serving_database() {
        assert_eq!(by_members(3, 3), State::Ready);
        assert_eq!(by_members(3, 2), State::Degraded);
        assert_eq!(by_members(1, 0), State::Failed);
    }
}
