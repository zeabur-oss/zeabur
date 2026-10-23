//! The databases capability: the database clusters an operator runs in a project, how they are,
//! how to connect, and their backups, whichever operator runs them.
//!
//! Each provider ([`crate::cloudnative_pg`] for PostgreSQL, [`crate::percona_mongodb`] for
//! MongoDB) turns its own objects into the shared view below. What is shared is what a person
//! asks of any database: is it up, how many members are ready, which one takes writes, which
//! version, where to connect and as whom, and whether it is backed up. The rest (replication
//! settings, extensions, sharding) stays with each provider's objects.
//!
//! Connection details are read from the objects' spec and status, not from a Secret: a viewer
//! may not read Secrets, and the host, port, database and user are not secret. The Secret that
//! holds the password is named, so the interface can reveal it as the person when asked.

use serde::Serialize;
use utoipa::ToSchema;
use zeabur_kube::DynamicObject;

use crate::backups::{Run, Schedule};

/// A database engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = DatabaseEngine)]
pub enum Engine {
    /// PostgreSQL.
    Postgresql,
    /// MongoDB.
    Mongodb,
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
    /// The database it may use, and authenticates against.
    pub database: String,
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
}

impl Database {
    /// Whether a pod with `labels` is one of its members. CloudNativePG labels an instance with
    /// `cnpg.io/cluster` and `cnpg.io/instanceName` (its jobs have no instance name); the Percona
    /// Operator labels a replica set member with `app.kubernetes.io/instance` and
    /// `app.kubernetes.io/component: mongod`.
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
        }
    }
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
