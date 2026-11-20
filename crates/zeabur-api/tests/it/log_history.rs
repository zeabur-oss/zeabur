//! A service's log history through Zeabur Proxy: the person's token and one namespace's query
//! reach it, and each of its answers becomes a state the interface can explain.

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::http::StatusCode;
    use secrecy::ExposeSecret as _;
    use serde_json::json;
    use zeabur_api::services::proxy::{ProxyAnswer, ZeaburProxy};
    use zeabur_kube::k8s_openapi::jiff::Timestamp;
    use zeabur_kube::memory::{MemoryCluster, State};

    use crate::common::send;
    use crate::support::TestApi;

    const TOKEN: &str = "eyJhbGciOi.token-of-alice.sig";
    const NOW: i64 = 1_800_000_000;

    fn now() -> Timestamp {
        Timestamp::from_second(NOW).unwrap()
    }

    /// The proxy: what it was asked, and a fixed answer.
    struct Fake {
        answer: Result<ProxyAnswer, String>,
        asked: Mutex<Vec<(String, String)>>,
    }

    impl ZeaburProxy for Fake {
        fn get<'a>(
            &'a self,
            token: &'a zeabur_kube::UserToken,
            path_and_query: String,
        ) -> Pin<Box<dyn Future<Output = Result<ProxyAnswer, String>> + Send + 'a>> {
            self.asked
                .lock()
                .unwrap()
                .push((token.secret().expose_secret().to_owned(), path_and_query));
            Box::pin(std::future::ready(self.answer.clone()))
        }
    }

    fn app(answer: Result<ProxyAnswer, String>) -> (Router, Arc<Fake>) {
        let fake = Arc::new(Fake {
            answer,
            asked: Mutex::new(Vec::new()),
        });
        let app = TestApi::new(&MemoryCluster::new(State::default()))
            .signed_in(TOKEN)
            .with_clock(now)
            .with_zeabur_proxy(Arc::<Fake>::clone(&fake))
            .router();
        (app, fake)
    }

    fn answer(status: u16, body: &str) -> ProxyAnswer {
        ProxyAnswer {
            status,
            body: body.as_bytes().to_vec(),
        }
    }

    const PATH: &str = "/api/v1/projects/shop/services/deployment/web/logs/history";

    #[tokio::test]
    async fn the_persons_token_asks_for_the_services_pods_in_the_project() {
        let later = json!({"_time":"2026-10-22T14:44:11Z","_msg":"later","kubernetes.pod_name":"web-6d9-abcde","kubernetes.container_name":"app"});
        let earlier = json!({"_time":"2026-10-22T14:44:10.5Z","_msg":"earlier","kubernetes.pod_name":"web-5c8-fghij","kubernetes.container_name":"app"});
        let lines = format!("{later}\n{earlier}\n");
        let (app, fake) = app(Ok(answer(200, &lines)));
        let (status, body) = send(
            &app,
            "GET",
            &format!("{PATH}?range=6h&container=app&limit=2"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body,
            json!({
                "id": "projects/shop/services/deployment/web/logs/history",
                "state": "available",
                "limit": 2,
                "range": "6h",
                "truncated": true,
                "lines": [
                    {"writtenAt": "2026-10-22T14:44:10.5Z", "pod": "web-5c8-fghij", "container": "app", "message": "earlier"},
                    {"writtenAt": "2026-10-22T14:44:11Z", "pod": "web-6d9-abcde", "container": "app", "message": "later"},
                ],
            })
        );
        let asked = fake.asked.lock().unwrap().clone();
        let [(token, path)] = asked.as_slice() else {
            panic!("{asked:?}")
        };
        assert_eq!(token, TOKEN);
        let (route, query) = path.split_once('?').unwrap();
        assert_eq!(route, "/logs/select/logsql/query");
        let params: Vec<(String, String)> = form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();
        assert_eq!(
            params,
            [
                ("namespace".to_owned(), "shop".to_owned()),
                (
                    "query".to_owned(),
                    r#"kubernetes.pod_name:~"^web-[a-z0-9]{1,10}-[a-z0-9]{5}$" kubernetes.container_name:="app""#
                        .to_owned()
                ),
                ("start".to_owned(), (NOW - 21_600).to_string()),
                ("limit".to_owned(), "2".to_owned()),
            ]
        );
    }

    #[tokio::test]
    async fn each_refusal_is_a_state_with_the_proxys_words() {
        for (answer, state, message) in [
            (
                Ok(answer(
                    403,
                    &json!({"code":"forbidden","detail":"You may not read logs in shop."})
                        .to_string(),
                )),
                "unreadable",
                Some("You may not read logs in shop."),
            ),
            (
                Ok(answer(
                    503,
                    &json!({"code":"unavailable","detail":"kube-apiserver did not answer."})
                        .to_string(),
                )),
                "unavailable",
                Some("kube-apiserver did not answer."),
            ),
            (
                Err("connection refused".to_owned()),
                "unavailable",
                Some("Zeabur Proxy did not answer: connection refused"),
            ),
        ] {
            let (app, _) = app(answer);
            let (status, body) = send(&app, "GET", PATH, None).await;
            assert_eq!(status, StatusCode::OK, "{body}");
            assert_eq!(
                body,
                json!({
                    "id": "projects/shop/services/deployment/web/logs/history",
                    "state": state,
                    "range": "1h",
                    "lines": [],
                    "limit": 1000,
                    "truncated": false,
                    "message": message,
                })
            );
        }
    }

    #[tokio::test]
    async fn without_a_proxy_or_a_token_to_pass_on_it_is_not_installed() {
        let cluster = MemoryCluster::new(State::default());
        for app in [
            TestApi::new(&cluster).signed_in(TOKEN).router(),
            TestApi::new(&cluster)
                .with_clock(now)
                .with_zeabur_proxy(Arc::new(Fake {
                    answer: Ok(answer(200, "")),
                    asked: Mutex::new(Vec::new()),
                }))
                .router(),
        ] {
            let (status, body) = send(&app, "GET", PATH, None).await;
            assert_eq!(status, StatusCode::OK, "{body}");
            assert_eq!(
                body,
                json!({
                    "id": "projects/shop/services/deployment/web/logs/history",
                    "state": "not-installed",
                    "range": "1h",
                    "lines": [],
                    "limit": 1000,
                    "truncated": false,
                })
            );
        }
    }

    #[tokio::test]
    async fn malformed_parameters_never_reach_the_proxy() {
        let (app, fake) = app(Ok(answer(200, "")));
        for path in [
            format!("{PATH}?range=1y"),
            format!("{PATH}?container=a%22%20or%20*"),
            format!("{PATH}?limit=5000"),
            "/api/v1/projects/Shop/services/deployment/web/logs/history".to_owned(),
            "/api/v1/projects/shop/services/service/web/logs/history".to_owned(),
            "/api/v1/projects/shop/services/deployment/we%22b/logs/history".to_owned(),
        ] {
            let (status, body) = send(&app, "GET", &path, None).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{path}: {body}");
        }
        assert!(fake.asked.lock().unwrap().is_empty());
    }
}
