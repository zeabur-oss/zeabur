//! Resource usage against an in-memory cluster: live usage from `PodMetrics`, history from a
//! metrics store reached through the service proxy (canned answers), and the metrics store
//! setting, kept in a ConfigMap written as the person.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use engawa_api::Settings;
    use engawa_core::problem::KubernetesStatus;
    use engawa_kube::memory::{MemoryCluster, State};
    use engawa_kube::{Identity, ServedResource, WriteBody};
    use serde_json::{Value, json};

    use crate::common::{app_with, send};

    /// The store the settings name by default.
    const STORE: &str = "observability/vmsingle-vm-victoria-metrics-k8s-stack:8428";

    fn cluster() -> MemoryCluster {
        MemoryCluster::new(State {
            identity: Identity {
                username: "alice".to_owned(),
                groups: vec!["developers".to_owned()],
            },
            namespaces: vec![
                serde_json::from_value(json!({"metadata": {"name": "shop"}})).unwrap(),
            ],
            ..State::default()
        })
    }

    /// Settings kept in the `engawa` namespace, as on the in-cluster server.
    fn kept() -> Settings {
        Settings {
            namespace: Some("engawa".to_owned()),
            ..Settings::default()
        }
    }

    async fn call(
        cluster: &MemoryCluster,
        settings: Settings,
        method: &str,
        uri: &str,
        body: Option<&Value>,
    ) -> (StatusCode, Value) {
        send(&app_with(cluster, settings), method, uri, body).await
    }

    async fn get(cluster: &MemoryCluster, uri: &str) -> Value {
        let (status, body) = call(cluster, Settings::default(), "GET", uri, None).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        body
    }

    fn pod_metrics(name: &str, containers: &Value) -> engawa_kube::DynamicObject {
        serde_json::from_value(json!({
            "apiVersion": "metrics.k8s.io/v1beta1", "kind": "PodMetrics",
            "metadata": {"name": name, "namespace": "shop"},
            "timestamp": format!("2026-10-16T08:00:0{}Z", name.len() % 10), "window": "15s",
            "containers": containers
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn usage_comes_per_pod_from_metrics_server_or_says_why_not() {
        let cluster = cluster();
        let usage = get(&cluster, "/api/v1/projects/shop/usage").await;
        assert_eq!(usage["state"], "not-installed");
        assert_eq!(usage["pods"], json!([]));

        cluster.update(|s| {
            s.served.push(ServedResource::readable(
                "metrics.k8s.io",
                "v1beta1",
                "pods",
                "PodMetrics",
                true,
            ));
            s.dynamic.push(pod_metrics(
                "web-7c9d8-abcde",
                &json!([
                    {"name": "web", "usage": {"cpu": "1234567n", "memory": "20480Ki"}},
                    {"name": "proxy", "usage": {"cpu": "2m", "memory": "4Mi"}}
                ]),
            ));
        });
        let usage = get(&cluster, "/api/v1/projects/shop/usage").await;
        assert_eq!(usage["state"], "available");
        assert_eq!(usage["id"], "projects/shop/usage");
        assert_eq!(usage["window"], "15s");
        let pod = &usage["pods"][0];
        assert_eq!(pod["pod"], "web-7c9d8-abcde");
        assert_eq!(pod["origin"]["kind"], "PodMetrics");
        // 1.234567m rounds up, as requests do; memory is the working set.
        assert_eq!(pod["containers"][0]["cpuMillicores"], 2);
        assert_eq!(pod["cpuMillicores"], 4);
        assert_eq!(pod["memoryBytes"], (20 << 20) + (4 << 20));

        cluster.update(|s| {
            s.forbidden_in_namespaces
                .insert("pods.metrics.k8s.io".to_owned());
        });
        let usage = get(&cluster, "/api/v1/projects/shop/usage").await;
        assert_eq!(usage["state"], "unreadable");
        assert_eq!(usage["unreadable"][0]["resource"], "pods.metrics.k8s.io");
        assert_eq!(usage["unreadable"][0]["kubernetes"]["code"], 403);
    }

    /// The query of a proxied range query, its `start`, `end` and `step`.
    fn range_query(path: &str) -> (String, i64, i64, i64) {
        let (_, query) = path.split_once('?').unwrap();
        let pairs: BTreeMap<String, String> = form_urlencoded::parse(query.as_bytes())
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        let number = |key: &str| pairs[key].parse::<i64>().unwrap();
        (
            pairs["query"].clone(),
            number("start"),
            number("end"),
            number("step"),
        )
    }

    /// A matrix: one series per pod, `value(i)` at the `i`-th point.
    fn matrix(path: &str, pods: &[&str], value: impl Fn(usize) -> f64) -> Value {
        let (_, start, end, step) = range_query(path);
        let points: Vec<Value> = (start..=end)
            .step_by(usize::try_from(step).unwrap())
            .enumerate()
            .map(|(i, t)| json!([t, value(i).to_string()]))
            .collect();
        let result: Vec<Value> = pods
            .iter()
            .map(|pod| json!({"metric": {"pod": pod}, "values": points}))
            .collect();
        json!({"status": "success", "data": {"resultType": "matrix", "result": result}})
    }

    /// A store with kube-state-metrics: two pods at a quarter core and 100 MiB, one of which
    /// restarted at its fourth point after an out-of-memory kill.
    #[expect(clippy::unnecessary_wraps, reason = "a `ProxyAnswer`, which may fail")]
    fn store(path: &str) -> Result<Value, KubernetesStatus> {
        if path == "/api/v1/status/buildinfo" {
            return Ok(json!({"status": "success", "data": {"version": "2.24.0"}}));
        }
        let (query, ..) = range_query(path);
        let pods = ["web-7c9d8-abcde", "web-7c9d8-fghij"];
        Ok(if query.contains("container_cpu_usage_seconds_total") {
            matrix(path, &pods, |_| 0.25)
        } else if query.contains("container_memory_working_set_bytes") {
            matrix(path, &pods, |_| 104_857_600.0)
        } else if query.contains("restarts_total") {
            matrix(path, &pods[..1], |i| if i == 3 { 1.0 } else { 0.0 })
        } else {
            matrix(path, &pods[..1], |i| if i >= 3 { 1.0 } else { 0.0 })
        })
    }

    /// The same store without kube-state-metrics: no restart counts at all.
    fn store_without_ksm(path: &str) -> Result<Value, KubernetesStatus> {
        let (query, ..) = range_query(path);
        if query.contains("kube_pod") {
            return Ok(
                json!({"status": "success", "data": {"resultType": "matrix", "result": []}}),
            );
        }
        store(path)
    }

    fn refused(_: &str) -> Result<Value, KubernetesStatus> {
        Err(KubernetesStatus::new(
            403,
            "Forbidden",
            "services \"vmsingle-vm-victoria-metrics-k8s-stack:8428\" is forbidden: User \"alice\" \
             cannot get resource \"services/proxy\"",
        ))
    }

    fn no_endpoints(_: &str) -> Result<Value, KubernetesStatus> {
        Err(KubernetesStatus::new(
            503,
            "ServiceUnavailable",
            "no endpoints available for service \"vmsingle-vm-victoria-metrics-k8s-stack:8428\"",
        ))
    }

    const HISTORY: &str = "/api/v1/projects/shop/services/deployment/web/history";

    #[tokio::test]
    async fn history_comes_from_the_store_as_the_person() {
        let cluster = cluster();
        let history = get(&cluster, HISTORY).await;
        assert_eq!(history["state"], "not-configured");
        assert_eq!(history["source"]["service"], STORE);

        cluster.update(|s| {
            s.proxied.insert(STORE.to_owned(), store);
        });
        let history = get(&cluster, &format!("{HISTORY}?range=6h")).await;
        assert_eq!(history["state"], "available", "{history}");
        assert_eq!(
            (history["range"].as_str(), history["step"].as_i64()),
            (Some("6h"), Some(60))
        );
        let times = history["times"].as_array().unwrap();
        assert_eq!(times.len(), 361);
        assert_eq!(history["cpu"][0]["pod"], "web-7c9d8-abcde");
        assert_eq!(history["cpu"][0]["values"][0], 250.0);
        assert_eq!(history["memory"][1]["values"][360], 104_857_600.0);
        assert_eq!(
            history["restarts"],
            json!([{"pod": "web-7c9d8-abcde", "at": times[3], "oomKilled": true}])
        );
        // Engawa's own queries, naming the project and the service's pods; the person's words
        // never reach the store.
        let asked = cluster.inspect(|s| s.proxy_requests.clone()).unwrap();
        let cpu = asked
            .iter()
            .find(|r| r.contains("container_cpu_usage_seconds_total"))
            .unwrap();
        let (query, ..) = range_query(cpu);
        assert!(query.contains("namespace=\"shop\""), "{query}");
        assert!(
            query.contains("pod=~\"web-[a-z0-9]{1,10}-[a-z0-9]{5}\""),
            "{query}"
        );

        cluster.update(|s| {
            s.proxied.insert(STORE.to_owned(), store_without_ksm);
        });
        let history = get(&cluster, HISTORY).await;
        assert_eq!(history["state"], "available");
        assert_eq!(history["times"].as_array().unwrap().len(), 241);
        assert!(history.get("restarts").is_none(), "unknown, not none");

        cluster.update(|s| {
            s.proxied.insert(STORE.to_owned(), refused);
        });
        let history = get(&cluster, HISTORY).await;
        assert_eq!(history["state"], "unreadable");
        assert_eq!(
            history["unreadable"][0]["resource"],
            "services/proxy (observability/vmsingle-vm-victoria-metrics-k8s-stack)"
        );
        assert_eq!(history["cpu"], json!([]));

        cluster.update(|s| {
            s.proxied.insert(STORE.to_owned(), no_endpoints);
        });
        let history = get(&cluster, HISTORY).await;
        assert_eq!(history["state"], "unavailable");
        assert!(
            history["message"]
                .as_str()
                .unwrap()
                .contains("no endpoints")
        );
    }

    #[tokio::test]
    async fn history_parameters_are_checked_before_the_store_is_asked() {
        let cluster = cluster();
        for uri in [
            format!("{HISTORY}?range=30d"),
            "/api/v1/projects/shop/services/service/web/history".to_owned(),
            "/api/v1/projects/shop/services/deployment/web%7C.*/history".to_owned(),
        ] {
            let (status, body) = call(&cluster, Settings::default(), "GET", &uri, None).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
        }
        assert_eq!(cluster.inspect(|s| s.proxy_requests.len()), Some(0));
    }

    #[tokio::test]
    async fn the_metrics_store_setting_is_kept_in_a_config_map_written_as_the_person() {
        let cluster = cluster();
        cluster.update(|s| {
            s.proxied.insert(STORE.to_owned(), store);
        });
        // On a person's computer there is nowhere to keep it.
        let (status, setting) = call(
            &cluster,
            Settings::default(),
            "GET",
            "/api/v1/settings/metrics-store",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(setting["service"], STORE);
        assert_eq!(setting["source"], "default");
        assert_eq!(setting["state"], "available");
        assert_eq!(setting["canChange"], "denied");
        assert!(setting.get("storedIn").is_none());
        let change = json!({"service": "monitoring/prometheus-operated:9090"});
        let (status, problem) = call(
            &cluster,
            Settings::default(),
            "PUT",
            "/api/v1/settings/metrics-store",
            Some(&change),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{problem}");
        assert!(
            problem["detail"]
                .as_str()
                .unwrap()
                .contains("ENGAWA_METRICS_SERVICE")
        );

        // In the cluster: the first change creates the ConfigMap, and history follows it.
        let (status, setting) = call(
            &cluster,
            kept(),
            "PUT",
            "/api/v1/settings/metrics-store",
            Some(&change),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{setting}");
        assert_eq!(setting["service"], "monitoring/prometheus-operated:9090");
        assert_eq!(setting["source"], "setting");
        assert_eq!(setting["default"], STORE);
        assert_eq!(setting["state"], "not-configured");
        assert_eq!(setting["storedIn"]["name"], "engawa-settings");
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].target.namespace, "engawa");
        assert!(matches!(writes[0].body, WriteBody::Create(_)));
        let (_, history) = call(&cluster, kept(), "GET", HISTORY, None).await;
        assert_eq!(
            history["source"]["service"],
            "monitoring/prometheus-operated:9090"
        );

        // `null` goes back to the default; the same change twice writes once.
        let reset = json!({"service": null});
        for _ in 0..2 {
            let (status, setting) = call(
                &cluster,
                kept(),
                "PUT",
                "/api/v1/settings/metrics-store",
                Some(&reset),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{setting}");
            assert_eq!(setting["source"], "default");
            assert_eq!(setting["state"], "available");
        }
        assert_eq!(cluster.inspect(|s| s.writes.len()), Some(2));

        let (status, problem) = call(
            &cluster,
            kept(),
            "PUT",
            "/api/v1/settings/metrics-store",
            Some(&json!({"service": "prometheus:9090"})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{problem}");

        // Who may change it is the cluster's decision.
        cluster.update(|s| {
            s.forbidden_writes
                .insert("ConfigMap/engawa-settings".to_owned());
        });
        let (status, problem) = call(
            &cluster,
            kept(),
            "PUT",
            "/api/v1/settings/metrics-store",
            Some(&change),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(
            problem["denied"],
            json!({"verb": "patch", "group": "", "resource": "configmaps",
                   "name": "engawa-settings", "namespace": "engawa"})
        );
    }
}
