//! Restoring a volume from its K8up backups: only the snapshots of the claim are offered, a
//! restore carries the backup's backend and Pod security, and an export downloads through
//! Zeabur with requests signed with the bucket's keys, which never reach the page.

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::extract::{Path, RawQuery};
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{DynamicObject, ServedResource};

    use crate::common::{app, send};

    const SECRET_KEY: &str = "restic-bucket-secret";

    fn object(value: Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    fn schedule(endpoint: &str) -> DynamicObject {
        object(json!({
            "apiVersion": "k8up.io/v1", "kind": "Schedule",
            "metadata": {"name": "data-backup", "namespace": "shop"},
            "spec": {
                "backend": {
                    "repoPasswordSecretRef": {"name": "data-backup", "key": "RESTIC_PASSWORD"},
                    "s3": {"endpoint": endpoint, "bucket": "backups/shop/data",
                           "accessKeyIDSecretRef": {"name": "data-backup", "key": "AWS_ACCESS_KEY_ID"},
                           "secretAccessKeySecretRef": {"name": "data-backup", "key": "AWS_SECRET_ACCESS_KEY"}}
                },
                "podSecurityContext": {"runAsUser": 1000, "fsGroup": 1000},
                "backup": {"schedule": "0 3 * * *",
                           "labelSelectors": [{"matchLabels": {"zeabur.com/backup-volume": "data"}}]}
            }
        }))
    }

    fn snapshot(id: &str, date: &str, repository: &str, path: &str) -> DynamicObject {
        object(json!({
            "apiVersion": "k8up.io/v1", "kind": "Snapshot",
            "metadata": {"name": id.get(..8).unwrap(), "namespace": "shop"},
            "spec": {"id": id, "date": date, "paths": [path], "repository": repository}
        }))
    }

    fn restore(name: &str, method: &Value, reason: &str) -> DynamicObject {
        object(json!({
            "apiVersion": "k8up.io/v1", "kind": "Restore",
            "metadata": {"name": name, "namespace": "shop",
                         "labels": {"zeabur.com/backup-volume": "data"},
                         "creationTimestamp": "2026-10-25T04:00:00Z"},
            "spec": {"snapshot": "aaaaaaaa1111", "restoreMethod": method},
            "status": {"conditions": [{"type": "Completed", "status": "True", "reason": reason,
                                       "lastTransitionTime": "2026-10-25T04:01:00Z"}]}
        }))
    }

    fn k8up(dynamic: Vec<DynamicObject>) -> MemoryCluster {
        let served = [
            ("schedules", "Schedule"),
            ("backups", "Backup"),
            ("snapshots", "Snapshot"),
            ("restores", "Restore"),
        ]
        .into_iter()
        .map(|(resource, kind)| ServedResource::readable("k8up.io", "v1", resource, kind, true))
        .collect();
        let cluster = MemoryCluster::new(State {
            served,
            dynamic,
            ..State::default()
        });
        cluster.update(|s| {
            s.secret_values.insert(
                "shop/data-backup".to_owned(),
                [
                    ("AWS_ACCESS_KEY_ID".to_owned(), b"restic-bucket".to_vec()),
                    (
                        "AWS_SECRET_ACCESS_KEY".to_owned(),
                        SECRET_KEY.as_bytes().to_vec(),
                    ),
                ]
                .into(),
            );
        });
        cluster
    }

    /// A bucket that lists two exports of `data` and serves each, to signed requests only.
    async fn bucket() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let app = Router::new().route(
            "/{*path}",
            axum::routing::get(
                |Path(path): Path<String>, RawQuery(query): RawQuery| async move {
                    let query = query.unwrap_or_default();
                    let signed = query.contains("X-Amz-Signature=")
                        && query.contains("X-Amz-Credential=restic-bucket%2F")
                        && !query.contains(SECRET_KEY);
                    if !signed {
                        return (StatusCode::FORBIDDEN, String::new());
                    }
                    match path.as_str() {
                        "backups" if query.contains("prefix=backup-shop-data-") => (
                            StatusCode::OK,
                            "<ListBucketResult>\
                         <Contents><Key>backup-shop-data-2026-10-24T03:00:00Z.tar.gz</Key>\
                         <LastModified>2026-10-25T04:01:00.000Z</LastModified></Contents>\
                         <Contents><Key>backup-shop-data-2026-10-25T03:00:00Z.tar.gz</Key>\
                         <LastModified>2026-10-25T03:30:00.000Z</LastModified></Contents>\
                         </ListBucketResult>"
                                .to_owned(),
                        ),
                        "backups/backup-shop-data-2026-10-25T03:00:00Z.tar.gz" => {
                            (StatusCode::OK, "files of the 25th".to_owned())
                        }
                        _ => (StatusCode::NOT_FOUND, String::new()),
                    }
                },
            ),
        );
        tokio::spawn(async move { axum::serve(listener, app).await });
        format!("http://127.0.0.1:{port}")
    }

    #[tokio::test]
    async fn only_the_claims_own_snapshots_are_offered_and_a_restore_runs_like_its_backups() {
        let endpoint = "http://seaweedfs:8333";
        let repository = format!("s3:{endpoint}/backups/shop/data");
        let cluster = k8up(vec![
            schedule(endpoint),
            snapshot(
                "aaaaaaaa1111",
                "2026-10-24T03:00:00Z",
                &repository,
                "/data/data",
            ),
            snapshot(
                "bbbbbbbb2222",
                "2026-10-25T03:00:00Z",
                &repository,
                "/data/data",
            ),
            // Another claim, and another repository.
            snapshot(
                "cccccccc3333",
                "2026-10-25T03:00:00Z",
                &repository,
                "/data/cache",
            ),
            snapshot(
                "dddddddd4444",
                "2026-10-25T03:00:00Z",
                "s3:http://old/backups",
                "/data/data",
            ),
            restore(
                "data-into-copy",
                &json!({"folder": {"claimName": "data-restored"}}),
                "Failed",
            ),
        ]);
        let app = app(&cluster);
        let (status, body) = send(
            &app,
            "GET",
            "/api/v1/projects/shop/volumes/data/restores",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let ids: Vec<&str> = body["snapshots"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["bbbbbbbb2222", "aaaaaaaa1111"]);
        assert_eq!(body["schedule"], "data-backup");
        assert_eq!(
            body["template"]["spec"]["backend"]["s3"]["bucket"],
            "backups/shop/data"
        );
        assert_eq!(
            body["template"]["spec"]["podSecurityContext"]["runAsUser"],
            1000
        );
        assert_eq!(
            body["template"]["metadata"]["labels"]["zeabur.com/backup-volume"],
            "data"
        );
        // K8up joins the endpoint and the bucket; a bucket in the endpoint's path is not read.
        assert_eq!(body["export"]["s3"]["endpoint"], "http://seaweedfs:8333");
        assert_eq!(body["export"]["s3"]["bucket"], "backups");
        assert_eq!(body["restores"][0]["into"], "data-restored");
        assert_eq!(body["restores"][0]["outcome"], "failed");
        assert!(!body.to_string().contains(SECRET_KEY));
    }

    #[tokio::test]
    async fn an_export_downloads_through_zeabur_signed_with_the_buckets_keys() {
        let endpoint = bucket().await;
        let repository = format!("s3:{endpoint}/backups/shop/data");
        let export = |at: &str| {
            json!({"s3": {
            "endpoint": at, "bucket": "backups",
            "accessKeyIDSecretRef": {"name": "data-backup", "key": "AWS_ACCESS_KEY_ID"},
            "secretAccessKeySecretRef": {"name": "data-backup", "key": "AWS_SECRET_ACCESS_KEY"}}})
        };
        let mut exported = restore("data-export", &export(&endpoint), "Succeeded");
        exported.data["spec"]["snapshot"] = json!("bbbbbbbb2222");
        let cluster = k8up(vec![
            schedule(&endpoint),
            snapshot(
                "bbbbbbbb2222",
                "2026-10-25T03:00:00Z",
                &repository,
                "/data/data",
            ),
            exported,
            restore("data-elsewhere", &export("http://127.0.0.2:9"), "Succeeded"),
            restore(
                "data-into-copy",
                &json!({"folder": {"claimName": "data-restored"}}),
                "Succeeded",
            ),
        ]);
        let app = app(&cluster);
        let request = axum::http::Request::get(
            "/api/v1/projects/shop/volumes/data/restores/data-export/download",
        )
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
                .contains("backup-shop-data-2026-10-25T03")
        );
        let bytes = http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes();
        assert_eq!(&bytes[..], b"files of the 25th");

        // An export to another place, or a restore into a volume, has nothing to download here.
        for name in ["data-elsewhere", "data-into-copy"] {
            let (status, problem) = send(
                &app,
                "GET",
                &format!("/api/v1/projects/shop/volumes/data/restores/{name}/download"),
                None,
            )
            .await;
            assert_eq!(
                status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "{name}: {problem}"
            );
            assert!(
                !problem.to_string().contains("X-Amz-Signature"),
                "{problem}"
            );
        }
    }
}
