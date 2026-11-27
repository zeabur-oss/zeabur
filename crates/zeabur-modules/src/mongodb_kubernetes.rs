//! MongoDB Controllers for Kubernetes (MCK): MongoDB replica sets from `MongoDBCommunity` objects.
//!
//! Zeabur shows them and edits only their YAML. It does not create or delete them, and MCK
//! Community has no backup of its own, so Zeabur cannot back one up or restore it [mck-crd].
//!
//! # As a database
//!
//! The operator runs one StatefulSet named after the object, with one pod per member of
//! `spec.members`, labelled `app: <service>` [mck-sts]. Each member's data is in the claim
//! `data-volume-<name>-<n>`, 10G unless `spec.statefulSet` overrides the claim template
//! [mck-arch]. The Service is `spec.statefulSet.spec.serviceName`, else `<name>-svc`, and the
//! replica set is named after the object; `mongod` listens on `net.port` of
//! `spec.additionalMongodConfig`, else 27017. With `spec.security.tls.enabled`, connections use
//! TLS [mck-types].
//!
//! `status.phase` is `Running` once every member has reached its goal, `Pending` while the
//! operator works towards it and `Failed` when it cannot [mck-controller].
//!
//! # Users
//!
//! There is no administrator account to sign in as: the people who run the database list its
//! users in `spec.users`. For each, the operator writes a Secret with the keys `username` and
//! `password`, named `users[].connectionStringSecretName`, else `<name>-<db>-<user>` made into a
//! DNS subdomain name; `db` defaults to `admin` [mck-deploy] [mck-users]. Zeabur connects as the
//! first user.
//!
//! [mck-crd]: https://github.com/mongodb/mongodb-kubernetes/blob/1.16.0/config/crd/bases/mongodbcommunity.mongodb.com_mongodbcommunity.yaml
//! [mck-arch]: https://github.com/mongodb/mongodb-kubernetes/blob/1.16.0/docs/mongodbcommunity/architecture.md
//! [mck-deploy]: https://github.com/mongodb/mongodb-kubernetes/blob/1.16.0/docs/mongodbcommunity/deploy-configure.md
//! [mck-users]: https://github.com/mongodb/mongodb-kubernetes/blob/1.16.0/docs/mongodbcommunity/users.md
//! [mck-types]: https://github.com/mongodb/mongodb-kubernetes/blob/1.16.0/mongodb-community-operator/api/v1/mongodbcommunity_types.go
//! [mck-sts]: https://github.com/mongodb/mongodb-kubernetes/blob/1.16.0/mongodb-community-operator/controllers/construct/mongodbstatefulset.go
//! [mck-controller]: https://github.com/mongodb/mongodb-kubernetes/blob/1.16.0/mongodb-community-operator/controllers/replica_set_controller.go

use k8s_openapi::api::core::v1::{Container, PersistentVolumeClaim};
use serde::Deserialize;
use zeabur_kube::DynamicObject;

use crate::databases::{self, AppUser, Backups, Connection, Database, Engine, State};
use crate::{Module, Read, filled, read_as};

/// The module.
pub static MODULE: Module = Module {
    id: "mongodb-kubernetes",
    component: None,
    capability: "mongodb",
    reads: &[CLUSTERS],
    http: &[],
};

/// `mongodbcommunity.mongodb.com/v1` `MongoDBCommunity`, in the project's namespace.
pub const CLUSTERS: Read = Read {
    group: "mongodbcommunity.mongodb.com",
    version: "v1",
    resource: "mongodbcommunity",
    kind: "MongoDBCommunity",
};

/// The claim template holding each member's data.
const DATA_VOLUME: &str = "data-volume";
/// The container `mongod` runs in.
const MONGOD: &str = "mongod";

