//! MOCO: MySQL clusters from `MySQLCluster` objects, and their backups to S3 [moco-usage].
//!
//! # As a database
//!
//! A `MySQLCluster` runs the StatefulSet `moco-<cluster>`, whose pods MOCO labels with
//! `app.kubernetes.io/name: mysql` and `app.kubernetes.io/instance: <cluster>`. Writes go through
//! the Service `moco-<cluster>-primary` on port 3306. Each member keeps its data in the claim
//! `mysql-data-moco-<cluster>-<n>`. `status.conditions` has `Initialized`, `Available` and
//! `Healthy`, and `status.syncedReplicas` counts the members in sync, the primary with them
//! [moco-crd]. MOCO keeps its users' passwords in the Secret `moco-<cluster>`; services connect as
//! `moco-writable` (`WRITABLE_PASSWORD`), which may create databases and users [moco-usage].
//!
//! # Backups
//!
//! `spec.backupPolicyName` names a `BackupPolicy`: the schedule, the bucket and the Job's
//! settings. MOCO runs it as the CronJob `moco-backup-<cluster>`; each backup is one of its Jobs.
//! MOCO keeps every backup: it has no retention [moco-backup]. `status.backup.time` is when the
//! last backup was taken, which a restore of it names as `spec.restore.restorePoint`.
//!
//! [moco-usage]: https://github.com/cybozu-go/moco/blob/v0.37.0/docs/usage.md
//! [moco-crd]: https://github.com/cybozu-go/moco/blob/v0.37.0/docs/crd_mysqlcluster_v1beta2.md
//! [moco-backup]: https://github.com/cybozu-go/moco/blob/v0.37.0/docs/backup.md

use serde_json::Value;
use zeabur_kube::DynamicObject;

use crate::backups::{Outcome, Schedule, Target, TargetKind, newest};
use crate::databases::{self, AppUser, Backups, Connection, Database, Engine, State, Store};
use crate::{Module, Read, text};

/// The module.
pub static MODULE: Module = Module {
    id: "moco",
    capability: "mysql",
    reads: &[CLUSTERS, BACKUP_POLICIES],
    http: &[],
};

/// `moco.cybozu.com/v1beta2` `MySQLCluster`, in the project's namespace.
pub const CLUSTERS: Read = Read {
    group: "moco.cybozu.com",
    version: "v1beta2",
    resource: "mysqlclusters",
    kind: "MySQLCluster",
};

/// `moco.cybozu.com/v1beta2` `BackupPolicy`, in the project's namespace.
pub const BACKUP_POLICIES: Read = Read {
    group: "moco.cybozu.com",
    version: "v1beta2",
    resource: "backuppolicies",
    kind: "BackupPolicy",
};

fn condition<'a>(cluster: &'a DynamicObject, kind: &str) -> Option<&'a Value> {
    cluster
        .data
        .pointer("/status/conditions")?
        .as_array()?
        .iter()
        .find(|c| c["type"] == kind)
}

fn state(cluster: &DynamicObject, members: i64, ready: i64) -> State {
    let holds = |kind: &str| condition(cluster, kind).is_some_and(|c| c["status"] == "True");
    if cluster.data.pointer("/spec/offline") == Some(&Value::Bool(true)) {
        State::Paused
    } else if holds("Healthy") {
        databases::by_members(members, ready)
    } else if holds("Available") {
        State::Degraded
    } else if condition(cluster, "Initialized").is_none() || !holds("Initialized") {
        State::Progressing
    } else {
        State::Failed
    }
}

fn mysqld(cluster: &DynamicObject) -> Option<&Value> {
    cluster
        .data
        .pointer("/spec/podTemplate/spec/containers")?
        .as_array()?
        .iter()
        .find(|c| c["name"] == "mysqld")
}

fn data_claim(cluster: &DynamicObject) -> Option<&Value> {
    cluster
        .data
        .pointer("/spec/volumeClaimTemplates")?
        .as_array()?
        .iter()
        .find(|c| c.pointer("/metadata/name").and_then(Value::as_str) == Some("mysql-data"))
}

