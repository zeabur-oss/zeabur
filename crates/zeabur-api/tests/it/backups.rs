//! The cluster's backups through Velero: not installed is an answer, setting the location keeps
//! its keys out of every answer, the schedule is created, paused and backed up from, a finished
//! backup goes into a new project while the original runs and back under its own name once it is
//! gone, both without routes and with the plan read from the backup's list, domains come back
//! from the backup or move from the project that serves them, and the refusals: someone RBAC does
//! not let write, a plan the cluster moved past, a domain another project serves.

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::k8s_openapi::api::networking::v1::Ingress;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{DynamicObject, NamespaceObjects, ServedResource};

    use crate::common::{app, send};

    const SECRET_KEY: &str = "very-secret-access-key";

    fn served() -> Vec<ServedResource> {
        [
            ("backupstoragelocations", "BackupStorageLocation"),
            ("schedules", "Schedule"),
            ("backups", "Backup"),
            ("podvolumebackups", "PodVolumeBackup"),
            ("restores", "Restore"),
            ("downloadrequests", "DownloadRequest"),
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

    fn backup(name: &str, phase: &str) -> DynamicObject {
        object(json!({
            "apiVersion": "velero.io/v1", "kind": "Backup",
            "metadata": {"name": name, "namespace": "velero", "resourceVersion": "4"},
            "spec": {"includedNamespaces": ["shop", "blog"]}, "status": {"phase": phase}
        }))
    }

    fn namespace(name: &str, labels: &Value) -> Namespace {
        serde_json::from_value(json!({
            "metadata": {"name": name, "resourceVersion": "2", "labels": labels}
        }))
        .unwrap()
    }

    fn ingress(name: &str, namespace: &str, host: &str, labels: &Value) -> Ingress {
        serde_json::from_value(json!({
            "metadata": {"name": name, "namespace": namespace, "uid": format!("uid-{name}"),
                         "resourceVersion": "5", "labels": labels},
            "spec": {"rules": [{"host": host}]}
        }))
        .unwrap()
    }

    const KEY: &str = "kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk";

    fn gzip(data: &[u8]) -> Vec<u8> {
        use std::io::Write as _;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    /// Velero's archive of a backup that holds `routes`, at the paths Velero writes.
    fn contents(routes: &[Value]) -> Vec<u8> {
        let mut archive = tar::Builder::new(Vec::new());
        for route in routes {
            let namespace = route["metadata"]["namespace"].as_str().unwrap();
            let name = route["metadata"]["name"].as_str().unwrap();
            let body = serde_json::to_vec(route).unwrap();
            for folder in ["", "v1-preferredversion/"] {
                let mut header = tar::Header::new_gnu();
                header.set_size(body.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                archive
                    .append_data(
                        &mut header,
                        format!(
                            "resources/ingresses.networking.k8s.io/{folder}namespaces/{namespace}/{name}.json"
                        ),
                        body.as_slice(),
                    )
                    .unwrap();
            }
        }
        gzip(&archive.into_inner().unwrap())
    }

    fn route(namespace: &str, name: &str, host: &str) -> Value {
        json!({"apiVersion": "networking.k8s.io/v1", "kind": "Ingress",
               "metadata": {"name": name, "namespace": namespace},
               "spec": {"rules": [{"host": host}]}})
    }

    /// The store of backup `zeabur-1`, which holds `shop` and `blog`, and Velero signing URLs
    /// to it.
    async fn backup_store(cluster: &MemoryCluster) -> tokio::task::JoinHandle<()> {
        let list = json!({
            "v1/Service": ["shop/api", "shop/postgres", "blog/web"],
            "v1/Secret": ["shop/postgres-app"],
            "batch/v1/CronJob": ["shop/nightly"],
            "networking.k8s.io/v1/Ingress": ["shop/web", "blog/web"],
            "apps/v1/Deployment": ["shop/web"],
            "v1/Namespace": ["blog", "shop"]
        });
        let files = vec![
            (
                "-resource-list.json.gz",
                gzip(&serde_json::to_vec(&list).unwrap()),
            ),
            (
                ".tar.gz",
                contents(&[
                    route("shop", "web", "shop.example.com"),
                    route("blog", "web", "blog.example.com"),
                ]),
            ),
        ];
        let location = store(KEY, files).await;
        let base = location.data["spec"]["config"]["s3Url"]
            .as_str()
            .unwrap()
            .to_owned();
        cluster.update(|s| {
            s.dynamic.retain(|o| {
                o.types
                    .as_ref()
                    .is_none_or(|t| t.kind != "BackupStorageLocation")
            });
            s.dynamic.push(location);
            s.secret_values.insert(
                "velero/velero-backup-key".to_owned(),
                [("key".to_owned(), KEY.as_bytes().to_vec())].into(),
            );
        });
        sign_downloads(cluster, base)
    }

    /// Velero, and the project `shop` with a route.
    fn with_shop() -> MemoryCluster {
        let cluster = velero(vec![
            location(),
            schedule(),
            backup("zeabur-1", "Completed"),
            backup("zeabur-2", "InProgress"),
        ]);
        let deployment: Deployment = serde_json::from_value(json!({
            "metadata": {"name": "web", "namespace": "shop"},
            "spec": {"selector": {}, "template": {"spec": {"containers": [{"name": "web"}]}}}
        }))
        .unwrap();
        cluster.update(|s| {
            s.namespaces = vec![
                namespace(
                    "shop",
                    &json!({"zeabur.com/project": "true",
                           "pod-security.kubernetes.io/enforce": "baseline"}),
                ),
                namespace("other", &json!({})),
            ];
            s.objects.insert(
                "shop".to_owned(),
                NamespaceObjects {
                    namespace: "shop".to_owned(),
                    deployments: vec![deployment],
                    ingresses: vec![ingress("web", "shop", "shop.example.com", &json!({}))],
                    ..NamespaceObjects::default()
                },
            );
        });
        cluster
    }

    fn strings(value: &Value) -> Vec<&str> {
        value
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect()
    }

    #[tokio::test]
    async fn a_backup_goes_into_a_new_project_while_the_original_runs() {
        let cluster = with_shop();
        let _velero = backup_store(&cluster).await;
        let app = app(&cluster);
        let request = json!({"op": "restore", "backup": "zeabur-1", "namespace": "shop", "into": "shop-copy"});
        let (planned, done) = applied(&app, &request).await;
        let copy = &planned["copy"];
        assert_eq!(copy["sameName"], false);
        assert_eq!(copy["into"], "shop-copy");
        // What the plan lists comes from the backup, not from the running project.
        assert_eq!(
            copy["inventory"],
            json!({"services": ["api", "postgres"], "secrets": ["postgres-app"],
                   "cronJobs": ["nightly"], "routes": ["Ingress/web"]})
        );

        let name = done["restore"].as_str().unwrap();
        let made = stored(&cluster, "Restore", name).unwrap();
        assert_eq!(
            made["spec"]["namespaceMapping"],
            json!({"shop": "shop-copy"})
        );
        assert_eq!(made["spec"]["includedNamespaces"], json!(["shop"]));
        let held = strings(&made["spec"]["excludedResources"]);
        assert!(held.contains(&"ingresses.networking.k8s.io"), "{held:?}");
        assert_eq!(
            made["spec"]["resourceModifier"],
            json!({"kind": "ConfigMap", "name": name})
        );
        let modifier = cluster
            .inspect(|s| {
                s.objects.get("velero").and_then(|o| {
                    o.config_maps
                        .iter()
                        .find(|c| c.metadata.name.as_deref() == Some(name))
                        .cloned()
                })
            })
            .flatten()
            .expect("the resource modifier");
        let rules = &modifier.data.as_ref().unwrap()["cronjobs.yaml"];
        assert!(rules.contains("groupResource: cronjobs.batch"), "{rules}");
        assert!(rules.contains("- shop\n"), "{rules}");
        let owner = &modifier.metadata.owner_references.as_ref().unwrap()[0];
        assert_eq!(
            (owner.kind.as_str(), owner.name.as_str(), Some(&owner.uid)),
            (
                "Restore",
                name,
                made["metadata"]["uid"].as_str().map(str::to_owned).as_ref()
            )
        );

        let labels = cluster
            .inspect(|s| {
                s.namespaces
                    .iter()
                    .find(|n| n.metadata.name.as_deref() == Some("shop-copy"))
                    .and_then(|n| n.metadata.labels.clone())
            })
            .flatten()
            .expect("the new project");
        assert_eq!(labels["zeabur.com/project"], "true");
        assert_eq!(labels["pod-security.kubernetes.io/enforce"], "baseline");
        let (_, found) = send(&app, "GET", "/api/v1/cluster/backups", None).await;
        assert_eq!(found["restores"][0]["into"], "shop-copy");

        for (request, status) in [
            (
                json!({"op": "restore", "backup": "zeabur-1", "namespace": "shop"}),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                json!({"op": "restore", "backup": "zeabur-1", "namespace": "shop", "into": "shop"}),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                json!({"op": "restore", "backup": "zeabur-1", "namespace": "shop", "into": "other"}),
                StatusCode::CONFLICT,
            ),
            (
                json!({"op": "restore", "backup": "zeabur-2", "namespace": "shop", "into": "x"}),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                json!({"op": "restore", "backup": "zeabur-1", "namespace": "payments", "into": "x"}),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
        ] {
            let (got, problem) = plan(&app, &request).await;
            assert_eq!(got, status, "{request} {problem}");
        }
    }

    #[tokio::test]
    async fn a_namespace_that_is_gone_comes_back_under_its_own_name_without_its_routes() {
        let cluster = velero(vec![
            location(),
            schedule(),
            backup("zeabur-1", "Completed"),
        ]);
        let _velero = backup_store(&cluster).await;
        let app = app(&cluster);
        let (planned, done) = applied(
            &app,
            &json!({"op": "restore", "backup": "zeabur-1", "namespace": "blog",
                    "into": "ignored"}),
        )
        .await;
        assert_eq!(planned["copy"]["sameName"], true);
        assert_eq!(planned["copy"]["into"], "blog");
        assert_eq!(
            planned["copy"]["inventory"]["routes"],
            json!(["Ingress/web"])
        );
        let kinds: Vec<&str> = planned["writes"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|w| w["kind"].as_str())
            .collect();
        assert_eq!(kinds, ["Restore"]);
        let made = stored(&cluster, "Restore", done["restore"].as_str().unwrap()).unwrap();
        assert_eq!(made["spec"]["includedNamespaces"], json!(["blog"]));
        assert!(
            strings(&made["spec"]["excludedResources"]).contains(&"ingresses.networking.k8s.io")
        );
        assert_eq!(made["spec"].get("resourceModifier"), None);
        assert_eq!(made["spec"].get("namespaceMapping"), None);
    }

    #[tokio::test]
    async fn domains_come_back_from_the_backup_unless_another_project_serves_them() {
        let cluster = with_shop();
        cluster.update(|s| {
            s.namespaces.push(namespace("shop-copy", &json!({})));
            s.namespaces.push(namespace("blog", &json!({})));
        });
        let _velero = backup_store(&cluster).await;
        let app = app(&cluster);

        let into_copy = json!({"op": "attachDomains", "backup": "zeabur-1", "namespace": "shop",
                               "to": "shop-copy"});
        let (status, planned) = plan(&app, &into_copy).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        assert_eq!(
            planned["held"],
            json!([{"host": "shop.example.com", "project": "shop"}])
        );
        assert_eq!(planned["writes"], json!([]));
        let (status, problem) = apply(&app, &planned).await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");

        let home = json!({"op": "attachDomains", "backup": "zeabur-1", "namespace": "blog",
                          "to": "blog"});
        let (planned, done) = applied(&app, &home).await;
        assert_eq!(planned["hosts"], json!(["blog.example.com"]));
        assert_eq!(planned.get("held"), None);
        let made = stored(&cluster, "Restore", done["restore"].as_str().unwrap()).unwrap();
        assert_eq!(
            made["spec"],
            json!({"backupName": "zeabur-1", "includedNamespaces": ["blog"],
                   "includedResources": ["ingresses.networking.k8s.io",
                                         "httproutes.gateway.networking.k8s.io"]})
        );
        let (_, found) = send(&app, "GET", "/api/v1/cluster/backups", None).await;
        let listed = found["restores"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == done["restore"])
            .unwrap();
        assert_eq!(listed["resources"][0], "ingresses.networking.k8s.io");

        let (status, problem) = plan(
            &app,
            &json!({"op": "attachDomains", "backup": "zeabur-1", "namespace": "blog",
                    "to": "missing"}),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    }

    #[tokio::test]
    async fn moving_domains_moves_each_route_and_leaves_one_a_tool_manages() {
        let cluster = with_shop();
        cluster.update(|s| {
            s.namespaces.push(namespace("shop-copy", &json!({})));
            let shop = s.objects.get_mut("shop").unwrap();
            shop.ingresses.push(ingress(
                "chart",
                "shop",
                "chart.example.com",
                &json!({"app.kubernetes.io/managed-by": "Helm"}),
            ));
        });
        let app = app(&cluster);
        let (planned, _) = applied(
            &app,
            &json!({"op": "moveDomains", "from": "shop", "to": "shop-copy"}),
        )
        .await;
        assert_eq!(planned["hosts"], json!(["shop.example.com"]));
        assert_eq!(planned["managed"], json!(["Ingress/chart"]));
        let routes = |namespace: &str| {
            cluster
                .inspect(|s| {
                    s.objects.get(namespace).map(|o| {
                        o.ingresses
                            .iter()
                            .filter_map(|i| i.metadata.name.clone())
                            .collect::<Vec<_>>()
                    })
                })
                .flatten()
                .unwrap_or_default()
        };
        assert_eq!(routes("shop"), ["chart"]);
        assert_eq!(routes("shop-copy"), ["web"]);

        let (status, problem) = plan(
            &app,
            &json!({"op": "moveDomains", "from": "shop", "to": "missing"}),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    }

    /// A store at 127.0.0.1 that answers, only with the customer key `key`, the body of the first
    /// of `files` whose suffix ends the path, and the location of it.
    async fn store(key: &'static str, files: Vec<(&'static str, Vec<u8>)>) -> DynamicObject {
        use axum::extract::Path;
        use axum::http::HeaderMap;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let files = std::sync::Arc::new(files);
        let app = Router::new().route(
            "/{*path}",
            axum::routing::get(
                move |Path(path): Path<String>, headers: HeaderMap| async move {
                    let body = files
                        .iter()
                        .find(|(suffix, _)| path.ends_with(suffix))
                        .map_or_default(|(_, body)| body.clone());
                    let sent = |name: &str| {
                        headers
                            .get(name)
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or_default()
                            .to_owned()
                    };
                    let encoded =
                        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, key);
                    if sent("x-amz-server-side-encryption-customer-key") == encoded
                        && sent("x-amz-server-side-encryption-customer-algorithm") == "AES256"
                        && !sent("x-amz-server-side-encryption-customer-key-md5").is_empty()
                    {
                        (StatusCode::OK, body)
                    } else {
                        (StatusCode::BAD_REQUEST, Vec::new())
                    }
                },
            ),
        );
        tokio::spawn(async move { axum::serve(listener, app).await });
        object(json!({
            "apiVersion": "velero.io/v1", "kind": "BackupStorageLocation",
            "metadata": {"name": "default", "namespace": "velero", "resourceVersion": "3"},
            "spec": {"provider": "aws", "default": true,
                     "objectStorage": {"bucket": "backups"},
                     "config": {"region": "us-east-1",
                                "s3Url": format!("http://127.0.0.1:{port}"),
                                "customerKeyEncryptionSecret": "velero-backup-key/key"}}
        }))
    }

    /// Plays Velero: signs a URL for every `DownloadRequest` at `base`.
    fn sign_downloads(cluster: &MemoryCluster, base: String) -> tokio::task::JoinHandle<()> {
        let cluster = cluster.clone();
        tokio::spawn(async move {
            loop {
                cluster.update(|s| {
                    for request in s.dynamic.iter_mut().filter(|o| {
                        o.types.as_ref().is_some_and(|t| t.kind == "DownloadRequest")
                    }) {
                        let target = &request.data["spec"]["target"];
                        let backup = target["name"].as_str().unwrap_or_default().to_owned();
                        let file = match target["kind"].as_str() {
                            Some("BackupResourceList") => "-resource-list.json.gz",
                            _ => ".tar.gz",
                        };
                        request.data["status"] = json!({
                            "phase": "Processed",
                            "downloadURL": format!("{base}/backups/backups/{backup}/{backup}{file}?X-Amz-Signature=x"),
                            "expiration": "2026-10-25T00:10:00Z"
                        });
                    }
                });
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
    }

    #[tokio::test]
    async fn a_backup_downloads_through_zeabur_with_its_key_and_no_url_reaches_the_page() {
        let location = store(KEY, vec![(".tar.gz", b"tarball".to_vec())]).await;
        let base = location.data["spec"]["config"]["s3Url"]
            .as_str()
            .unwrap()
            .to_owned();
        let cluster = velero(vec![location, schedule(), backup("zeabur-1", "Completed")]);
        cluster.update(|s| {
            s.secret_values.insert(
                "velero/velero-backup-key".to_owned(),
                [("key".to_owned(), KEY.as_bytes().to_vec())].into(),
            );
        });
        let velero = sign_downloads(&cluster, base.clone());
        let app = app(&cluster);
        let request = axum::http::Request::get("/api/v1/cluster/backups/zeabur-1/download")
            .body(axum::body::Body::empty())
            .unwrap();
        let response = tower::ServiceExt::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers()["content-disposition"]
                .to_str()
                .unwrap()
                .contains("zeabur-1.tar.gz")
        );
        let bytes = http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes();
        assert_eq!(&bytes[..], b"tarball");

        // A second download in the same second makes its own request.
        let request = axum::http::Request::get("/api/v1/cluster/backups/zeabur-1/download")
            .body(axum::body::Body::empty())
            .unwrap();
        let again = tower::ServiceExt::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(again.status(), StatusCode::OK);

        // A URL to another host is not fetched.
        velero.abort();
        assert!(velero.await.is_err_and(|e| e.is_cancelled()));
        let _other = sign_downloads(&cluster, "http://127.0.0.2:9".to_owned());
        let (status, problem) = send(
            &app,
            "GET",
            "/api/v1/cluster/backups/zeabur-1/download",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{problem}");
        assert!(
            !problem.to_string().contains("X-Amz-Signature"),
            "{problem}"
        );
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
    }
}
