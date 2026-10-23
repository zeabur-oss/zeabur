//! Notification channels: not installed is an answer, a refused read is not "not installed", a
//! channel's URL stays in its Secret and out of every answer, and a channel is added, changed,
//! tested and removed through plans.

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_kube::ServedResource;
    use zeabur_kube::memory::{MemoryCluster, State};

    use crate::common::{app, send};

    const HOOK: &str = "https://hooks.slack.com/services/T0/B0/very-secret";
    const BASE: &str = "/api/v1/projects/shop/notifications";

    fn alerting() -> MemoryCluster {
        MemoryCluster::new(State {
            served: vec![
                ServedResource::readable(
                    "operator.victoriametrics.com",
                    "v1beta1",
                    "vmalertmanagerconfigs",
                    "VMAlertmanagerConfig",
                    true,
                ),
                ServedResource::readable(
                    "operator.victoriametrics.com",
                    "v1beta1",
                    "vmrules",
                    "VMRule",
                    true,
                ),
            ],
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

    fn add() -> Value {
        json!({"op": "add", "name": "ops", "events": ["deploy-failed", "crash-loop"],
               "settings": {"kind": "slack", "webhookUrl": HOOK}})
    }

    #[tokio::test]
    async fn without_alerting_the_answer_says_so_and_nothing_can_be_planned() {
        let app = app(&MemoryCluster::new(State::default()));
        let (status, found) = send(&app, "GET", BASE, None).await;
        assert_eq!(status, StatusCode::OK, "{found}");
        assert_eq!(found["state"], "not-installed");
        let (status, _) = send(&app, "POST", &format!("{BASE}/plan"), Some(&add())).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_refused_read_is_not_alerting_missing() {
        let cluster = alerting();
        cluster.update(|s| {
            s.forbidden_in_namespaces
                .insert("vmalertmanagerconfigs.operator.victoriametrics.com".to_owned());
        });
        let (status, found) = send(&app(&cluster), "GET", BASE, None).await;
        assert_eq!(status, StatusCode::OK, "{found}");
        assert_eq!(found["state"], "refused");
        assert_eq!(found["unreadable"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_channel_is_added_changed_tested_and_removed_and_its_url_never_shown() {
        let cluster = alerting();
        let app = app(&cluster);
        let (planned, done) = applied(&app, BASE, &add()).await;
        assert!(!planned.to_string().contains("very-secret"), "{planned}");
        assert!(!done.to_string().contains("very-secret"), "{done}");
        assert_eq!(
            done["written"],
            json!([
                "Secret/zeabur-notify-ops",
                "VMAlertmanagerConfig/zeabur-notify-ops"
            ])
        );
        let url = cluster
            .inspect(|s| s.secret_values["shop/zeabur-notify-ops"]["url"].clone())
            .unwrap();
        assert_eq!(String::from_utf8(url).unwrap(), HOOK);

        let (status, found) = send(&app, "GET", BASE, None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(!found.to_string().contains("very-secret"), "{found}");
        assert_eq!(found["state"], "installed");
        let channel = &found["channels"][0];
        assert_eq!(channel["name"], "ops");
        assert_eq!(channel["kind"], "slack");
        assert_eq!(channel["target"], Value::Null);
        assert_eq!(channel["status"], "waiting");
        assert_eq!(channel["events"], json!(["crash-loop", "deploy-failed"]));

        let (planned, _) = applied(
            &app,
            BASE,
            &json!({"op": "events", "name": "ops", "events": ["build-failed"]}),
        )
        .await;
        assert_eq!(planned["writes"][0]["action"], "patch");
        let (_, found) = send(&app, "GET", BASE, None).await;
        assert_eq!(found["channels"][0]["events"], json!(["build-failed"]));

        let (planned, _) = applied(&app, BASE, &json!({"op": "test", "name": "ops"})).await;
        let job = planned["writes"][0]["name"].as_str().unwrap();
        assert!(job.starts_with("zeabur-notify-test-ops-"), "{job}");
        assert!(
            planned["writes"][0]["afterYaml"]
                .as_str()
                .unwrap()
                .contains("zeabur.com/notification-test: ops")
        );

        let (_, done) = applied(&app, BASE, &json!({"op": "remove", "name": "ops"})).await;
        assert_eq!(
            done["written"],
            json!([
                "VMAlertmanagerConfig/zeabur-notify-ops",
                "Secret/zeabur-notify-ops"
            ])
        );
        let (_, found) = send(&app, "GET", BASE, None).await;
        assert_eq!(found["channels"], json!([]));
    }

    #[tokio::test]
    async fn a_plan_the_channel_moved_past_is_refused() {
        let cluster = alerting();
        let app = app(&cluster);
        applied(&app, BASE, &add()).await;
        let (_, planned) = send(
            &app,
            "POST",
            &format!("{BASE}/plan"),
            Some(&json!({"op": "remove", "name": "ops"})),
        )
        .await;
        applied(
            &app,
            BASE,
            &json!({"op": "events", "name": "ops", "events": ["build-failed"]}),
        )
        .await;
        let (status, problem) = send(
            &app,
            "POST",
            &format!("{BASE}/apply"),
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
    }

    #[tokio::test]
    async fn the_cluster_offers_cluster_events_and_a_project_cannot_take_them() {
        let app = app(&alerting());
        let (status, found) = send(&app, "GET", "/api/v1/cluster/notifications", None).await;
        assert_eq!(status, StatusCode::OK, "{found}");
        assert_eq!(found["namespace"], "observability");
        assert_eq!(found["rulesInstalled"], false);
        assert!(
            found["events"]
                .as_array()
                .unwrap()
                .contains(&json!("node-not-ready"))
        );
        let (status, _) = send(
            &app,
            "POST",
            &format!("{BASE}/plan"),
            Some(
                &json!({"op": "add", "name": "ops", "events": ["node-not-ready"],
                         "settings": {"kind": "slack", "webhookUrl": HOOK}}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }
}