fn backups_of(
    namespace: &str,
    cluster: &DynamicObject,
    name: &str,
    state: State,
    policies: &[DynamicObject],
    jobs: &[DynamicObject],
) -> Backups {
    let Some(wanted) = text(cluster, "/spec/backupPolicyName") else {
        return Backups::default();
    };
    let policy = policies
        .iter()
        .find(|p| p.metadata.name.as_deref() == Some(&wanted));
    let config = |pointer: &str| policy.and_then(|p| text(p, &format!("/spec/jobConfig{pointer}")));
    let store = Store {
        name: wanted.clone(),
        destination: config("/bucketConfig/bucketName")
            .map(|bucket| format!("s3://{bucket}/moco/{namespace}/{name}")),
        endpoint: config("/bucketConfig/endpointURL"),
        secret: config("/envFrom/0/secretRef/name"),
        region: None,
        retention: None,
        in_use: policy.is_some(),
    };
    let schedules = policy
        .and_then(|p| text(p, "/spec/schedule"))
        .map(|cron| Schedule {
            name: format!("{namespace}/{wanted}"),
            provider: MODULE.id.to_owned(),
            cron,
            paused: false,
            targets: vec![Target {
                kind: TargetKind::Database,
                name: name.to_owned(),
            }],
            note: None,
        })
        .into_iter()
        .collect();
    let mut runs = databases::job_runs(MODULE.id, name, jobs, &[format!("moco-backup-{name}")]);
    newest(&mut runs, 20);
    // The newest backup's binary logs are not in the bucket yet, so it restores to the time of
    // its dump, not the time the Job finished.
    if let (Some(run), Some(time)) = (
        runs.iter_mut().find(|r| r.outcome == Outcome::Succeeded),
        text(cluster, "/status/backup/time"),
    ) {
        run.restore_from = Some(time);
    }
    Backups {
        ready: store.in_use && state == State::Ready,
        store: Some(store),
        schedules,
        runs,
    }
}

