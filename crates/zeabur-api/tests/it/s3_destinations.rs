//! S3 destinations: an administrator registers one through a plan and no answer shows its keys,
//! a project plan copies the keys into a Secret of the project that names its source, a person
//! who may not read the keys is refused, a Velero location takes them too, and new keys reach
//! every copy the person may change.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::Router;
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{NamespaceObjects, ServedResource, WriteBody};

    use crate::common::{app, send};

    const SECRET_KEY: &str = "very-secret-access-key";
    const BASE: &str = "/api/v1/cluster/s3-destinations";
    const NAMESPACE: &str = "zeabur-s3-destinations";

    fn cluster() -> MemoryCluster {
        let namespace: Namespace = serde_json::from_value(json!({"metadata": {"name": "shop",
            "labels": {"zeabur.com/project": "true"}}}))
        .unwrap();
        MemoryCluster::new(State {
            namespaces: vec![namespace],
            objects: BTreeMap::from([(
                "shop".to_owned(),
                NamespaceObjects {
                    namespace: "shop".to_owned(),
                    ..NamespaceObjects::default()
                },
            )]),
            served: [
                ("backupstoragelocations", "BackupStorageLocation"),
                ("schedules", "Schedule"),
                ("backups", "Backup"),
                ("podvolumebackups", "PodVolumeBackup"),
            ]
            .into_iter()
            .map(|(r, k)| ServedResource::readable("velero.io", "v1", r, k, true))
            .collect(),
            ..State::default()
        })
    }

    async fn applied(app: &Router, base: &str, request: &Value) -> (Value, Value) {
        let (status, planned) = send(app, "POST", &format!("{base}/plan"), Some(request)).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        let (status, done) = send(
            app,
            "POST",
            &format!("{base}/apply"),
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{done}");
        (planned, done)
    }

    async fn register(app: &Router) {
        let request = json!({"op": "save", "name": "r2", "endpoint": "https://r2.example.com/",
            "bucket": "backups", "region": "auto", "prefix": "/prod/",
            "accessKeyId": "AKIA1", "secretAccessKey": SECRET_KEY});
        let (planned, done) = applied(app, BASE, &request).await;
        assert!(!planned.to_string().contains(SECRET_KEY), "{planned}");
        assert_eq!(
            done["written"],
            json!([
                "Namespace/zeabur-s3-destinations",
                "ConfigMap/r2",
                "Secret/r2"
            ])
        );
    }

    fn copy(name: &str) -> Value {
        json!({"changes": [{"op": "copyS3Keys", "destination": "r2",
            "object": {"apiVersion": "v1", "kind": "Secret", "metadata": {"name": name},
                       "type": "Opaque", "stringData": {"RESTIC_PASSWORD": "pw"}},
            "accessKeyId": "ACCESS_KEY_ID", "secretAccessKey": "ACCESS_SECRET_KEY"}]})
    }

    #[tokio::test]
    async fn a_destination_is_registered_listed_without_keys_and_removed() {
        let cluster = cluster();
        let app = app(&cluster);
        register(&app).await;
        let (status, found) = send(&app, "GET", BASE, None).await;
        assert_eq!(status, StatusCode::OK, "{found}");
        assert!(!found.to_string().contains(SECRET_KEY), "{found}");
        let item = &found["items"][0];
        assert_eq!(
            (&item["name"], &item["endpoint"], &item["prefix"]),
            (
                &json!("r2"),
                &json!("https://r2.example.com"),
                &json!("prod")
            )
        );
        assert_eq!(found["canUseKeys"], "allowed");

        let request = json!({"op": "save", "name": "r2", "endpoint": "https://r2.example.com",
            "bucket": "backups-2"});
        let (planned, _) = applied(&app, BASE, &request).await;
        assert_eq!(planned["writes"].as_array().unwrap().len(), 1, "{planned}");
        let (_, done) = applied(&app, BASE, &json!({"op": "remove", "name": "r2"})).await;
        assert_eq!(done["written"], json!(["ConfigMap/r2", "Secret/r2"]));
    }

    #[tokio::test]
    async fn a_project_plan_copies_the_keys_and_names_the_source() {
        let cluster = cluster();
        let app = app(&cluster);
        register(&app).await;
        let (planned, _) = applied(&app, "/api/v1/projects/shop", &copy("db-backups-s3")).await;
        assert!(!planned.to_string().contains(SECRET_KEY), "{planned}");
        let values = cluster
            .inspect(|s| s.secret_values["shop/db-backups-s3"].clone())
            .unwrap();
        let text = |key: &str| String::from_utf8(values[key].clone()).unwrap();
        assert_eq!(
            (
                text("ACCESS_KEY_ID"),
                text("ACCESS_SECRET_KEY"),
                text("RESTIC_PASSWORD")
            ),
            ("AKIA1".to_owned(), SECRET_KEY.to_owned(), "pw".to_owned())
        );
        let (status, backups) = send(&app, "GET", "/api/v1/projects/shop/backups", None).await;
        assert_eq!(status, StatusCode::OK, "{backups}");
        assert_eq!(backups["keysFrom"], json!({"db-backups-s3": "r2"}));
    }

    #[tokio::test]
    async fn someone_who_may_not_read_the_keys_is_refused_with_the_read_named() {
        let cluster = cluster();
        let app = app(&cluster);
        register(&app).await;
        cluster.update(|s| {
            s.forbidden_namespaces.insert(NAMESPACE.to_owned());
        });
        let (status, problem) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/plan",
            Some(&copy("db-backups-s3")),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(problem["denied"]["resource"], "secrets");
        assert_eq!(problem["denied"]["namespace"], NAMESPACE);
    }

    #[tokio::test]
    async fn a_velero_location_takes_the_keys_of_a_destination() {
        let cluster = cluster();
        let app = app(&cluster);
        register(&app).await;
        let request = json!({"op": "location", "bucket": "backups", "prefix": "velero",
            "endpoint": "https://r2.example.com", "destination": "r2"});
        let (planned, _) = applied(&app, "/api/v1/cluster/backups", &request).await;
        assert!(!planned.to_string().contains(SECRET_KEY), "{planned}");
        let keys = cluster
            .inspect(|s| s.secret_values["velero/zeabur-backup-credentials"]["cloud"].clone())
            .unwrap();
        assert!(String::from_utf8(keys).unwrap().contains(SECRET_KEY));
        let (_, found) = send(&app, "GET", "/api/v1/cluster/backups", None).await;
        assert_eq!(found["keysFrom"], "r2", "{found}");
    }

    async fn copied_twice(app: &Router) {
        register(app).await;
        applied(app, "/api/v1/projects/shop", &copy("db-backups-s3")).await;
        let request = json!({"op": "location", "bucket": "backups", "prefix": "velero",
            "endpoint": "https://r2.example.com", "destination": "r2"});
        applied(app, "/api/v1/cluster/backups", &request).await;
    }

    fn rotate() -> Value {
        json!({"op": "save", "name": "r2", "endpoint": "https://r2.example.com",
            "bucket": "backups", "accessKeyId": "AKIA2", "secretAccessKey": "rotated-secret"})
    }

    #[tokio::test]
    async fn new_keys_reach_every_copy() {
        let cluster = cluster();
        let app = app(&cluster);
        copied_twice(&app).await;
        let (planned, _) = applied(&app, BASE, &rotate()).await;
        assert!(!planned.to_string().contains("rotated-secret"), "{planned}");
        assert_eq!(planned["keptCopies"], json!([]), "{planned}");
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        let written = |namespace: &str, name: &str| {
            writes
                .iter()
                .rev()
                .find(|w| {
                    (w.target.namespace.as_str(), w.target.name.as_str()) == (namespace, name)
                })
                .and_then(|w| {
                    let WriteBody::JsonPatch(ops) = &w.body else {
                        return None;
                    };
                    ops.as_array()?
                        .iter()
                        .find(|op| op["path"] == "/stringData")
                        .map(|op| op["value"].clone())
                })
                .unwrap()
        };
        assert_eq!(
            written("shop", "db-backups-s3"),
            json!({"ACCESS_KEY_ID": "AKIA2", "ACCESS_SECRET_KEY": "rotated-secret"})
        );
        let file = written("velero", "zeabur-backup-credentials");
        assert!(
            file["cloud"]
                .as_str()
                .unwrap()
                .contains("aws_access_key_id=AKIA2")
        );
    }

    #[tokio::test]
    async fn a_plan_names_the_copies_it_cannot_change() {
        let cluster = cluster();
        let app = app(&cluster);
        copied_twice(&app).await;
        cluster.update(|s| {
            s.forbidden_namespaces.insert("velero".to_owned());
        });
        let (status, planned) = send(&app, "POST", &format!("{BASE}/plan"), Some(&rotate())).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        assert_eq!(
            planned["keptCopies"],
            json!([{"namespace": "velero", "name": "zeabur-backup-credentials",
                    "reason": "denied"}])
        );
        cluster.update(|s| {
            s.forbidden_resources.insert("secrets".to_owned());
        });
        let (_, planned) = send(&app, "POST", &format!("{BASE}/plan"), Some(&rotate())).await;
        assert_eq!(planned["copiesUnknown"], true, "{planned}");
    }
}