/// The fields of a `MongoDBCommunity` that Zeabur reads [mck-crd].
#[derive(Default, Deserialize)]
#[serde(default)]
struct View {
    spec: Spec,
    status: Status,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Spec {
    members: Option<i64>,
    version: Option<String>,
    users: Vec<User>,
    security: Security,
    additional_mongod_config: MongodConfig,
    stateful_set: StatefulSetOverride,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct User {
    name: Option<String>,
    db: Option<String>,
    connection_string_secret_name: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Security {
    tls: Tls,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Tls {
    enabled: bool,
}

/// `mongod`'s settings, as nested maps or dotted keys: the operator reads both.
#[derive(Default, Deserialize)]
#[serde(default)]
struct MongodConfig {
    net: Net,
    #[serde(rename = "net.port")]
    net_port: Option<u16>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Net {
    port: Option<u16>,
}

/// `spec.statefulSet`, merged by the operator into the StatefulSet it makes.
#[derive(Default, Deserialize)]
#[serde(default)]
struct StatefulSetOverride {
    spec: StatefulSetSpec,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct StatefulSetSpec {
    service_name: Option<String>,
    volume_claim_templates: Vec<PersistentVolumeClaim>,
    template: PodTemplate,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct PodTemplate {
    spec: PodSpec,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct PodSpec {
    containers: Vec<Container>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Status {
    phase: Option<String>,
    version: Option<String>,
    message: Option<String>,
}

fn state(phase: Option<&str>, members: i64) -> State {
    match phase {
        Some("Running") => databases::by_members(members, members),
        Some("Pending") => State::Progressing,
        Some("Failed") => State::Failed,
        Some(_) | None => State::Unknown,
    }
}

/// `name` as the operator makes it a DNS subdomain name: unchanged when it is one, else in lower
/// case, each run of other characters than letters, digits and `-` made one `-`, without `-` at
/// either end, at most 253 characters (`NormalizeName`, `pkg/util/util.go`, 1.16.0).
fn subdomain(name: &str) -> String {
    let valid = !name.is_empty()
        && name.len() <= 253
        && name.split('.').all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        });
    if valid {
        return name.to_owned();
    }
    let mut out = String::with_capacity(name.len());
    for c in name.to_lowercase().chars() {
        let c = if c.is_ascii_lowercase() || c.is_ascii_digit() {
            c
        } else {
            '-'
        };
        if !(c == '-' && out.ends_with('-')) {
            out.push(c);
        }
    }
    out.trim_matches('-').chars().take(253).collect()
}

/// The first user, and the Secret the operator writes its credentials to.
fn first_user(view: &View, name: &str) -> Option<AppUser> {
    let user = view.spec.users.first()?;
    let username = filled(user.name.as_ref())?;
    let db = filled(user.db.as_ref()).unwrap_or_else(|| "admin".to_owned());
    let secret = filled(user.connection_string_secret_name.as_ref())
        .unwrap_or_else(|| subdomain(&format!("{name}-{db}-{username}")));
    Some(AppUser {
        username,
        database: Some(db),
        secret,
        password_key: "password".to_owned(),
    })
}

/// One `MongoDBCommunity` as an unmanaged database.
#[must_use]
pub fn database(namespace: &str, cluster: &DynamicObject) -> Database {
    let name = cluster.metadata.name.clone().unwrap_or_default();
    let read: View = read_as(cluster);
    let spec = &read.spec;
    let members = spec.members.unwrap_or(0);
    let phase = filled(read.status.phase.as_ref());
    let state = state(phase.as_deref(), members);
    let service = filled(spec.stateful_set.spec.service_name.as_ref())
        .unwrap_or_else(|| format!("{name}-svc"));
    let port = spec
        .additional_mongod_config
        .net_port
        .or(spec.additional_mongod_config.net.port)
        .unwrap_or(27017);
    let claim = spec
        .stateful_set
        .spec
        .volume_claim_templates
        .iter()
        .find(|c| c.metadata.name.as_deref() == Some(DATA_VOLUME))
        .and_then(|c| c.spec.as_ref());
    let resources = spec
        .stateful_set
        .spec
        .template
        .spec
        .containers
        .iter()
        .find(|c| c.name == MONGOD)
        .and_then(|c| c.resources.as_ref())
        .and_then(|r| serde_json::to_value(r).ok());
    let user = first_user(&read, &name);
    let options = if spec.security.tls.enabled {
        format!("replicaSet={name}&tls=true")
    } else {
        format!("replicaSet={name}")
    };
    Database {
        origin: zeabur_model::Origin::complete(
            "mongodbcommunity.mongodb.com/v1",
            "MongoDBCommunity",
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
        state,
        phase,
        message: filled(read.status.message.as_ref()),
        members,
        ready: if state == State::Ready { members } else { 0 },
        primary: None,
        version: filled(read.status.version.as_ref()).or_else(|| filled(spec.version.as_ref())),
        image: None,
        storage: Some(
            claim
                .and_then(|c| c.resources.as_ref())
                .and_then(|r| r.requests.as_ref())
                .and_then(|r| r.get("storage"))
                .map_or_else(|| "10G".to_owned(), |q| q.0.clone()),
        ),
        storage_class: claim.and_then(|c| c.storage_class_name.clone()),
        claims: (0..members.max(0))
            .map(|i| format!("{DATA_VOLUME}-{name}-{i}"))
            .collect(),
        resources,
        connection: Connection {
            host: format!("{service}.{namespace}.svc"),
            port,
            database: user.as_ref().and_then(|u| u.database.clone()),
            username: user.as_ref().map(|u| u.username.clone()),
            secret: user.as_ref().map_or_default(|u| u.secret.clone()),
            username_key: Some("username".to_owned()),
            password_key: "password".to_owned(),
            options: Some(options),
        },
        app: user,
        backups: Backups::default(),
        created_at: cluster.metadata.creation_timestamp.as_ref().map(|t| t.0),
        object: databases::written(cluster),
        pods: Vec::new(),
        restored: None,
        image_updates: None,
        unmanaged: true,
        name,
    }
}

/// The label MCK gives each member pod: the Service's name, the first label of the host.
pub(crate) fn member_label(database: &Database) -> Option<&str> {
    database.connection.host.split('.').next()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(value: serde_json::Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn a_running_replica_set_with_its_first_user() {
        let db = database(
            "shop",
            &object(serde_json::json!({
                "apiVersion": "mongodbcommunity.mongodb.com/v1", "kind": "MongoDBCommunity",
                "metadata": {"name": "mongo", "namespace": "shop"},
                "spec": {
                    "members": 3, "type": "ReplicaSet", "version": "8.0.4",
                    "security": {"authentication": {"modes": ["SCRAM"]}},
                    "users": [{"name": "my-user", "db": "admin",
                        "passwordSecretRef": {"name": "my-user-password"},
                        "roles": [{"name": "readWrite", "db": "app"}],
                        "scramCredentialsSecretName": "my-scram"}],
                    "statefulSet": {"spec": {"volumeClaimTemplates": [{
                        "metadata": {"name": "data-volume"},
                        "spec": {"storageClassName": "local-path",
                            "resources": {"requests": {"storage": "5Gi"}}}}]}}
                },
                "status": {"phase": "Running", "version": "8.0.4", "mongoUri": "mongodb://x",
                    "currentMongoDBMembers": 3, "currentStatefulSetReplicas": 3}
            })),
        );
        assert!(db.unmanaged);
        assert_eq!((db.state, db.members, db.ready), (State::Ready, 3, 3));
        assert_eq!(db.version.as_deref(), Some("8.0.4"));
        assert_eq!(
            (db.storage.as_deref(), db.storage_class.as_deref()),
            (Some("5Gi"), Some("local-path"))
        );
        assert_eq!(db.claims[2], "data-volume-mongo-2");
        let c = &db.connection;
        assert_eq!((c.host.as_str(), c.port), ("mongo-svc.shop.svc", 27017));
        assert_eq!(c.secret, "mongo-admin-my-user");
        assert_eq!(c.username.as_deref(), Some("my-user"));
        assert_eq!(c.options.as_deref(), Some("replicaSet=mongo"));
        assert_eq!(db.app.as_ref().unwrap().secret, "mongo-admin-my-user");
        assert!(db.backups.store.is_none() && db.backups.runs.is_empty());

        let pod = |app: &str| std::iter::once(("app".to_owned(), app.to_owned())).collect();
        assert!(db.is_member(&pod("mongo-svc")));
        assert!(!db.is_member(&pod("other-svc")));
    }

    #[test]
    fn the_spec_moves_the_service_port_and_user_secret() {
        let db = database(
            "shop",
            &object(serde_json::json!({
                "apiVersion": "mongodbcommunity.mongodb.com/v1", "kind": "MongoDBCommunity",
                "metadata": {"name": "mongo", "namespace": "shop"},
                "spec": {
                    "members": 1, "type": "ReplicaSet", "version": "7.0.12",
                    "security": {"tls": {"enabled": true}},
                    "additionalMongodConfig": {"net.port": 27018},
                    "users": [{"name": "App_User", "db": "shop",
                        "connectionStringSecretName": "shop-login"}],
                    "statefulSet": {"spec": {"serviceName": "db"}}
                },
                "status": {"phase": "Pending", "message": "ReplicaSet is not yet ready"}
            })),
        );
        assert_eq!(db.state, State::Progressing);
        assert_eq!(db.storage.as_deref(), Some("10G"));
        let c = &db.connection;
        assert_eq!((c.host.as_str(), c.port), ("db.shop.svc", 27018));
        assert_eq!(c.secret, "shop-login");
        assert_eq!(c.options.as_deref(), Some("replicaSet=mongo&tls=true"));
    }

    #[test]
    fn a_user_secret_name_is_made_a_subdomain_name_as_the_operator_does() {
        assert_eq!(subdomain("mongo-admin-my-user"), "mongo-admin-my-user");
        assert_eq!(subdomain("mongo-admin-App_User"), "mongo-admin-app-user");
        assert_eq!(subdomain("mongo-$external-_x_"), "mongo-external-x");
    }

    #[test]
    fn without_users_there_is_no_one_to_connect_as() {
        let db = database(
            "shop",
            &object(serde_json::json!({
                "apiVersion": "mongodbcommunity.mongodb.com/v1", "kind": "MongoDBCommunity",
                "metadata": {"name": "mongo", "namespace": "shop"},
                "spec": {"members": 3, "type": "ReplicaSet", "users": []},
                "status": {"phase": "Failed", "message": "error"}
            })),
        );
        assert_eq!((db.state, db.ready), (State::Failed, 0));
        assert_eq!(db.connection.secret, "");
        assert_eq!(db.app, None);
    }
}