/// One `MySQLCluster` as a database, with its `BackupPolicy` and the newest twenty backup Jobs.
#[must_use]
pub fn database(
    namespace: &str,
    cluster: &DynamicObject,
    policies: &[DynamicObject],
    jobs: &[DynamicObject],
) -> Database {
    let name = cluster.metadata.name.clone().unwrap_or_default();
    let members = databases::int(cluster, "/spec/replicas").unwrap_or(1);
    let ready = databases::int(cluster, "/status/syncedReplicas").unwrap_or(0);
    let state = state(cluster, members, ready);
    let healthy = condition(cluster, "Healthy");
    let server = mysqld(cluster);
    let image = server.and_then(|c| c["image"].as_str()).map(str::to_owned);
    let claim = data_claim(cluster);
    let secret = format!("moco-{name}");
    Database {
        origin: zeabur_model::Origin::complete(
            "moco.cybozu.com/v1beta2",
            "MySQLCluster",
            Some(namespace),
            &name,
        ),
        engine: Engine::Mysql,
        provider: MODULE.id.to_owned(),
        api_version: "moco.cybozu.com/v1beta2".to_owned(),
        kind: "MySQLCluster".to_owned(),
        resource: "mysqlclusters".to_owned(),
        state,
        phase: healthy
            .and_then(|c| c["reason"].as_str())
            .map(str::to_owned),
        message: healthy
            .filter(|c| c["status"] != "True")
            .and_then(|c| c["message"].as_str())
            .filter(|m| !m.is_empty())
            .map(str::to_owned),
        members,
        ready,
        primary: databases::int(cluster, "/status/currentPrimaryIndex")
            .filter(|_| cluster.data.pointer("/status/conditions").is_some())
            .map(|i| format!("moco-{name}-{i}")),
        version: image.as_deref().and_then(databases::tag).map(str::to_owned),
        image,
        storage: claim
            .and_then(|c| c.pointer("/spec/resources/requests/storage"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        storage_class: claim
            .and_then(|c| c.pointer("/spec/storageClassName"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        claims: (0..members.max(0))
            .map(|i| format!("mysql-data-moco-{name}-{i}"))
            .collect(),
        resources: server.and_then(|c| c.get("resources")).cloned(),
        connection: Connection {
            host: format!("moco-{name}-primary.{namespace}.svc"),
            port: 3306,
            database: None,
            username: Some("moco-writable".to_owned()),
            secret: secret.clone(),
            username_key: None,
            password_key: "WRITABLE_PASSWORD".to_owned(),
            options: None,
        },
        app: Some(AppUser {
            username: "moco-writable".to_owned(),
            database: None,
            secret,
            password_key: "WRITABLE_PASSWORD".to_owned(),
        }),
        backups: backups_of(namespace, cluster, &name, state, policies, jobs),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn object(value: Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    fn cluster(status: &Value) -> DynamicObject {
        object(serde_json::json!({
            "apiVersion": "moco.cybozu.com/v1beta2", "kind": "MySQLCluster",
            "metadata": {"name": "mysql", "namespace": "shop",
                         "annotations": {"zeabur.com/restored-from": "old",
                                         "zeabur.com/restored-time": "2026-10-20T03:00:00Z"}},
            "spec": {
                "replicas": 3,
                "backupPolicyName": "mysql-backups",
                "podTemplate": {"spec": {"containers": [
                    {"name": "mysqld", "image": "ghcr.io/cybozu-go/moco/mysql:8.4.10",
                     "resources": {"limits": {"memory": "1Gi"}}}]}},
                "volumeClaimTemplates": [{"metadata": {"name": "mysql-data"}, "spec": {
                    "storageClassName": "local-path",
                    "resources": {"requests": {"storage": "5Gi"}}}}]
            },
            "status": status
        }))
    }

    #[test]
    fn a_healthy_cluster_with_its_policy_and_backup_jobs() {
        let policy = object(serde_json::json!({
            "apiVersion": "moco.cybozu.com/v1beta2", "kind": "BackupPolicy",
            "metadata": {"name": "mysql-backups", "namespace": "shop"},
            "spec": {"schedule": "0 3 * * *", "jobConfig": {
                "serviceAccountName": "mysql-backup",
                "envFrom": [{"secretRef": {"name": "mysql-backups-s3"}}],
                "bucketConfig": {"bucketName": "b", "endpointURL": "https://s3.example.com"}}}
        }));
        let job = |name: &str, owner: &str, status: Value| {
            object(serde_json::json!({
                "apiVersion": "batch/v1", "kind": "Job",
                "metadata": {"name": name, "namespace": "shop", "ownerReferences": [
                    {"apiVersion": "batch/v1", "kind": "CronJob", "name": owner, "uid": "u",
                     "controller": true}]},
                "status": status
            }))
        };
        let jobs = [
            job(
                "moco-backup-mysql-1",
                "moco-backup-mysql",
                serde_json::json!({
                "startTime": "2026-10-20T03:00:00Z", "completionTime": "2026-10-20T03:02:00Z"}),
            ),
            job(
                "moco-backup-mysql-2",
                "moco-backup-mysql",
                serde_json::json!({
                "startTime": "2026-10-21T03:00:00Z", "completionTime": "2026-10-21T03:02:00Z"}),
            ),
            job(
                "moco-backup-mysql-3",
                "moco-backup-mysql",
                serde_json::json!({
                "startTime": "2026-10-22T03:00:00Z",
                "conditions": [{"type": "Failed", "status": "True", "message": "BackoffLimitExceeded"}]}),
            ),
            job("other", "moco-backup-other", serde_json::json!({})),
        ];
        let db = database(
            "shop",
            &cluster(&serde_json::json!({
                "conditions": [{"type": "Initialized", "status": "True"},
                               {"type": "Available", "status": "True"},
                               {"type": "Healthy", "status": "True", "reason": "Healthy"}],
                "currentPrimaryIndex": 1, "syncedReplicas": 3,
                "backup": {"time": "2026-10-21T03:00:05Z"}
            })),
            &[policy],
            &jobs,
        );
        assert_eq!(db.state, State::Ready);
        assert_eq!(db.primary.as_deref(), Some("moco-mysql-1"));
        assert_eq!(db.version.as_deref(), Some("8.4.10"));
        assert_eq!(db.claims[2], "mysql-data-moco-mysql-2");
        assert_eq!(db.connection.host, "moco-mysql-primary.shop.svc");
        assert_eq!(db.app.as_ref().and_then(|a| a.database.as_deref()), None);
        let store = db.backups.store.unwrap();
        assert_eq!(store.destination.as_deref(), Some("s3://b/moco/shop/mysql"));
        assert_eq!(store.secret.as_deref(), Some("mysql-backups-s3"));
        assert_eq!(db.backups.schedules[0].cron, "0 3 * * *");
        let runs: Vec<_> = db
            .backups
            .runs
            .iter()
            .map(|r| (r.name.as_str(), r.outcome, r.restore_from.as_deref()))
            .collect();
        assert_eq!(
            runs,
            [
                ("moco-backup-mysql-3", Outcome::Failed, None),
                (
                    "moco-backup-mysql-2",
                    Outcome::Succeeded,
                    Some("2026-10-21T03:00:05Z")
                ),
                (
                    "moco-backup-mysql-1",
                    Outcome::Succeeded,
                    Some("2026-10-20T03:02:00Z")
                ),
            ]
        );
        assert_eq!(db.restored.map(|r| r.from), Some("old".to_owned()));
    }

    #[test]
    fn conditions_decide_the_state() {
        let with = |conditions: Value| {
            database(
                "shop",
                &cluster(&serde_json::json!({"conditions": conditions, "syncedReplicas": 2})),
                &[],
                &[],
            )
            .state
        };
        assert_eq!(with(serde_json::json!([])), State::Progressing);
        assert_eq!(
            with(
                serde_json::json!([{"type": "Initialized", "status": "True"},
                                    {"type": "Available", "status": "True"},
                                    {"type": "Healthy", "status": "False"}])
            ),
            State::Degraded
        );
        assert_eq!(
            with(
                serde_json::json!([{"type": "Initialized", "status": "True"},
                                    {"type": "Available", "status": "False"}])
            ),
            State::Failed
        );
    }
}
