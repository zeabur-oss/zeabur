//! The cluster's backups through Velero: not installed is an answer, setting the location keeps
//! its keys out of every answer, the schedule is created, paused and backed up from, a finished
//! backup is restored into the namespaces it holds, and the refusals: someone RBAC does not let
//! write, a plan the cluster moved past.

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{DynamicObject, ServedResource};

    use crate::common::{app, send};

    const SECRET_KEY: &str = "very-secret-access-key";

    fn served() -> Vec<ServedResource> {
        [
            ("backupstoragelocations", "BackupStorageLocation"),
            ("schedules", "Schedule"),
            ("backups", "Backup"),
            ("podvolumebackups", "PodVolumeBackup"),
            ("restores", "Restore"),
        ]
        .into_iter()
        .map(|(resource, kind)| ServedResource::readable("velero.io", "v1", resource, kind, true))
        .collect()
    }

    fn object(value: Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    fn velero(dynamic: Vec<DynamicObject>) -> MemoryCluster {
        MemoryCluster::new(State {
            served: served(),
            dynamic,
            ..State::default()
        })
    }

    async fn plan(app: &Router, request: &Value) -> (StatusCode, Value) {
        send(app, "POST", "/api/v1/cluster/backups/plan", Some(request)).await
    }

    async fn apply(app: &Router, planned: &Value) -> (StatusCode, Value) {
        send(
            app,
            "POST",
            "/api/v1/cluster/backups/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await
    }

    async fn applied(app: &Router, request: &Value) -> (Value, Value) {
        let (status, planned) = plan(app, request).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        let (status, done) = apply(app, &planned).await;
        assert_eq!(status, StatusCode::OK, "{done}");
        (planned, done)
    }

    fn stored(cluster: &MemoryCluster, kind: &str, name: &str) -> Option<Value> {
        cluster
            .inspect(|s| {
                s.dynamic
                    .iter()
                    .find(|o| {
                        o.types.as_ref().is_some_and(|t| t.kind == kind)
                            && o.metadata.name.as_deref() == Some(name)
                    })
                    .map(|o| serde_json::to_value(o).unwrap())
            })
            .flatten()
    }

    fn location() -> DynamicObject {
        object(json!({
            "apiVersion": "velero.io/v1", "kind": "BackupStorageLocation",
            "metadata": {"name": "default", "namespace": "velero", "resourceVersion": "3"},
            "spec": {"provider": "aws", "default": true,
                     "objectStorage": {"bucket": "backups"},
                     "config": {"region": "us-east-1", "s3Url": "http://minio:9000"}},
            "status": {"phase": "Available", "lastValidationTime": "2026-10-23T03:00:00Z"}
        }))
    }

    fn schedule() -> DynamicObject {
        object(json!({
            "apiVersion": "velero.io/v1", "kind": "Schedule",
            "metadata": {"name": "zeabur", "namespace": "velero", "resourceVersion": "8"},
            "spec": {"schedule": "0 3 * * *",
                     "template": {"includedNamespaces": ["*"], "ttl": "168h0m0s",
                                  "defaultVolumesToFsBackup": true}}
        }))
    }

    #[tokio::test]
    async fn without_velero_the_answer_says_so_and_nothing_can_be_planned() {
        let cluster = MemoryCluster::new(State::default());
        let app = app(&cluster);
        let (status, found) = send(&app, "GET", "/api/v1/cluster/backups", None).await;
        assert_eq!(status, StatusCode::OK, "{found}");
        assert_eq!(found["state"], "not-installed");
        let (status, _) = plan(&app, &json!({"op": "backupNow"})).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_new_location_writes_its_keys_to_a_secret_and_no_answer_shows_them() {
        let cluster = velero(vec![]);
        let app = app(&cluster);
        let request = json!({"op": "location", "bucket": "cluster-backups", "prefix": "prod",
            "endpoint": "https://r2.example.com", "region": "auto",
            "accessKeyId": "AKIA1", "secretAccessKey": SECRET_KEY});
        let (planned, done) = applied(&app, &request).await;
        assert!(!planned.to_string().contains(SECRET_KEY), "{planned}");
        assert!(!done.to_string().contains(SECRET_KEY), "{done}");
        let kinds: Vec<(&str, &str)> = planned["writes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| (w["action"].as_str().unwrap(), w["kind"].as_str().unwrap()))
            .collect();
        assert_eq!(
            kinds,
            [
                ("create", "Secret"),
                ("create", "Secret"),
                ("create", "BackupStorageLocation")
            ]
        );
        let written = stored(&cluster, "BackupStorageLocation", "default").unwrap();
        assert_eq!(
            written["spec"]["objectStorage"]["bucket"],
            "cluster-backups"
        );
        assert_eq!(written["spec"]["config"]["s3Url"], "https://r2.example.com");
        assert_eq!(
            written["spec"]["credential"],
            json!({"name": "zeabur-backup-credentials", "key": "cloud"})
        );
        assert_eq!(
            written["spec"]["config"]["customerKeyEncryptionSecret"],
            "velero-backup-key/key"
        );
        let key = cluster
            .inspect(|s| s.secret_values["velero/velero-backup-key"]["key"].clone())
            .unwrap();
        assert_eq!(key.len(), 32);
        assert!(
            !planned
                .to_string()
                .contains(&String::from_utf8(key).unwrap())
        );
        let keys = cluster
            .inspect(|s| s.secret_values["velero/zeabur-backup-credentials"]["cloud"].clone())
            .unwrap();
        assert_eq!(
            String::from_utf8(keys).unwrap(),
            format!("[default]\naws_access_key_id=AKIA1\naws_secret_access_key={SECRET_KEY}\n")
        );

        let (status, found) = send(&app, "GET", "/api/v1/cluster/backups", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(found["state"], "installed");
        assert_eq!(found["namespace"], "velero");
        assert_eq!(found["locations"][0]["bucket"], "cluster-backups");
        assert_eq!(found["locations"][0]["phase"], "unknown");
    }

    #[tokio::test]
    async fn a_new_location_keeps_the_backup_key_that_exists() {
        let key = object(json!({
            "apiVersion": "v1", "kind": "Secret",
            "metadata": {"name": "velero-backup-key", "namespace": "velero",
                         "resourceVersion": "5"},
        }));
        let cluster = velero(vec![key]);
        let app = app(&cluster);
        let request = json!({"op": "location", "bucket": "backups", "endpoint": "http://s3:8333",
            "accessKeyId": "AKIA1", "secretAccessKey": SECRET_KEY});
        let (planned, _) = applied(&app, &request).await;
        let names: Vec<&str> = planned["writes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["zeabur-backup-credentials", "default"]);
    }

    #[tokio::test]
    async fn a_location_needs_its_keys_until_it_has_some() {
        let app = app(&velero(vec![]));
        let (status, problem) = plan(
            &app,
            &json!({"op": "location", "bucket": "backups", "endpoint": "http://minio:9000"}),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");

        let cluster = velero(vec![location()]);
        let app = self::app(&cluster);
        let (planned, _) = applied(
            &app,
            &json!({"op": "location", "bucket": "other", "endpoint": "http://minio:9000"}),
        )
        .await;
        assert_eq!(planned["writes"].as_array().unwrap().len(), 1);
        assert_eq!(planned["writes"][0]["action"], "patch");
        let written = stored(&cluster, "BackupStorageLocation", "default").unwrap();
        assert_eq!(written["spec"]["objectStorage"]["bucket"], "other");
        assert!(written["spec"].get("credential").is_none());
    }

    #[tokio::test]
    async fn the_schedule_is_created_paused_and_backed_up_from() {
        let cluster = velero(vec![location()]);
        let app = app(&cluster);
        applied(
            &app,
            &json!({"op": "schedule", "cron": "0 3 * * *", "keepDays": 14,
                    "namespaces": ["shop", "blog"]}),
        )
        .await;
        let made = stored(&cluster, "Schedule", "zeabur").unwrap();
        assert_eq!(
            made["spec"]["template"],
            json!({"includedNamespaces": ["shop", "blog"], "ttl": "336h0m0s",
                   "defaultVolumesToFsBackup": true})
        );

        applied(&app, &json!({"op": "pause", "paused": true})).await;
        let (_, found) = send(&app, "GET", "/api/v1/cluster/backups", None).await;
        let view = &found["schedules"][0];
        assert_eq!(
            (&view["paused"], &view["keepDays"], &view["zeabur"]),
            (&json!(true), &json!(14), &json!(true))
        );

        let (planned, done) = applied(&app, &json!({"op": "backupNow"})).await;
        let name = done["backup"].as_str().unwrap();
        assert_eq!(planned["backup"], name);
        assert!(name.starts_with("zeabur-"), "{name}");
        let backup = stored(&cluster, "Backup", name).unwrap();
        assert_eq!(
            backup["metadata"]["labels"]["velero.io/schedule-name"],
            "zeabur"
        );
        assert_eq!(backup["spec"], made["spec"]["template"]);
    }

    #[tokio::test]
    async fn a_plan_the_schedule_moved_past_is_refused() {
        let cluster = velero(vec![location(), schedule()]);
        let app = app(&cluster);
        let (status, planned) = plan(&app, &json!({"op": "pause", "paused": true})).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        cluster.update(|s| {
            let schedule = s
                .dynamic
                .iter_mut()
                .find(|o| o.metadata.name.as_deref() == Some("zeabur"))
                .unwrap();
            schedule.metadata.resource_version = Some("9".to_owned());
        });
        let (status, problem) = apply(&app, &planned).await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
        assert_eq!(
            stored(&cluster, "Schedule", "zeabur").unwrap()["spec"].get("paused"),
            None
        );
    }

    #[tokio::test]
    async fn a_write_rbac_refuses_names_the_right_it_needs() {
        let cluster = velero(vec![location(), schedule()]);
        cluster.update(|s| {
            s.forbidden_writes.insert("Schedule/zeabur".to_owned());
        });
        let app = app(&cluster);
        let (_, planned) = plan(&app, &json!({"op": "pause", "paused": true})).await;
        let (status, problem) = apply(&app, &planned).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(
            problem["denied"],
            json!({"verb": "patch", "group": "velero.io", "resource": "schedules",
                   "name": "zeabur", "namespace": "velero"})
        );
    }

    #[tokio::test]
    async fn backups_are_listed_newest_first_with_why_one_failed_and_how_big_each_is() {
        let backup = |name: &str, started: &str, status: Value| {
            let mut status = status;
            status["startTimestamp"] = json!(started);
            object(json!({
                "apiVersion": "velero.io/v1", "kind": "Backup",
                "metadata": {"name": name, "namespace": "velero",
                             "labels": {"velero.io/schedule-name": "zeabur"}},
                "spec": {"includedNamespaces": ["*"]}, "status": status
            }))
        };
        let cluster = velero(vec![
            location(),
            schedule(),
            backup(
                "zeabur-20261022030000",
                "2026-10-22T03:00:00Z",
                json!({"phase": "Completed", "expiration": "2026-10-29T03:00:00Z"}),
            ),
            backup(
                "zeabur-20261023030000",
                "2026-10-23T03:00:00Z",
                json!({"phase": "Failed", "failureReason":
                    "backup storage location default is unavailable"}),
            ),
            object(json!({
                "apiVersion": "velero.io/v1", "kind": "PodVolumeBackup",
                "metadata": {"name": "pvb-1", "namespace": "velero",
                             "labels": {"velero.io/backup-name": "zeabur-20261022030000"}},
                "status": {"progress": {"bytesDone": 2048}}
            })),
        ]);
        let app = app(&cluster);
        let (status, found) = send(&app, "GET", "/api/v1/cluster/backups", None).await;
        assert_eq!(status, StatusCode::OK, "{found}");
        let names: Vec<&str> = found["backups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["zeabur-20261023030000", "zeabur-20261022030000"]);
        assert_eq!(found["backups"][0]["failure"], "locationUnavailable");
        assert_eq!(found["backups"][1]["bytes"], 2048);
        assert_eq!(found["totalBackups"], 2);
        assert_eq!(found["canChange"], "allowed");
        assert_eq!(found["canRestore"], "allowed");
    }

    #[tokio::test]
    async fn who_may_change_schedules_may_not_restore_without_its_own_right() {
        let cluster = MemoryCluster::new(State {
            served: served(),
            dynamic: vec![location(), schedule()],
            refused_rights: ["create restores".to_owned()].into(),
            ..State::default()
        });
        let (status, found) = send(&app(&cluster), "GET", "/api/v1/cluster/backups", None).await;
        assert_eq!(status, StatusCode::OK, "{found}");
        assert_eq!(found["canChange"], "allowed");
        assert_eq!(found["canRestore"], "denied");
    }

    #[tokio::test]
    async fn a_finished_backup_is_restored_into_namespaces_it_holds() {
        let backup = |name: &str, phase: &str| {
            object(json!({
                "apiVersion": "velero.io/v1", "kind": "Backup",
                "metadata": {"name": name, "namespace": "velero", "resourceVersion": "4"},
                "spec": {"includedNamespaces": ["shop", "blog"]}, "status": {"phase": phase}
            }))
        };
        let cluster = velero(vec![
            location(),
            schedule(),
            backup("zeabur-1", "Completed"),
            backup("zeabur-2", "InProgress"),
        ]);
        let app = app(&cluster);
        let (planned, done) = applied(
            &app,
            &json!({"op": "restore", "backup": "zeabur-1", "namespaces": ["shop"]}),
        )
        .await;
        let name = done["restore"].as_str().unwrap();
        assert_eq!(planned["restore"], name);
        assert!(name.starts_with("zeabur-1-"), "{name}");
        let made = stored(&cluster, "Restore", name).unwrap();
        assert_eq!(
            made["spec"],
            json!({"backupName": "zeabur-1", "includedNamespaces": ["shop"]})
        );
        let (_, found) = send(&app, "GET", "/api/v1/cluster/backups", None).await;
        assert_eq!(found["restores"][0]["backup"], "zeabur-1");

        for request in [
            json!({"op": "restore", "backup": "zeabur-2", "namespaces": ["shop"]}),
            json!({"op": "restore", "backup": "zeabur-1", "namespaces": ["payments"]}),
            json!({"op": "restore", "backup": "zeabur-1", "namespaces": []}),
        ] {
            let (status, problem) = plan(&app, &request).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");
        }
    }

    #[tokio::test]
    async fn a_refused_read_is_not_velero_missing() {
        let cluster = velero(vec![location(), schedule()]);
        cluster.update(|s| {
            for resource in ["schedules", "backupstoragelocations", "backups"] {
                s.forbidden_resources
                    .insert(format!("{resource}.velero.io"));
            }
        });
        let app = app(&cluster);
        let (status, found) = send(&app, "GET", "/api/v1/cluster/backups", None).await;
        assert_eq!(status, StatusCode::OK, "{found}");
        assert_eq!(found["state"], "refused");
        assert_eq!(found["unreadable"].as_array().unwrap().len(), 3);
    }
}
