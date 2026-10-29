//! A service's usage history through Zeabur Proxy: the person's token and the project's
//! namespace reach it, the store itself is not asked, and a refusal names the right.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::http::StatusCode;
    use axum::http::request::Parts;
    use secrecy::{ExposeSecret as _, SecretString};
    use serde_json::{Value, json};
    use zeabur_api::access::{ClusterAccess, Handoff};
    use zeabur_api::services::proxy::{ProxyAnswer, ZeaburProxy};
    use zeabur_core::problem::{KubernetesStatus, Problem};
    use zeabur_kube::Cluster;
    use zeabur_kube::k8s_openapi::jiff::{SignedDuration, Timestamp};
    use zeabur_kube::memory::{MemoryCluster, State};

    use crate::common::send;

    const TOKEN: &str = "eyJhbGciOi.token-of-dana.sig";
    const NOW: i64 = 1_800_000_000;
    const STORE: &str = "observability/vmsingle-vm-victoria-metrics-k8s-stack:8428";

    fn now() -> Timestamp {
        Timestamp::from_second(NOW).unwrap()
    }

    /// The proxy: what it was asked, and its answer to a path.
    struct Fake {
        answer: fn(&str) -> ProxyAnswer,
        asked: Mutex<Vec<(String, String)>>,
    }

    impl ZeaburProxy for Fake {
        fn get<'a>(
            &'a self,
            token: &'a zeabur_kube::UserToken,
            path_and_query: String,
        ) -> Pin<Box<dyn Future<Output = Result<ProxyAnswer, String>> + Send + 'a>> {
            let answer = (self.answer)(&path_and_query);
            self.asked
                .lock()
                .unwrap()
                .push((token.secret().expose_secret().to_owned(), path_and_query));
            Box::pin(std::future::ready(Ok(answer)))
        }
    }

    struct Signed(Arc<dyn Cluster>);

    impl ClusterAccess for Signed {
        fn for_request(&self, _parts: &Parts) -> Result<Arc<dyn Cluster>, Box<Problem>> {
            Ok(Arc::clone(&self.0))
        }

        fn handoff(&self, _parts: &Parts) -> Option<Handoff> {
            Some(Handoff {
                token: zeabur_kube::UserToken::new(SecretString::from(TOKEN)).unwrap(),
                expires_at: now() + SignedDuration::from_mins(10),
            })
        }
    }

    fn params(path: &str) -> BTreeMap<String, String> {
        let (_, query) = path.split_once('?').unwrap();
        form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect()
    }

    /// One series for one pod at a quarter core (or 100 MiB) at every point asked for.
    fn matrix(path: &str) -> Value {
        let p = params(path);
        let number = |key: &str| p[key].parse::<i64>().unwrap();
        let points: Vec<Value> = (number("start")..=number("end"))
            .step_by(usize::try_from(number("step")).unwrap())
            .map(|t| json!([t, "0.25"]))
            .collect();
        let result = if p["query"].contains("kube_pod") {
            json!([])
        } else {
            json!([{"metric": {"pod": "web-7c9d8-abcde"}, "values": points}])
        };
        json!({"status": "success", "data": {"resultType": "matrix", "result": result}})
    }

    fn json_answer(status: u16, body: &Value) -> ProxyAnswer {
        ProxyAnswer {
            status,
            body: body.to_string().into_bytes(),
        }
    }

    /// Dana is a developer of `shop` only, as the proxy's access review would say.
    fn developer_of_shop(path: &str) -> ProxyAnswer {
        if params(path)["namespace"] == "shop" {
            json_answer(200, &matrix(path))
        } else {
            json_answer(
                403,
                &json!({"code": "forbidden", "detail": "you may not read this in this namespace"}),
            )
        }
    }

    fn just_created(_: &str) -> ProxyAnswer {
        json_answer(
            400,
            &json!({"code": "before_namespace", "detail": "the range ends before the namespace was created"}),
        )
    }

    fn no_metrics_backend(_: &str) -> ProxyAnswer {
        json_answer(404, &json!({"code": "unknown_backend"}))
    }

    fn store_down(_: &str) -> ProxyAnswer {
        json_answer(
            502,
            &json!({"code": "backend", "detail": "the backend did not answer"}),
        )
    }

    fn store_missing(_: &str) -> Result<Value, KubernetesStatus> {
        Err(KubernetesStatus::new(
            404,
            "NotFound",
            "services \"vmsingle-vm-victoria-metrics-k8s-stack\" not found",
        ))
    }

    /// The store itself refuses Dana, who may not use `services/proxy`.
    fn store_refuses(_: &str) -> Result<Value, KubernetesStatus> {
        Err(KubernetesStatus::new(
            403,
            "Forbidden",
            "User \"dana\" cannot get resource \"services/proxy\"",
        ))
    }

    fn app(answer: fn(&str) -> ProxyAnswer) -> (Router, Arc<Fake>, MemoryCluster) {
        let fake = Arc::new(Fake {
            answer,
            asked: Mutex::new(Vec::new()),
        });
        let memory = MemoryCluster::new(State::default());
        memory.update(|s| {
            s.proxied.insert(STORE.to_owned(), store_refuses);
        });
        let cluster: Arc<dyn Cluster> = Arc::new(memory.clone());
        let proxy: Arc<dyn ZeaburProxy> = Arc::<Fake>::clone(&fake);
        (
            zeabur_api::router_with_zeabur_proxy(Arc::new(Signed(cluster)), now, proxy),
            fake,
            memory,
        )
    }

    fn history(project: &str) -> String {
        format!("/api/v1/projects/{project}/services/deployment/web/history")
    }

    #[tokio::test]
    async fn a_developer_reads_their_projects_history_through_the_proxy() {
        let (app, fake, store) = app(developer_of_shop);
        let (status, body) = send(&app, "GET", &history("shop"), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["state"], "available", "{body}");
        assert_eq!(body["source"]["via"], "zeabur-proxy");
        assert_eq!(body["cpu"][0]["pod"], "web-7c9d8-abcde");
        assert_eq!(body["cpu"][0]["values"][0], 250.0);
        assert!(body.get("restarts").is_none(), "{body}");
        let asked = fake.asked.lock().unwrap().clone();
        for (token, path) in &asked {
            assert_eq!(token, TOKEN);
            let (route, _) = path.split_once('?').unwrap();
            assert_eq!(route, "/metrics/api/v1/query_range");
            assert_eq!(params(path)["namespace"], "shop");
        }
        assert!(
            asked.iter().any(
                |(_, path)| params(path)["query"].contains("container_cpu_usage_seconds_total")
            ),
            "{asked:?}"
        );
        assert_eq!(store.inspect(|s| s.proxy_requests.len()), Some(0));
    }

    /// A project created moments ago, maybe under the name of a deleted one, has no history yet.
    #[tokio::test]
    async fn a_new_project_has_an_empty_history() {
        let (app, ..) = app(just_created);
        let (status, body) = send(&app, "GET", &history("shop"), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["state"], "available", "{body}");
        assert_eq!(body["cpu"], json!([]));
        assert_eq!(body["memory"], json!([]));
    }

    #[tokio::test]
    async fn another_projects_history_is_refused_with_the_right_named() {
        let (app, _, store) = app(developer_of_shop);
        let (status, body) = send(&app, "GET", &history("bank"), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["state"], "unreadable", "{body}");
        assert_eq!(body["source"]["via"], "zeabur-proxy");
        assert_eq!(
            body["unreadable"][0]["resource"],
            "metrics.proxy.zeabur.com (bank)"
        );
        assert_eq!(body["cpu"], json!([]));
        assert_eq!(body["memory"], json!([]));
        assert_eq!(store.inspect(|s| s.proxy_requests.len()), Some(0));
    }

    #[tokio::test]
    async fn without_metrics_in_the_proxy_the_store_is_read_as_the_person() {
        let (app, _, store) = app(no_metrics_backend);
        let (status, body) = send(&app, "GET", &history("shop"), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["state"], "unreadable", "{body}");
        assert_eq!(body["source"]["via"], "service-proxy");
        assert_eq!(
            body["unreadable"][0]["resource"],
            "services/proxy (observability/vmsingle-vm-victoria-metrics-k8s-stack)"
        );
        assert!(store.inspect(|s| s.proxy_requests.len()) > Some(0));
    }

    #[tokio::test]
    async fn when_the_proxy_cannot_reach_the_store_who_may_read_it_learns_why() {
        let (app, _, store) = app(store_down);
        let (_, body) = send(&app, "GET", &history("shop"), None).await;
        assert_eq!(body["state"], "unavailable", "{body}");
        assert_eq!(body["source"]["via"], "zeabur-proxy");
        assert_eq!(body["message"], "the backend did not answer");
        assert_eq!(body["unreadable"], json!([]));

        store.update(|s| {
            s.proxied.insert(STORE.to_owned(), store_missing);
        });
        let (_, body) = send(&app, "GET", &history("shop"), None).await;
        assert_eq!(body["state"], "not-configured", "{body}");
        assert_eq!(body["source"]["via"], "service-proxy");
    }
}
