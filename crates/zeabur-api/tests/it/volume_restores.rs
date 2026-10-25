//! Restoring a volume from its K8up backups: only the snapshots of the claim are offered, a
//! restore carries the backup's backend and Pod security.

#[cfg(test)]
mod tests {
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
        assert_eq!(body["restores"][0]["into"], "data-restored");
        assert_eq!(body["restores"][0]["outcome"], "failed");
        assert!(!body.to_string().contains(SECRET_KEY));
    }
}
