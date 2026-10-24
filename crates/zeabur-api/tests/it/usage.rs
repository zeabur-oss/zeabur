//! Resource usage against an in-memory cluster: live usage from `PodMetrics` per project and per
//! namespace across the cluster, history from a
//! metrics store reached through the service proxy (canned answers), disk use from the same
//! store, and the metrics store setting, kept in a ConfigMap written as the person.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_api::Settings;
    use zeabur_core::problem::KubernetesStatus;
    use zeabur_kube::k8s_openapi::api::core::v1::{Namespace, Pod};
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects, ServedResource, WriteBody};

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

    /// Settings kept in the `zeabur` namespace, as on the in-cluster server.
    fn kept() -> Settings {
        Settings {
            namespace: Some("zeabur".to_owned()),
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

    fn pod_metrics(name: &str, containers: &Value) -> zeabur_kube::DynamicObject {
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

    fn pod(namespace: &str, name: &str, phase: &str, resources: &Value) -> Pod {
        serde_json::from_value(json!({
            "metadata": {"name": name, "namespace": namespace},
            "spec": {"containers": [{"name": "app", "image": "app:1", "resources": resources}]},
            "status": {"phase": phase}
        }))
        .unwrap()
    }

    fn sample(namespace: &str, name: &str, cpu: &str, memory: &str) -> zeabur_kube::DynamicObject {
        serde_json::from_value(json!({
            "apiVersion": "metrics.k8s.io/v1beta1", "kind": "PodMetrics",
            "metadata": {"name": name, "namespace": namespace},
            "timestamp": "2026-10-16T08:00:00Z", "window": "15s",
            "containers": [{"name": "app", "usage": {"cpu": cpu, "memory": memory}}]
        }))
        .unwrap()
    }

    /// `shop` is a project with two running pods, one without a memory limit, and a finished
    /// one; `lab` is a namespace that is not a project; `kube-system` is Kubernetes' own.
    fn cluster_with_namespaces() -> MemoryCluster {
        let namespace = |name: &str, labels: Value| -> Namespace {
            serde_json::from_value(json!({"metadata": {"name": name, "labels": labels}})).unwrap()
        };
        let objects = |namespace: &str, pods: Vec<Pod>| NamespaceObjects {
            namespace: namespace.to_owned(),
            pods,
            ..NamespaceObjects::default()
        };
        let limited = json!({"requests": {"cpu": "250m", "memory": "256Mi"},
                             "limits": {"cpu": "1", "memory": "512Mi"}});
        let open = json!({"requests": {"cpu": "100m", "memory": "128Mi"}});
        MemoryCluster::new(State {
            identity: Identity {
                username: "alice".to_owned(),
                groups: vec![],
            },
            namespaces: vec![
                serde_json::from_value(json!({"metadata": {"name": "shop",
                    "labels": {"zeabur.com/project": "true"},
                    "annotations": {"zeabur.com/display-name": "Shop"}}}))
                .unwrap(),
                namespace("lab", json!({})),
                namespace("kube-system", json!({})),
            ],
            objects: BTreeMap::from([
                (
                    "shop".to_owned(),
                    objects(
                        "shop",
                        vec![
                            pod("shop", "web-1", "Running", &limited),
                            pod("shop", "worker-1", "Running", &open),
                            pod("shop", "migrate-1", "Succeeded", &limited),
                        ],
                    ),
                ),
                (
                    "lab".to_owned(),
                    objects("lab", vec![pod("lab", "toolbox", "Pending", &open)]),
                ),
                (
                    "kube-system".to_owned(),
                    objects(
                        "kube-system",
                        vec![pod("kube-system", "coredns-1", "Running", &open)],
                    ),
                ),
            ]),
            ..State::default()
        })
    }

    fn item<'a>(usage: &'a Value, namespace: &str) -> &'a Value {
        usage["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["namespace"] == namespace)
            .unwrap_or_else(|| panic!("{namespace} missing: {usage}"))
    }

    #[tokio::test]
    async fn cluster_usage_sums_each_namespace_and_says_what_it_is() {
        let cluster = cluster_with_namespaces();
        let usage = get(&cluster, "/api/v1/cluster/usage").await;
        assert_eq!(usage["metrics"], "not-installed");
        assert_eq!(usage["podScan"], "complete");
        let shop = item(&usage, "shop");
        assert_eq!(
            (&shop["adopted"], &shop["system"]),
            (&json!(true), &json!(false))
        );
        assert_eq!(shop["displayName"], "Shop");
        // The finished pod reserves nothing; the pod without limits adds none.
        assert_eq!(shop["pods"], 2);
        assert_eq!(
            shop["requests"],
            json!({"cpuMillicores": 350, "memoryBytes": 384 << 20})
        );
        assert_eq!(
            shop["limits"],
            json!({"cpuMillicores": 1000, "memoryBytes": 512 << 20})
        );
        assert_eq!(shop["unlimited"], json!({"cpu": 1, "memory": 1}));
        assert!(shop.get("usage").is_none(), "unknown, not zero: {shop}");
        let lab = item(&usage, "lab");
        assert_eq!(
            (&lab["adopted"], &lab["system"]),
            (&json!(false), &json!(false))
        );
        let system = item(&usage, "kube-system");
        assert_eq!(
            (&system["adopted"], &system["system"]),
            (&json!(false), &json!(true))
        );
        assert_eq!(usage["totals"]["requests"]["cpuMillicores"], 550);
        assert!(usage["totals"].get("usage").is_none());

        cluster.update(|s| {
            s.served.push(ServedResource::readable(
                "metrics.k8s.io",
                "v1beta1",
                "pods",
                "PodMetrics",
                true,
            ));
            s.dynamic.push(sample("shop", "web-1", "120m", "200Mi"));
            s.dynamic
                .push(sample("shop", "worker-1", "1500000n", "100Mi"));
            s.dynamic
                .push(sample("kube-system", "coredns-1", "3m", "20Mi"));
        });
        let usage = get(&cluster, "/api/v1/cluster/usage").await;
        assert_eq!(usage["metrics"], "available");
        assert_eq!(usage["window"], "15s");
        assert_eq!(
            item(&usage, "shop")["usage"],
            json!({"cpuMillicores": 122, "memoryBytes": 300 << 20, "pods": 2})
        );
        // Pending, so metrics-server has no sample of it.
        assert_eq!(item(&usage, "lab")["usage"]["pods"], 0);
        assert_eq!(
            usage["totals"]["usage"],
            json!({"cpuMillicores": 125, "memoryBytes": 320 << 20, "pods": 3})
        );

        cluster.update(|s| {
            s.forbidden_resources
                .insert("pods.metrics.k8s.io".to_owned());
        });
        let usage = get(&cluster, "/api/v1/cluster/usage").await;
        assert_eq!(usage["metrics"], "unreadable");
        assert_eq!(usage["unreadable"][0]["resource"], "pods.metrics.k8s.io");
        assert_eq!(item(&usage, "shop")["requests"]["cpuMillicores"], 350);
        assert!(item(&usage, "shop").get("usage").is_none());
    }

    #[tokio::test]
    async fn cluster_usage_is_refused_without_listing_pods_everywhere() {
        let cluster = cluster_with_namespaces();
        cluster.update(|s| {
            s.forbidden_resources.insert("pods".to_owned());
        });
        let (status, problem) = call(
            &cluster,
            Settings::default(),
            "GET",
            "/api/v1/cluster/usage",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(problem["kind"], "forbidden");
        assert_eq!(
            problem["denied"],
            json!({"verb": "list", "group": "", "resource": "pods"})
        );
        assert!(problem.get("items").is_none());
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
        // Zeabur's own queries, naming the project and the service's pods; the person's words
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
        assert_eq!(setting["storedIn"]["name"], "zeabur-settings");
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].target.namespace, "zeabur");
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
                .insert("ConfigMap/zeabur-settings".to_owned());
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
                   "name": "zeabur-settings", "namespace": "zeabur"})
        );
    }

    /// The query of a proxied instant query.
    fn instant_query(path: &str) -> String {
        let (_, query) = path.split_once('?').unwrap();
        form_urlencoded::parse(query.as_bytes())
            .find(|(k, _)| k == "query")
            .unwrap()
            .1
            .into_owned()
    }

    fn vector(result: &Value) -> Value {
        json!({"status": "success", "data": {"resultType": "vector", "result": result}})
    }

    /// A store scraping node-exporter, whose node is at its address, and the kubelet: vps-1's
    /// filesystem and shop's one volume, both nearly full.
    #[expect(clippy::unnecessary_wraps, reason = "a `ProxyAnswer`, which may fail")]
    fn disk_store(path: &str) -> Result<Value, KubernetesStatus> {
        let query = instant_query(path);
        let series = |name: &str, labels: Value, value: u64| {
            let mut metric = labels;
            metric["__name__"] = json!(name);
            json!({"metric": metric, "value": [1_760_000_000, value.to_string()]})
        };
        Ok(vector(&if query.contains("node_filesystem") {
            let fs = json!({"instance": "10.0.0.1:9100", "mountpoint": "/"});
            json!([
                series("node_filesystem_size_bytes", fs.clone(), 100),
                series("node_filesystem_free_bytes", fs.clone(), 12),
                series("node_filesystem_avail_bytes", fs, 12),
            ])
        } else if query.contains("namespace=\"shop\"") {
            let claim = json!({"namespace": "shop", "persistentvolumeclaim": "data-db-0"});
            json!([
                series("kubelet_volume_stats_capacity_bytes", claim.clone(), 10),
                series("kubelet_volume_stats_used_bytes", claim.clone(), 9),
                series("kubelet_volume_stats_available_bytes", claim, 1),
            ])
        } else {
            json!([])
        }))
    }

    /// A store that scrapes neither.
    #[expect(clippy::unnecessary_wraps, reason = "a `ProxyAnswer`, which may fail")]
    fn bare_store(_: &str) -> Result<Value, KubernetesStatus> {
        Ok(vector(&json!([])))
    }

    #[tokio::test]
    async fn volumes_and_node_disks_come_from_the_store_or_say_why_not() {
        let cluster = cluster();
        cluster.update(|s| {
            s.nodes = vec![
                serde_json::from_value(json!({"metadata": {"name": "vps-1"},
                    "status": {"conditions": [{"type": "Ready", "status": "True"}],
                        "addresses": [{"type": "InternalIP", "address": "10.0.0.1"}]}}))
                .unwrap(),
            ];
        });
        let volumes = get(&cluster, "/api/v1/projects/shop/volumes").await;
        assert_eq!(volumes["state"], "not-configured");
        assert_eq!(volumes["items"], json!([]));
        let nodes = get(&cluster, "/api/v1/cluster/nodes").await;
        assert_eq!(nodes["disk"], "not-configured");
        assert!(nodes["items"][0].get("disk").is_none());

        cluster.update(|s| {
            s.proxied.insert(STORE.to_owned(), disk_store);
        });
        let volumes = get(&cluster, "/api/v1/projects/shop/volumes").await;
        assert_eq!(volumes["state"], "available");
        assert_eq!(
            volumes["items"],
            json!([{"claim": "data-db-0", "capacityBytes": 10, "usedBytes": 9,
                    "availableBytes": 1}])
        );
        let nodes = get(&cluster, "/api/v1/cluster/nodes").await;
        assert_eq!(nodes["disk"], "available");
        assert_eq!(
            nodes["items"][0]["disk"],
            json!({"capacityBytes": 100, "usedBytes": 88, "availableBytes": 12})
        );
        assert_eq!(nodes["totals"]["disk"]["capacityBytes"], 100);
        let asked = cluster.inspect(|s| s.proxy_requests.clone()).unwrap();
        let volume_query = instant_query(
            asked
                .iter()
                .find(|r| r.contains("kubelet_volume_stats"))
                .unwrap(),
        );
        assert!(
            volume_query.contains("namespace=\"shop\""),
            "{volume_query}"
        );

        // The store answers but scrapes neither: not zero, not collected.
        cluster.update(|s| {
            s.proxied.insert(STORE.to_owned(), bare_store);
        });
        let volumes = get(&cluster, "/api/v1/projects/shop/volumes").await;
        assert_eq!(volumes["state"], "not-collected");
        let nodes = get(&cluster, "/api/v1/cluster/nodes").await;
        assert_eq!(nodes["disk"], "not-collected");
        assert!(nodes["totals"].get("disk").is_none());

        cluster.update(|s| {
            s.proxied.insert(STORE.to_owned(), refused);
        });
        let volumes = get(&cluster, "/api/v1/projects/shop/volumes").await;
        assert_eq!(volumes["state"], "unreadable");
        assert_eq!(
            volumes["unreadable"][0]["resource"],
            "services/proxy (observability/vmsingle-vm-victoria-metrics-k8s-stack)"
        );
        let nodes = get(&cluster, "/api/v1/cluster/nodes").await;
        assert_eq!(nodes["disk"], "unreadable");
        assert!(nodes["items"][0].get("disk").is_none());

        cluster.update(|s| {
            s.proxied.insert(STORE.to_owned(), no_endpoints);
        });
        let volumes = get(&cluster, "/api/v1/projects/shop/volumes").await;
        assert_eq!(volumes["state"], "unavailable");

        let (status, _) = call(
            &cluster,
            Settings::default(),
            "GET",
            "/api/v1/projects/Shop%22%7D/volumes",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
