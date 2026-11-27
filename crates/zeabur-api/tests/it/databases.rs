//! A project's databases of MOCO, mariadb-operator and the Dragonfly Operator: their backup Jobs,
//! Dragonfly's master, and exporting a dump: refused without `pods/exec`, a failed dump as a
//! problem, and MOCO's password kept out of the command. A `MongoDBCommunity` of MongoDB
//! Controllers for Kubernetes is unmanaged, and a refused read of it is named.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt as _;
    use serde_json::{Value, json};
    use tower::ServiceExt as _;
    use zeabur_kube::k8s_openapi::api::core::v1::Pod;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{DynamicObject, NamespaceObjects, ServedResource};
    use zeabur_telemetry::Feature;

    use crate::common::{app, send};
    use crate::support::{TestApi, recorder, taken};

    const PASSWORD: &str = "writable-password-3f9a";

    fn object(value: Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    fn pod(name: &str, labels: &Value) -> Pod {
        serde_json::from_value(json!({
            "metadata": {"name": name, "namespace": "shop", "labels": labels},
            "spec": {"containers": [{"name": "server", "image": "x"}]},
            "status": {"phase": "Running"}
        }))
        .unwrap()
    }

    /// A `MongoDBCommunity` made outside Zeabur, running, with one user.
    fn mongodb_community() -> DynamicObject {
        object(json!({
            "apiVersion": "mongodbcommunity.mongodb.com/v1", "kind": "MongoDBCommunity",
            "metadata": {"name": "events", "namespace": "shop"},
            "spec": {"members": 1, "type": "ReplicaSet", "version": "8.0.4",
                     "security": {"authentication": {"modes": ["SCRAM"]}},
                     "users": [{"name": "app", "db": "admin",
                                "passwordSecretRef": {"name": "app-password"},
                                "roles": [{"name": "readWrite", "db": "events"}],
                                "scramCredentialsSecretName": "app"}]},
            "status": {"phase": "Running", "version": "8.0.4", "mongoUri": "mongodb://x",
                       "currentMongoDBMembers": 1, "currentStatefulSetReplicas": 1}
        }))
    }

    /// The operators' resources this cluster serves.
    fn served() -> Vec<ServedResource> {
        [
            (
                "moco.cybozu.com",
                "v1beta2",
                "mysqlclusters",
                "MySQLCluster",
            ),
            (
                "moco.cybozu.com",
                "v1beta2",
                "backuppolicies",
                "BackupPolicy",
            ),
            ("k8s.mariadb.com", "v1alpha1", "mariadbs", "MariaDB"),
            ("k8s.mariadb.com", "v1alpha1", "backups", "Backup"),
            ("dragonflydb.io", "v1alpha1", "dragonflies", "Dragonfly"),
            ("batch", "v1", "jobs", "Job"),
            (
                "mongodbcommunity.mongodb.com",
                "v1",
                "mongodbcommunity",
                "MongoDBCommunity",
            ),
        ]
        .into_iter()
        .map(|(group, version, resource, kind)| {
            ServedResource::readable(group, version, resource, kind, true)
        })
        .collect()
    }

    fn cluster() -> MemoryCluster {
        let served = served();
        let dynamic = vec![
            object(json!({
                "apiVersion": "moco.cybozu.com/v1beta2", "kind": "MySQLCluster",
                "metadata": {"name": "mysql", "namespace": "shop"},
                "spec": {"replicas": 1, "backupPolicyName": "mysql-backups",
                         "podTemplate": {"spec": {"containers": [{"name": "mysqld", "image": "m:8.4.10"}]}},
                         "volumeClaimTemplates": [{"metadata": {"name": "mysql-data"}, "spec": {}}]},
                "status": {"conditions": [{"type": "Initialized", "status": "True"},
                                          {"type": "Available", "status": "True"},
                                          {"type": "Healthy", "status": "True"}],
                           "currentPrimaryIndex": 0, "syncedReplicas": 1}
            })),
            object(json!({
                "apiVersion": "moco.cybozu.com/v1beta2", "kind": "BackupPolicy",
                "metadata": {"name": "mysql-backups", "namespace": "shop"},
                "spec": {"schedule": "0 3 * * *", "jobConfig": {"bucketConfig": {"bucketName": "b"}}}
            })),
            object(json!({
                "apiVersion": "batch/v1", "kind": "Job",
                "metadata": {"name": "moco-backup-mysql-1", "namespace": "shop",
                             "ownerReferences": [{"apiVersion": "batch/v1", "kind": "CronJob",
                                                  "name": "moco-backup-mysql", "uid": "c", "controller": true}]},
                "status": {"startTime": "2026-10-22T03:00:00Z", "completionTime": "2026-10-22T03:01:00Z"}
            })),
            object(json!({
                "apiVersion": "k8s.mariadb.com/v1alpha1", "kind": "MariaDB",
                "metadata": {"name": "maria", "namespace": "shop"},
                "spec": {"image": "mariadb:11.8.3"},
                "status": {"conditions": [{"type": "Ready", "status": "True"}]}
            })),
            object(json!({
                "apiVersion": "dragonflydb.io/v1alpha1", "kind": "Dragonfly",
                "metadata": {"name": "cache", "namespace": "shop"},
                "spec": {"replicas": 2},
                "status": {"phase": "Ready"}
            })),
            mongodb_community(),
        ];
        let pods = vec![
            pod(
                "moco-mysql-0",
                &json!({"app.kubernetes.io/name": "mysql",
                "app.kubernetes.io/instance": "mysql", "app.kubernetes.io/created-by": "moco"}),
            ),
            pod(
                "maria-0",
                &json!({"app.kubernetes.io/name": "mariadb",
                "app.kubernetes.io/instance": "maria"}),
            ),
            pod(
                "cache-0",
                &json!({"app.kubernetes.io/name": "dragonfly",
                "app": "cache", "app.kubernetes.io/part-of": "cache", "role": "replica"}),
            ),
            pod("events-0", &json!({"app": "events-svc"})),
            pod(
                "cache-1",
                &json!({"app.kubernetes.io/name": "dragonfly",
                "app": "cache", "app.kubernetes.io/part-of": "cache", "role": "master"}),
            ),
        ];
        MemoryCluster::new(State {
            served,
            dynamic,
            objects: BTreeMap::from([(
                "shop".to_owned(),
                NamespaceObjects {
                    namespace: "shop".to_owned(),
                    pods,
                    ..NamespaceObjects::default()
                },
            )]),
            secret_values: BTreeMap::from([(
                "shop/moco-mysql".to_owned(),
                BTreeMap::from([("WRITABLE_PASSWORD".to_owned(), PASSWORD.as_bytes().to_vec())]),
            )]),
            ..State::default()
        })
    }

    fn named<'a>(answer: &'a Value, name: &str) -> &'a Value {
        answer["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["name"] == name)
            .unwrap()
    }

    #[tokio::test]
    async fn the_database_api_owns_the_available_image_actions() {
        let cluster = cluster();
        cluster.update(|state| {
            state.served.push(ServedResource::readable(
                "postgresql.cnpg.io",
                "v1",
                "clusters",
                "Cluster",
                true,
            ));
            for (name, labels, image) in [
                ("direct", json!({}), "postgres:17-standard"),
                (
                    "helm",
                    json!({"app.kubernetes.io/managed-by":"Helm"}),
                    "postgres:17-standard",
                ),
                (
                    "flux",
                    json!({"kustomize.toolkit.fluxcd.io/name":"git"}),
                    "postgres:17-standard",
                ),
                ("untagged", json!({}), "postgres:latest"),
            ] {
                state.dynamic.push(object(json!({
                    "apiVersion":"postgresql.cnpg.io/v1", "kind":"Cluster",
                    "metadata":{"namespace":"shop", "name":name, "labels":labels,
                        "annotations":{"keel.sh/policy":"force","keel.sh/trigger":"poll",
                            "keel.sh/match-tag":"true","keel.sh/pollSchedule":"0 0 3 * * 0"}},
                    "spec":{"imageName":image,"instances":1}
                })));
            }
        });
        let (status, answer) = send(
            &app(&cluster),
            "GET",
            "/api/v1/projects/shop/databases",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let setting = json!({"kind":"image","policy":"sameTag","window":{"day":"sunday","hour":3}});
        assert_eq!(
            named(&answer, "direct")["imageUpdates"],
            json!({"allowed":["sameTag"],"setting":setting})
        );
        assert_eq!(
            named(&answer, "untagged")["imageUpdates"],
            json!({"allowed":[],"setting":setting})
        );
        for name in ["helm", "flux", "mysql"] {
            assert!(named(&answer, name).get("imageUpdates").is_none(), "{name}");
        }
    }

    #[tokio::test]
    async fn the_new_engines_with_their_backup_jobs_and_members() {
        let app = app(&cluster());
        let (status, answer) = send(&app, "GET", "/api/v1/projects/shop/databases", None).await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        let engines: Vec<(&str, &str, Option<&str>)> = answer["operators"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| {
                (
                    o["module"].as_str().unwrap(),
                    o["state"].as_str().unwrap(),
                    o["component"]["id"].as_str(),
                )
            })
            .collect();
        assert_eq!(
            engines,
            [
                ("cloudnative-pg", "not-installed", Some("cloudnative-pg")),
                (
                    "percona-server-mongodb",
                    "not-installed",
                    Some("percona-mongodb")
                ),
                ("mongodb-kubernetes", "installed", None),
                ("moco", "installed", Some("moco")),
                ("mariadb-operator", "installed", Some("mariadb-operator")),
                (
                    "dragonfly-operator",
                    "installed",
                    Some("dragonfly-operator")
                )
            ]
        );
        let mysql = named(&answer, "mysql");
        assert_eq!(mysql["backups"]["runs"][0]["name"], "moco-backup-mysql-1");
        assert_eq!(mysql["pods"][0]["name"], "moco-mysql-0");
        assert_eq!(named(&answer, "maria")["pods"][0]["name"], "maria-0");
        assert_eq!(named(&answer, "cache")["primary"], "cache-1");
        for database in answer["items"].as_array().unwrap() {
            assert_eq!(
                format!(
                    "{}/{}",
                    database["apiGroup"].as_str().unwrap(),
                    database["apiGroupVersion"].as_str().unwrap()
                ),
                database["apiVersion"],
                "{database}"
            );
        }
        assert_eq!(mysql["apiGroup"], "moco.cybozu.com");
        assert_eq!(mysql["apiGroupVersion"], "v1beta2");
        assert!(!answer.to_string().contains(PASSWORD));
    }

    #[tokio::test]
    async fn a_mongodb_community_is_unmanaged_with_its_members_and_first_user() {
        let app = app(&cluster());
        let (_, answer) = send(&app, "GET", "/api/v1/projects/shop/databases", None).await;
        let operator = answer["operators"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["module"] == "mongodb-kubernetes")
            .unwrap();
        assert_eq!(operator["unmanaged"], true);
        let events = named(&answer, "events");
        assert_eq!(events["engine"], "mongodb");
        assert_eq!(events["unmanaged"], true);
        assert_eq!(events["state"], "ready");
        assert_eq!(events["pods"][0]["name"], "events-0");
        assert_eq!(events["connection"]["host"], "events-svc.shop.svc");
        assert_eq!(events["connection"]["secret"], "events-admin-app");
        assert!(named(&answer, "mysql").get("unmanaged").is_none());
    }

    #[tokio::test]
    async fn a_refused_read_of_mongodb_community_is_named_not_left_out() {
        let cluster = cluster();
        cluster.update(|s| {
            s.forbidden_in_namespaces
                .insert("mongodbcommunity.mongodbcommunity.mongodb.com".to_owned());
        });
        let (status, answer) = send(
            &app(&cluster),
            "GET",
            "/api/v1/projects/shop/databases",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        let operator = answer["operators"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["module"] == "mongodb-kubernetes")
            .unwrap();
        assert_eq!(operator["state"], "refused");
        assert!(
            answer["unreadable"]
                .as_array()
                .unwrap()
                .iter()
                .any(|u| u["resource"] == "mongodbcommunity.mongodbcommunity.mongodb.com"),
            "{answer}"
        );
        assert!(
            answer["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|d| d["name"] != "events")
        );
    }

    async fn export(
        app: &axum::Router,
        path: &str,
    ) -> (StatusCode, Vec<u8>, axum::http::HeaderMap) {
        let request = Request::get(format!("/api/v1/projects/shop/databases/{path}/export"))
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, bytes.to_vec(), headers)
    }

    #[tokio::test]
    async fn an_export_without_exec_is_refused_with_the_right_it_needs() {
        let cluster = cluster();
        cluster.update(|s| {
            s.exec_refused.insert("shop/maria-0".to_owned());
        });
        let (status, body, _) = export(&app(&cluster), "mariadb/maria").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let problem: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(problem["denied"]["resource"], "pods/exec", "{problem}");
        assert_eq!(problem["denied"]["name"], "maria-0");
    }

    #[tokio::test]
    async fn a_dump_that_fails_at_once_is_a_problem_and_keeps_the_password_out_of_the_command() {
        let cluster = cluster();
        let (status, body, _) = export(&app(&cluster), "mysql/mysql").await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(!String::from_utf8_lossy(&body).contains(PASSWORD));
        let streams = cluster.inspect(|s| s.streams.clone()).unwrap();
        let (place, command) = &streams[0];
        assert_eq!(place, "shop/moco-mysql-0/mysqld");
        assert!(command.iter().all(|word| !word.contains(PASSWORD)));
    }

    #[tokio::test]
    async fn only_engines_with_a_dump_tool_export() {
        let app = app(&cluster());
        assert_eq!(
            export(&app, "dragonfly/cache").await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(export(&app, "mysql/missing").await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_dump_that_finishes_is_sent_and_one_that_fails_is_not() {
        let cluster = cluster();
        let (used, seen) = recorder();
        let app = TestApi::new(&cluster).with_used(used).router();
        assert_eq!(export(&app, "mysql/mysql").await.0, StatusCode::CONFLICT);
        assert_eq!(taken(&seen), Vec::<Feature>::new());
        cluster.update(|s| {
            s.stream_output
                .insert("shop/maria-0".to_owned(), b"-- dump".to_vec());
        });
        let (status, body, _) = export(&app, "mariadb/maria").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, b"-- dump");
        assert_eq!(taken(&seen), [Feature::DatabaseExported]);
    }
}
