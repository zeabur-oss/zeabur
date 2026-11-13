//! S3 destinations: an administrator registers one through a plan and no answer shows its keys,
//! a project plan copies the keys into a Secret of the project that names its source, or into
//! one that exists, keeping its other keys, a person
//! who may not read the keys is refused, a Velero location takes them too, and new keys reach
//! every copy the person may change.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::Router;
    use axum::http::StatusCode;
    use base64::Engine as _;
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
    async fn copying_into_a_secret_that_exists_replaces_its_keys_and_keeps_the_rest() {
        let cluster = cluster();
        let app = app(&cluster);
        register(&app).await;
        let typed = json!({"changes": [{"op": "create", "object": {"apiVersion": "v1",
            "kind": "Secret", "metadata": {"name": "db-backups-s3"}, "type": "Opaque",
            "stringData": {"ACCESS_KEY_ID": "old", "ACCESS_SECRET_KEY": "old-secret",
                           "REGION": "eu-west-1"}}}]});
        applied(&app, "/api/v1/projects/shop", &typed).await;
        let mut request = copy("db-backups-s3");
        request["changes"][0]["object"]["stringData"] = json!({});
        let (planned, _) = applied(&app, "/api/v1/projects/shop", &request).await;
        assert!(!planned.to_string().contains(SECRET_KEY), "{planned}");
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        let Some(WriteBody::JsonPatch(ops)) = writes.last().map(|w| &w.body) else {
            panic!("no patch of the Secret: {writes:?}");
        };
        let set: BTreeMap<&str, &Value> = ops
            .as_array()
            .unwrap()
            .iter()
            .filter(|op| op["op"] != "test")
            .map(|op| (op["path"].as_str().unwrap(), &op["value"]))
            .collect();
        let encoded = |text: &str| json!(base64::engine::general_purpose::STANDARD.encode(text));
        assert_eq!(set["/data/ACCESS_KEY_ID"], &encoded("AKIA1"));
        assert_eq!(set["/data/ACCESS_SECRET_KEY"], &encoded(SECRET_KEY));
        assert!(!set.contains_key("/data/REGION"), "{ops}");
        assert_eq!(
            set["/metadata/labels"],
            &json!({"zeabur.com/s3-destination": "r2"})
        );
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
    async fn typing_other_keys_into_a_copy_keeps_a_rotation_from_replacing_them() {
        let cluster = cluster();
        let app = app(&cluster);
        copied_twice(&app).await;
        let typed = json!({"changes": [{"op": "data", "kind": "Secret",
            "name": "db-backups-s3", "changes": [
                {"op": "set", "key": "ACCESS_KEY_ID", "value": "mine"},
                {"op": "set", "key": "ACCESS_SECRET_KEY", "value": "my-secret"}]}]});
        applied(&app, "/api/v1/projects/shop", &typed).await;
        let ops = last_patch(&cluster);
        let encoded = |text: &str| json!(base64::engine::general_purpose::STANDARD.encode(text));
        assert_eq!(ops["/data/ACCESS_KEY_ID"], encoded("mine"), "{ops:?}");
        assert_eq!(
            ops["/data/ACCESS_SECRET_KEY"],
            encoded("my-secret"),
            "{ops:?}"
        );
        assert_eq!(
            ops["/metadata/labels/zeabur.com~1s3-destination"],
            Value::Null,
            "{ops:?}"
        );
        assert_eq!(ops["/metadata/annotations"], Value::Null, "{ops:?}");
    }

    #[tokio::test]
    async fn typing_into_a_copy_plans_without_the_values_of_its_other_keys() {
        let cluster = cluster();
        let app = app(&cluster);
        copied_twice(&app).await;
        let typed = json!({"changes": [{"op": "data", "kind": "Secret",
            "name": "db-backups-s3", "changes": [
                {"op": "set", "key": "ACCESS_KEY_ID", "value": "mine"}]}]});
        let (status, planned) =
            send(&app, "POST", "/api/v1/projects/shop/plan", Some(&typed)).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        let text = planned.to_string();
        let encoded = |value: &str| base64::engine::general_purpose::STANDARD.encode(value);
        for value in [SECRET_KEY, "pw"] {
            assert!(!text.contains(&encoded(value)), "{value} leaked: {planned}");
        }
        assert!(!text.contains(SECRET_KEY), "{planned}");
        let after = &planned["objects"][0]["after"];
        assert_eq!(after["metadata"]["labels"], json!({}), "{planned}");
        for key in ["ACCESS_SECRET_KEY", "RESTIC_PASSWORD"] {
            assert_eq!(after["data"][key], "(hidden)", "{planned}");
        }
    }

    fn last_patch(cluster: &MemoryCluster) -> BTreeMap<String, Value> {
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        let Some(WriteBody::JsonPatch(ops)) = writes.last().map(|w| &w.body) else {
            panic!("no patch of the Secret: {writes:?}");
        };
        ops.as_array()
            .unwrap()
            .iter()
            .filter(|op| op["op"] != "test")
            .map(|op| (op["path"].as_str().unwrap().to_owned(), op["value"].clone()))
            .collect()
    }

    #[tokio::test]
    async fn choosing_the_same_destination_again_leaves_the_other_changes_planned() {
        let cluster = cluster();
        let app = app(&cluster);
        register(&app).await;
        applied(&app, "/api/v1/projects/shop", &copy("db-backups-s3")).await;
        let mut request = copy("db-backups-s3");
        request["changes"].as_array_mut().unwrap().push(json!({
            "op": "data", "kind": "Secret", "name": "db-backups-s3",
            "changes": [{"op": "set", "key": "REGION", "value": "eu-west-1"}]}));
        let (planned, _) = applied(&app, "/api/v1/projects/shop", &request).await;
        assert!(!planned.to_string().contains(SECRET_KEY), "{planned}");
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        let Some(WriteBody::MergePatch(patch)) = writes.last().map(|w| &w.body) else {
            panic!("no merge patch of the Secret: {writes:?}");
        };
        assert_eq!(
            patch,
            &json!({"data": {"REGION": base64::engine::general_purpose::STANDARD.encode("eu-west-1")}})
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
