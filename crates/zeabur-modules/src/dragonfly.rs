//! The Dragonfly Operator: Redis-compatible stores from `Dragonfly` objects [df-api].
//!
//! # As a database
//!
//! A `Dragonfly` runs the StatefulSet of its name, whose pods the operator labels with
//! `app: <name>`, `app.kubernetes.io/name: dragonfly`, and `role: master` or `role: replica`;
//! there is no `app.kubernetes.io/instance`. Clients connect through the Service of its name on
//! port 6379, which sends them to the master. `status.phase` is `Ready`, `Configuring`,
//! `RollingUpdate` or `ResourcesCreated` [df-api]. `spec.authentication.passwordFromSecret` names
//! the password.
//!
//! # No backups
//!
//! Dragonfly writes snapshots to `spec.snapshot.dir` on the `spec.snapshot.cron` schedule, and on
//! shutdown. At start it loads the newest snapshot there [df-backups]. The folder is where the
//! store persists, not a separate copy, so Zeabur shows no backups for it.
//!
//! The Dragonfly server is under the Business Source License 1.1; the operator is Apache-2.0.
//!
//! [df-api]: https://github.com/dragonflydb/dragonfly-operator/blob/v1.7.1/api/v1alpha1/dragonfly_types.go
//! [df-backups]: https://www.dragonflydb.io/docs/managing-dragonfly/backups

use serde_json::Value;
use zeabur_kube::DynamicObject;

use crate::databases::{self, AppUser, Backups, Connection, Database, Engine, State};
use crate::{Module, Read, text};

/// The module.
pub static MODULE: Module = Module {
    id: "dragonfly-operator",
    capability: "redis",
    reads: &[STORES],
    http: &[],
};

/// `dragonflydb.io/v1alpha1` `Dragonfly`, in the project's namespace.
pub const STORES: Read = Read {
    group: "dragonflydb.io",
    version: "v1alpha1",
    resource: "dragonflies",
    kind: "Dragonfly",
};

fn state(phase: Option<&str>, members: i64) -> State {
    match phase {
        Some("Ready") => databases::by_members(members, members),
        Some("Configuring" | "RollingUpdate" | "ResourcesCreated") => State::Progressing,
        Some("NotReady") => State::Degraded,
        Some(_) | None => State::Unknown,
    }
}

/// One `Dragonfly` as a database.
#[must_use]
pub fn database(namespace: &str, store: &DynamicObject) -> Database {
    let name = store.metadata.name.clone().unwrap_or_default();
    let members = databases::int(store, "/spec/replicas").unwrap_or(1);
    let phase = text(store, "/status/phase");
    let state = state(phase.as_deref(), members);
    let image = text(store, "/spec/image");
    let claim = store
        .data
        .pointer("/spec/snapshot/persistentVolumeClaimSpec");
    let password = (
        text(store, "/spec/authentication/passwordFromSecret/name"),
        text(store, "/spec/authentication/passwordFromSecret/key")
            .unwrap_or_else(|| "password".to_owned()),
    );
    Database {
        origin: zeabur_model::Origin::complete(
            "dragonflydb.io/v1alpha1",
            "Dragonfly",
            Some(namespace),
            &name,
        ),
        engine: Engine::Dragonfly,
        provider: MODULE.id.to_owned(),
        api_version: "dragonflydb.io/v1alpha1".to_owned(),
        kind: "Dragonfly".to_owned(),
        resource: "dragonflies".to_owned(),
        state,
        phase,
        message: None,
        members,
        ready: if state == State::Ready { members } else { 0 },
        primary: None,
        version: image
            .as_deref()
            .and_then(databases::tag)
            .map(|t| t.trim_start_matches('v').to_owned()),
        image,
        storage: claim
            .and_then(|c| c.pointer("/resources/requests/storage"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        storage_class: claim
            .and_then(|c| c.get("storageClassName"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        claims: if claim.is_some() {
            (0..members.max(0))
                .map(|i| format!("df-{name}-{i}"))
                .collect()
        } else {
            Vec::new()
        },
        resources: store.data.pointer("/spec/resources").cloned(),
        connection: Connection {
            host: format!("{name}.{namespace}.svc"),
            port: 6379,
            database: None,
            username: None,
            secret: password.0.clone().unwrap_or_default(),
            username_key: None,
            password_key: password.1.clone(),
            options: None,
        },
        app: password.0.map(|secret| AppUser {
            username: "default".to_owned(),
            database: None,
            secret,
            password_key: password.1,
        }),
        backups: Backups::default(),
        created: store
            .metadata
            .creation_timestamp
            .as_ref()
            .map(|t| t.0.to_string()),
        object: databases::written(store),
        pods: Vec::new(),
        restored: databases::restored(store),
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
    fn its_pods_carry_the_operators_labels() {
        let db = database(
            "shop",
            &object(serde_json::json!({
                "apiVersion": "dragonflydb.io/v1alpha1", "kind": "Dragonfly",
                "metadata": {"name": "cache", "namespace": "shop"}, "spec": {}
            })),
        );
        let labels = |app: &str, name: &str| {
            [
                ("app", app),
                ("app.kubernetes.io/name", name),
                ("app.kubernetes.io/part-of", app),
                ("role", "master"),
            ]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
        };
        assert!(db.is_member(&labels("cache", "dragonfly")));
        assert!(!db.is_member(&labels("other", "dragonfly")));
        assert!(!db.is_member(&labels("cache", "mariadb")));
    }

    #[test]
    fn snapshots_in_a_bucket_are_not_backups() {
        let db = database(
            "shop",
            &object(serde_json::json!({
                "apiVersion": "dragonflydb.io/v1alpha1", "kind": "Dragonfly",
                "metadata": {"name": "cache", "namespace": "shop"},
                "spec": {
                    "replicas": 2,
                    "image": "docker.dragonflydb.io/dragonflydb/dragonfly:v1.34.1",
                    "args": ["--s3_endpoint=s3.example.com"],
                    "env": [{"name": "AWS_SECRET_ACCESS_KEY", "valueFrom": {"secretKeyRef": {
                        "name": "cache-backups-s3", "key": "AWS_SECRET_ACCESS_KEY"}}}],
                    "authentication": {"passwordFromSecret": {"name": "cache-auth", "key": "password"}},
                    "snapshot": {"dir": "s3://b/cache", "cron": "0 3 * * *"}
                },
                "status": {"phase": "Ready"}
            })),
        );
        assert_eq!((db.state, db.ready), (State::Ready, 2));
        assert_eq!(db.version.as_deref(), Some("1.34.1"));
        assert_eq!(db.claims, Vec::<String>::new());
        assert_eq!(db.connection.secret, "cache-auth");
        assert!(db.backups.store.is_none());
    }

    #[test]
    fn snapshots_on_a_volume_are_not_backups() {
        let db = database(
            "shop",
            &object(serde_json::json!({
                "apiVersion": "dragonflydb.io/v1alpha1", "kind": "Dragonfly",
                "metadata": {"name": "cache", "namespace": "shop"},
                "spec": {"snapshot": {"cron": "*/5 * * * *", "persistentVolumeClaimSpec": {
                    "storageClassName": "local-path",
                    "resources": {"requests": {"storage": "2Gi"}}}}},
                "status": {"phase": "Configuring"}
            })),
        );
        assert_eq!(db.state, State::Progressing);
        assert_eq!(db.claims, ["df-cache-0"]);
        assert_eq!(db.storage.as_deref(), Some("2Gi"));
        assert!(db.backups.store.is_none());
        assert_eq!(db.app, None);
    }
}
