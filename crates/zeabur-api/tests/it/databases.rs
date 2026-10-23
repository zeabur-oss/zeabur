//! A project's databases of MOCO, mariadb-operator and the Dragonfly Operator: their backup Jobs,
//! Dragonfly's master, and exporting a dump: refused without `pods/exec`, a failed dump as a
//! problem, and MOCO's password kept out of the command.

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

    use crate::common::{app, send};

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

    fn cluster() -> MemoryCluster {
        let served = [
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
        ]
        .into_iter()
        .map(|(group, version, resource, kind)| {
            ServedResource::readable(group, version, resource, kind, true)
        })
        .collect();
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
                "app.kubernetes.io/instance": "cache", "role": "replica"}),
            ),
            pod(
                "cache-1",
                &json!({"app.kubernetes.io/name": "dragonfly",
                "app.kubernetes.io/instance": "cache", "role": "master"}),
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
    async fn the_new_engines_with_their_backup_jobs_and_members() {
        let app = app(&cluster());
        let (status, answer) = send(&app, "GET", "/api/v1/projects/shop/databases", None).await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        let engines: Vec<(&str, &str)> = answer["operators"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| (o["engine"].as_str().unwrap(), o["state"].as_str().unwrap()))
            .collect();
        assert_eq!(
            engines,
            [
                ("postgresql", "not-installed"),
                ("mongodb", "not-installed"),
                ("mysql", "installed"),
                ("mariadb", "installed"),
                ("dragonfly", "installed")
            ]
        );
        let mysql = named(&answer, "mysql");
        assert_eq!(mysql["backups"]["runs"][0]["name"], "moco-backup-mysql-1");
        assert_eq!(mysql["pods"][0]["name"], "moco-mysql-0");
        assert_eq!(named(&answer, "maria")["pods"][0]["name"], "maria-0");
        assert_eq!(named(&answer, "cache")["primary"], "cache-1");
        assert!(!answer.to_string().contains(PASSWORD));
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
}
