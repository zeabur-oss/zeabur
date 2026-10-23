//! The API against an in-memory cluster: what each endpoint returns, that errors are problems
//! that keep kube-apiserver's answer, that a Secret's values appear only when asked for, that
//! taking a right away takes effect on the very next request, that no answer is cached, and that
//! event lists stop at their limit.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use axum::Router;
    use axum::body::Body;
    use axum::http::{HeaderMap, Request, StatusCode};
    use http_body_util::BodyExt as _;
    use serde_json::{Value, json};
    use tower::ServiceExt as _;
    use zeabur_api::access::SingleCluster;
    use zeabur_core::problem::KubernetesStatus;
    use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
    use zeabur_kube::k8s_openapi::api::core::v1::{ConfigMap, Namespace, Node, Pod, Secret};
    use zeabur_kube::k8s_openapi::api::events::v1::Event;
    use zeabur_kube::k8s_openapi::jiff::Timestamp;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects, SecretSummary, ServedResource};

    use crate::common::app;

    const SECRET_VALUE: &str = "correct-horse-battery-staple";

    fn objects() -> NamespaceObjects {
        let deployment: Deployment = serde_json::from_value(json!({
            "metadata": {"name": "web", "namespace": "shop", "labels": {"app": "web"}},
            "spec": {
                "selector": {"matchLabels": {"app": "web"}},
                "template": {
                    "metadata": {"labels": {"app": "web"}},
                    "spec": {"containers": [{
                        "name": "web", "image": "nginx:1.29",
                        "env": [{"name": "PASSWORD", "valueFrom": {"secretKeyRef": {"name": "db", "key": "password"}}}],
                        "envFrom": [{"configMapRef": {"name": "web-env"}}]
                    }]}
                }
            }
        }))
        .unwrap();
        let pod: Pod = serde_json::from_value(json!({
            "metadata": {"name": "web-1", "namespace": "shop", "labels": {"app": "web"}},
            "spec": {"containers": [{"name": "web", "image": "nginx:1.29"}]},
            "status": {"phase": "Running", "containerStatuses": [{
                "name": "web", "image": "nginx:1.29", "imageID": "", "ready": false,
                "restartCount": 3, "state": {"waiting": {"reason": "CrashLoopBackOff"}},
                "lastState": {"terminated": {"exitCode": 1, "reason": "Error"}}
            }]}
        }))
        .unwrap();
        let config_maps = ["web-env", "aaa-unused"]
            .iter()
            .map(|name| {
                serde_json::from_value::<ConfigMap>(json!({
                    "metadata": {"name": name, "namespace": "shop"},
                    "data": {"MODE": "production"}
                }))
                .unwrap()
            })
            .collect();
        let secret = Secret {
            metadata: serde_json::from_value(json!({"name": "db", "namespace": "shop"})).unwrap(),
            type_: Some("Opaque".to_owned()),
            ..Secret::default()
        };
        NamespaceObjects {
            namespace: "shop".to_owned(),
            deployments: vec![deployment],
            pods: vec![pod],
            config_maps,
            secrets: vec![SecretSummary {
                secret,
                key_count: 1,
            }],
            ..NamespaceObjects::default()
        }
    }

    /// Seven events: `e0`…`e5` a minute apart (`e5` newest; `e1` and `e4` about the pod, the
    /// others about the Deployment), and `old`, with no time at all; out of order, as
    /// kube-apiserver may return them.
    fn events() -> Vec<Event> {
        let mut events: Vec<Event> = (0..6)
            .map(|i| {
                let (kind, name) = if i % 3 == 1 {
                    ("Pod", "web-1")
                } else {
                    ("Deployment", "web")
                };
                serde_json::from_value(json!({
                    "metadata": {"name": format!("e{i}"), "namespace": "shop"},
                    "eventTime": format!("2026-10-14T08:0{i}:00.000000Z"),
                    "regarding": {"apiVersion": "v1", "kind": kind, "name": name, "namespace": "shop"},
                    "type": "Normal", "reason": "Test", "note": format!("event {i}"),
                    "reportingController": "test", "reportingInstance": "test", "action": "Test"
                }))
                .unwrap()
            })
            .collect();
        events.push(
            serde_json::from_value(json!({
                "metadata": {"name": "old", "namespace": "shop"},
                "type": "Normal", "reason": "Test", "note": "no time",
                "reportingController": "test", "reportingInstance": "test", "action": "Test"
            }))
            .unwrap(),
        );
        events.swap(0, 4);
        events.swap(2, 6);
        events
    }

    fn cluster() -> MemoryCluster {
        let namespaces: Vec<Namespace> = vec![
            serde_json::from_value(json!({"metadata": {"name": "default"}})).unwrap(),
            serde_json::from_value(json!({"metadata": {"name": "shop",
                "labels": {"zeabur.com/project": "true"}}}))
            .unwrap(),
        ];
        MemoryCluster::new(State {
            identity: Identity {
                username: "alice".to_owned(),
                groups: vec!["developers".to_owned()],
            },
            namespaces,
            objects: BTreeMap::from([("shop".to_owned(), objects())]),
            events: BTreeMap::from([("shop".to_owned(), events())]),
            logs: BTreeMap::from([("shop/web-1".to_owned(), "one\ntwo\n".to_owned())]),
            secret_values: BTreeMap::from([(
                "shop/db".to_owned(),
                BTreeMap::from([("password".to_owned(), SECRET_VALUE.as_bytes().to_vec())]),
            )]),
            ..State::default()
        })
    }

    async fn send(
        app: Router,
        method: &str,
        uri: &str,
        headers: &[(&str, &str)],
    ) -> (StatusCode, HeaderMap, String) {
        let mut request = Request::builder().method(method).uri(uri);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = app
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, headers, String::from_utf8(body.to_vec()).unwrap())
    }

    async fn call(app: Router, method: &str, uri: &str) -> (StatusCode, Option<String>, String) {
        let (status, headers, body) = send(app, method, uri, &[]).await;
        let cache = headers
            .get("cache-control")
            .map(|v| v.to_str().unwrap().to_owned());
        (status, cache, body)
    }

    async fn get(cluster: &MemoryCluster, uri: &str) -> Value {
        let (status, _, body) = call(app(cluster), "GET", uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        json(&body)
    }

    fn json(body: &str) -> Value {
        serde_json::from_str(body).unwrap()
    }

    fn names(list: &Value) -> Vec<String> {
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| {
                item["origin"]["name"]
                    .as_str()
                    .or_else(|| item["name"].as_str())
                    .unwrap()
                    .to_owned()
            })
            .collect()
    }

    /// On this machine nobody signs in: there is no signing out and no elevation to offer.
    #[tokio::test]
    async fn the_local_session_offers_no_sign_out_and_no_elevation() {
        let session = get(&cluster(), "/api/v1/session").await;
        assert_eq!(
            session,
            json!({"signsIn": false, "elevation": "unavailable"})
        );
    }

    #[tokio::test]
    async fn projects_list_adopted_namespaces_first_with_their_health() {
        let list = get(&cluster(), "/api/v1/projects").await;
        let items = &list["items"];
        assert_eq!(items[0]["name"], "shop");
        assert_eq!(items[0]["id"], "projects/shop");
        assert_eq!(items[0]["adopted"], true);
        assert_eq!(
            items[0]["origin"],
            json!({"apiVersion": "v1", "kind": "Namespace", "name": "shop"})
        );
        // The one service has a crash-looping pod.
        assert_eq!(
            items[0]["health"],
            json!({"services": 1, "healthy": 0, "progressing": 0, "warning": 0, "failing": 1})
        );
        assert_eq!(items[1]["name"], "default");
        assert!(items[1].get("health").is_none(), "not adopted: no counts");
        assert_eq!(names(&list), ["shop", "default"]);
        assert!(list.get("totalSize").is_none() && list.get("nextPageToken").is_none());
    }

    #[tokio::test]
    async fn projects_list_each_adopted_service_with_its_health_and_nothing_more() {
        let list = get(&cluster(), "/api/v1/projects").await;
        let items = &list["items"];
        // Exactly these fields: no containers, variables, Secrets or configuration.
        assert_eq!(
            items[0]["services"],
            json!([{
                "id": "projects/shop/services/deployment/web",
                "kind": "Deployment",
                "name": "web",
                "health": "fail",
                "ready": 0,
                "desired": 1,
                "restarts": 3,
                "reason": "CrashLoopBackOff"
            }])
        );
        assert!(
            items[1].get("services").is_none(),
            "not adopted: no services"
        );
        // The counts and the list come from the same read and agree.
        let failing = items[0]["services"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["health"] == "fail")
            .count();
        assert_eq!(items[0]["health"]["failing"], failing);
    }

    #[tokio::test]
    async fn the_project_document_is_the_project_itself() {
        let project = get(&cluster(), "/api/v1/projects/shop").await;
        assert_eq!(project["id"], "projects/shop");
        assert_eq!(project["namespace"], "shop");
        assert_eq!(project["unreadable"], json!([]));
        assert!(project.get("project").is_none(), "no wrapper");
        let service = &project["services"][0];
        assert_eq!(service["name"], "web");
        assert_eq!(service["id"], "projects/shop/services/deployment/web");
        assert_eq!(service["pods"][0]["id"], "projects/shop/pods/web-1");
        let env = &service["containers"][0]["env"][0];
        assert_eq!(env["name"], "PASSWORD");
        assert_eq!(env["source"]["kind"], "Secret");
        assert!(
            env["origin"]["fieldPath"]
                .as_str()
                .unwrap()
                .contains("env[name=PASSWORD]")
        );
        let ids: Vec<&str> = project["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect();
        assert!(
            ids.contains(&"projects/shop/resources/secret/db"),
            "{ids:?}"
        );
    }

    /// The project document lists each Kubernetes Service and Ingress once and a service names
    /// its own; the service document carries the details of those it names.
    #[tokio::test]
    async fn a_service_is_cut_from_the_document_with_its_kind_in_any_case() {
        let cluster = cluster();
        cluster.update(|s| {
            let shop = s.objects.get_mut("shop").unwrap();
            shop.services.push(
                serde_json::from_value(json!({
                    "metadata": {"name": "web", "namespace": "shop"},
                    "spec": {"selector": {"app": "web"}, "ports": [{"port": 80, "targetPort": "http"}]}
                }))
                .unwrap(),
            );
            shop.ingresses.push(
                serde_json::from_value(json!({
                    "metadata": {"name": "public", "namespace": "shop"},
                    "spec": {"rules": [{"host": "shop.example", "http": {"paths": [{
                        "path": "/", "pathType": "Prefix",
                        "backend": {"service": {"name": "web", "port": {"number": 80}}}
                    }]}}]}
                }))
                .unwrap(),
            );
        });
        let document = get(&cluster, "/api/v1/projects/shop").await;
        assert_eq!(document["services"][0]["endpointNames"], json!(["web"]));
        assert_eq!(document["services"][0]["routeNames"], json!(["public"]));
        assert_eq!(document["endpoints"][0]["name"], "web");
        assert_eq!(document["endpoints"][0]["ports"][0]["targetPort"], "http");
        assert_eq!(document["routes"][0]["hostnames"], json!(["shop.example"]));
        assert_eq!(document.get("relations"), None);
        for kind in ["deployment", "Deployment", "DEPLOYMENT"] {
            let service = get(
                &cluster,
                &format!("/api/v1/projects/shop/services/{kind}/web"),
            )
            .await;
            assert_eq!(service["id"], "projects/shop/services/deployment/web");
            assert_eq!(service["unreadable"], json!([]));
            let mut expected = document["services"][0].clone();
            expected["unreadable"] = json!([]);
            expected["endpoints"] = document["endpoints"].clone();
            expected["routes"] = document["routes"].clone();
            assert_eq!(service, expected);
        }
        for uri in [
            "/api/v1/projects/shop/services/deployment/missing",
            "/api/v1/projects/shop/services/statefulset/web",
            "/api/v1/projects/shop/services/replicaset/web",
        ] {
            let (status, _, body) = call(app(&cluster), "GET", uri).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
            assert_eq!(json(&body)["type"], "/problems/not-found");
        }
    }

    #[tokio::test]
    async fn the_project_document_says_which_workloads_use_each_resource() {
        let cluster = cluster();
        let document = get(&cluster, "/api/v1/projects/shop").await;
        let resource = |id: &str| {
            document["resources"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == id)
                .cloned()
                .unwrap()
        };
        let secret = resource("projects/shop/resources/secret/db");
        assert_eq!(secret["kind"], "Secret");
        assert_eq!(secret["keyCount"], 1);
        assert_eq!(
            secret["attribution"]["owners"],
            json!([{"kind": "Deployment", "name": "web"}])
        );

        let orphan = resource("projects/shop/resources/configmap/aaa-unused");
        assert_eq!(orphan["attribution"]["ownership"], "orphan");
        assert_eq!(orphan["attribution"]["owners"], json!([]));
    }

    #[tokio::test]
    async fn config_maps_are_listed_by_name() {
        let cluster = cluster();
        let config_maps = get(&cluster, "/api/v1/projects/shop/configmaps").await;
        assert_eq!(names(&config_maps), ["aaa-unused", "web-env"]);
        assert_eq!(
            config_maps["items"][1]["id"],
            "projects/shop/resources/configmap/web-env"
        );
        assert_eq!(
            config_maps["items"][1]["origin"],
            json!({"apiVersion": "v1", "kind": "ConfigMap", "namespace": "shop", "name": "web-env"})
        );
        assert_eq!(config_maps["items"][1]["data"]["MODE"], "production");
    }

    #[tokio::test]
    async fn events_are_newest_first_up_to_the_limit_and_filtered_by_object() {
        let cluster = cluster();
        let all = get(&cluster, "/api/v1/projects/shop/events").await;
        assert_eq!(names(&all), ["e5", "e4", "e3", "e2", "e1", "e0", "old"]);
        assert_eq!(all["totalSize"], 7);
        assert!(all.get("nextPageToken").is_none());
        let first = &all["items"][0];
        assert_eq!(first["id"], "projects/shop/events/e5");
        assert_eq!(first["lastSeen"], "2026-10-14T08:05:00Z");
        assert_eq!(first["regarding"]["kind"], "Deployment");

        let newest = get(&cluster, "/api/v1/projects/shop/events?limit=3").await;
        assert_eq!(names(&newest), ["e5", "e4", "e3"]);
        assert_eq!(newest["totalSize"], 7, "every match is counted");

        let pod = get(&cluster, "/api/v1/projects/shop/events?regarding=pod/web-1").await;
        assert_eq!(names(&pod), ["e4", "e1"]);
        let deployment = get(
            &cluster,
            "/api/v1/projects/shop/events?regarding=deployment/web&limit=2",
        )
        .await;
        assert_eq!(
            names(&deployment),
            ["e5", "e3"],
            "the kind is matched in any case"
        );
        assert_eq!(deployment["totalSize"], 4);

        for bad in ["regarding=web", "limit=0", "limit=x"] {
            let (status, ..) = call(
                app(&cluster),
                "GET",
                &format!("/api/v1/projects/shop/events?{bad}"),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
        }
    }

    #[tokio::test]
    async fn no_answer_is_cached_or_tagged() {
        let cluster = cluster();
        // A log stream keeps the `no-cache` Server-Sent Events carry; everything else is
        // `no-store`.
        for (method, uri, cache) in [
            ("GET", "/api/v1/projects/shop", "no-store"),
            ("GET", "/api/v1/projects/shop/events", "no-store"),
            ("GET", "/api/v1/nothing", "no-store"),
            (
                "POST",
                "/api/v1/projects/shop/secrets/db/reveal",
                "no-store",
            ),
            (
                "GET",
                "/api/v1/projects/shop/pods/web-1/logs/follow",
                "no-cache",
            ),
        ] {
            let (_, headers, _) = send(app(&cluster), method, uri, &[]).await;
            assert_eq!(
                headers.get("cache-control").unwrap(),
                cache,
                "{method} {uri}"
            );
            assert!(headers.get("etag").is_none(), "{method} {uri}");
        }
        let (status, ..) = send(
            app(&cluster),
            "GET",
            "/api/v1/projects/shop",
            &[("if-none-match", "*")],
        )
        .await;
        assert_eq!(status, StatusCode::OK, "no conditional answers");
    }

    #[tokio::test]
    async fn the_project_document_says_who_may_reach_each_service() {
        let cluster = cluster();
        let open = get(&cluster, "/api/v1/projects/shop").await;
        let reach = &open["extensions"]["networkPolicy"];
        assert_eq!(reach["policies"], json!([]));
        assert_eq!(reach["services"][0]["fromIngressController"], "allowed");
        cluster.update(|s| {
            s.dynamic.push(
                serde_json::from_value(json!({
                    "apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
                    "metadata": {"name": "deny-web", "namespace": "shop"},
                    "spec": {"podSelector": {"matchLabels": {"app": "web"}}}
                }))
                .unwrap(),
            );
        });
        let closed = get(&cluster, "/api/v1/projects/shop").await;
        let web = &closed["extensions"]["networkPolicy"]["services"][0];
        assert_eq!(web["service"], "projects/shop/services/deployment/web");
        assert_eq!(web["selectedBy"], json!(["deny-web"]));
        assert_eq!(web["ingressIsolated"], true);
        assert_eq!(web["fromIngressController"], "blocked");
    }

    #[tokio::test]
    async fn network_policy_presets_isolate_and_allow_the_ingress_controller() {
        let cluster = cluster();
        let presets = get(
            &cluster,
            "/api/v1/projects/shop/network-policy/presets?service=Deployment/web",
        )
        .await;
        let kinds: Vec<&str> = presets["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, ["isolate", "allow-from-ingress-controller"]);
        let isolate = &presets["items"][0]["object"];
        assert_eq!(isolate["metadata"]["namespace"], "shop");
        assert_eq!(isolate["spec"]["podSelector"]["matchLabels"]["app"], "web");
        let (status, ..) = call(
            app(&cluster),
            "GET",
            "/api/v1/projects/shop/network-policy/presets?service=deployment/nope",
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn backups_join_providers_and_name_what_could_not_be_read() {
        let cluster = cluster();
        cluster.update(|s| {
            for (group, resource, kind) in [
                ("k8up.io", "schedules", "Schedule"),
                ("k8up.io", "backups", "Backup"),
                ("velero.io", "schedules", "Schedule"),
                ("velero.io", "backups", "Backup"),
            ] {
                s.served
                    .push(ServedResource::readable(group, "v1", resource, kind, true));
            }
            s.forbidden_resources
                .insert("schedules.velero.io".to_owned());
            if let Some(objects) = s.objects.get_mut("shop") {
                objects.persistent_volume_claims.push(
                    serde_json::from_value(json!({
                        "metadata": {"name": "uploads", "namespace": "shop"},
                        "spec": {"accessModes": ["ReadWriteOnce"]}
                    }))
                    .unwrap(),
                );
            }
            s.dynamic.push(
                serde_json::from_value(json!({
                    "apiVersion": "k8up.io/v1", "kind": "Schedule",
                    "metadata": {"name": "nightly", "namespace": "shop"},
                    "spec": {"backup": {"schedule": "0 2 * * *"}}
                }))
                .unwrap(),
            );
        });
        let backups = get(&cluster, "/api/v1/projects/shop/backups").await;
        let uploads = backups["coverage"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["target"]["name"] == "uploads")
            .unwrap();
        assert_eq!(uploads["scheduledBy"], json!(["k8up:shop/nightly"]));
        let state = |module: &str| {
            backups["providers"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["module"] == module)
                .unwrap()["state"]
                .clone()
        };
        assert_eq!(state("k8up"), "installed");
        assert_eq!(state("velero"), "refused");
        assert_eq!(state("cloudnative-pg"), "not-installed");
        assert_eq!(backups["unreadable"][0]["resource"], "schedules.velero.io");
    }

    #[tokio::test]
    async fn runtime_alerts_are_falcos_report_results_or_say_why_there_are_none() {
        let cluster = cluster();
        let alerts = get(&cluster, "/api/v1/projects/shop/alerts").await;
        assert_eq!(alerts["state"], "not-installed");

        cluster.update(|s| {
            for (resource, kind) in [
                ("policyreports", "PolicyReport"),
                ("clusterpolicyreports", "ClusterPolicyReport"),
            ] {
                s.served.push(ServedResource::readable(
                    "wgpolicyk8s.io",
                    "v1alpha2",
                    resource,
                    kind,
                    resource == "policyreports",
                ));
            }
            s.dynamic.push(
                serde_json::from_value(json!({
                    "apiVersion": "wgpolicyk8s.io/v1alpha2", "kind": "PolicyReport",
                    "metadata": {"name": "falco-policy-report", "namespace": "shop"},
                    "results": [{
                        "rule": "Terminal shell in container", "source": "Falco",
                        "severity": "low", "policy": "syscall",
                        "message": "Notice A shell was spawned in a container",
                        "timestamp": {"seconds": 1_792_679_654_i64, "nanos": 0},
                        "properties": {"k8s.pod.name": "web-7c9d8-abcde", "container.name": "web"}
                    }]
                }))
                .unwrap(),
            );
        });
        let alerts = get(&cluster, "/api/v1/projects/shop/alerts").await;
        assert_eq!(alerts["state"], "installed");
        assert_eq!(alerts["items"][0]["rule"], "Terminal shell in container");
        assert_eq!(alerts["items"][0]["pod"], "web-7c9d8-abcde");
        assert_eq!(alerts["items"][0]["origin"]["kind"], "PolicyReport");
        let cluster_alerts = get(&cluster, "/api/v1/cluster/alerts").await;
        assert_eq!(cluster_alerts["state"], "installed");
        assert_eq!(cluster_alerts["items"], json!([]));

        cluster.update(|s| {
            s.forbidden_in_namespaces
                .insert("policyreports.wgpolicyk8s.io".to_owned());
            s.forbidden_resources
                .insert("clusterpolicyreports.wgpolicyk8s.io".to_owned());
        });
        let alerts = get(&cluster, "/api/v1/projects/shop/alerts").await;
        assert_eq!(alerts["state"], "refused");
        assert_eq!(alerts["items"], json!([]));
        assert_eq!(
            alerts["unreadable"][0]["resource"],
            "policyreports.wgpolicyk8s.io"
        );
        let cluster_alerts = get(&cluster, "/api/v1/cluster/alerts").await;
        assert_eq!(cluster_alerts["state"], "refused");
    }

    #[tokio::test]
    async fn builds_give_each_run_its_pod_or_say_the_builder_is_missing() {
        let cluster = cluster();
        let missing = get(&cluster, "/api/v1/projects/shop/builds").await;
        assert_eq!(missing["state"], "not-installed");
        cluster.update(|s| {
            for (resource, kind) in [("builds", "Build"), ("buildruns", "BuildRun")] {
                s.served.push(ServedResource::readable(
                    "builder.zeabur.com",
                    "v1alpha1",
                    resource,
                    kind,
                    true,
                ));
            }
            s.dynamic.push(
                serde_json::from_value(json!({
                    "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "Build",
                    "metadata": {"name": "web", "namespace": "shop"},
                    "spec": {"builderRef": "dockerfile",
                        "source": {"git": {"url": "https://example.com/acme/web", "ref": "main"}}},
                    "status": {"latestImage": "zot:5000/shop/web:main-1-abc@sha256:00"}
                }))
                .unwrap(),
            );
            s.dynamic.push(
                serde_json::from_value(json!({
                    "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "BuildRun",
                    "metadata": {"name": "web-x1", "namespace": "shop",
                        "creationTimestamp": "2026-10-23T08:00:00Z"},
                    "spec": {"buildRef": "web", "revision": "1ce75d01b6978863647da42557a707a479da3a51"},
                    "status": {"phase": "Building"}
                }))
                .unwrap(),
            );
            if let Some(objects) = s.objects.get_mut("shop") {
                objects.pods.push(
                    serde_json::from_value(json!({
                        "metadata": {"name": "web-x1-abcde", "namespace": "shop",
                            "labels": {"builder.zeabur.com/buildrun": "web-x1"}},
                        "spec": {"initContainers": [{"name": "fetch"}],
                            "containers": [{"name": "build"}]}
                    }))
                    .unwrap(),
                );
            }
        });
        let builds = get(&cluster, "/api/v1/projects/shop/builds").await;
        assert_eq!(builds["state"], "installed");
        assert_eq!(builds["builds"][0]["repository"], "zot:5000/shop/web");
        assert_eq!(builds["runs"][0]["name"], "web-x1");
        assert_eq!(builds["runs"][0]["pod"], "web-x1-abcde");
        assert_eq!(builds["runs"][0]["containers"], json!(["fetch", "build"]));
        assert_eq!(builds["keel"], json!([]));
    }

    /// The test cluster with a custom resource besides the built-in kinds.
    fn cluster_with_a_certificate() -> MemoryCluster {
        let cluster = cluster();
        cluster.update(|s| {
            s.served.push(ServedResource::readable(
                "cert-manager.io",
                "v1",
                "certificates",
                "Certificate",
                true,
            ));
            s.dynamic.push(
                serde_json::from_value(json!({
                    "apiVersion": "cert-manager.io/v1", "kind": "Certificate",
                    "metadata": {"name": "shop-tls", "namespace": "shop",
                        "creationTimestamp": "2026-10-15T00:00:00Z",
                        "labels": {"app.kubernetes.io/managed-by": "Helm"}},
                    "spec": {"secretName": "shop-tls", "dnsNames": ["shop.example.test"]},
                    "status": {"conditions": [{"type": "Ready", "status": "True"}]}
                }))
                .unwrap(),
            );
        });
        cluster
    }

    fn kind_of(list: &Value, kind: &str) -> Option<Value> {
        list["items"]
            .as_array()?
            .iter()
            .find(|k| k["kind"] == kind)
            .cloned()
    }

    #[tokio::test]
    async fn objects_are_listed_kinds_first() {
        let cluster = cluster_with_a_certificate();
        // Kinds first: every listable kind comes back; the usual ones are read at once.
        let list = get(&cluster, "/api/v1/projects/shop/objects").await;
        assert_eq!(list["id"], "projects/shop/objects");
        assert_eq!(list["scanned"], false);
        let config_maps = kind_of(&list, "ConfigMap").unwrap();
        assert_eq!(config_maps["curated"], true);
        assert_eq!(config_maps["listed"], true);
        assert_eq!(config_maps["access"], "allowed");
        assert_eq!(config_maps["items"][0]["name"], "aaa-unused");
        let ingresses = kind_of(&list, "Ingress").expect("empty usual kinds are named");
        assert_eq!(ingresses["listed"], true);
        let unread = kind_of(&list, "Certificate").expect("custom resources are named");
        assert_eq!(unread["listed"], false, "only usual kinds are read unasked");
        assert_eq!(unread["items"], json!([]));
        let events = kind_of(&list, "Event").unwrap();
        assert_eq!(events["generated"], true);
        assert_eq!(events["listed"], false);

        // Named kinds are read, generated ones included.
        let list = get(
            &cluster,
            "/api/v1/projects/shop/objects?kinds=cert-manager.io%2Fcertificates,events.k8s.io/events",
        )
        .await;
        let certificates = kind_of(&list, "Certificate").unwrap();
        assert_eq!(certificates["listed"], true);
        assert_eq!(certificates["group"], "cert-manager.io");
        assert_eq!(certificates["generated"], false);
        assert_eq!(certificates["items"][0]["name"], "shop-tls");
        assert_eq!(
            certificates["items"][0]["id"],
            "projects/shop/objects/cert-manager.io/v1/certificates/shop-tls"
        );
        assert_eq!(certificates["items"][0]["created"], "2026-10-15T00:00:00Z");
        assert_eq!(kind_of(&list, "Event").unwrap()["listed"], true);
        assert_eq!(
            kind_of(&list, "ConfigMap").unwrap()["listed"],
            false,
            "named kinds replace the usual ones"
        );

        // A scan reads every kind but the generated ones.
        let list = get(&cluster, "/api/v1/projects/shop/objects?all=true").await;
        assert_eq!(list["scanned"], true);
        assert_eq!(kind_of(&list, "Certificate").unwrap()["listed"], true);
        assert_eq!(kind_of(&list, "Event").unwrap()["listed"], false);
        let (status, ..) = call(
            app(&cluster),
            "GET",
            "/api/v1/projects/shop/objects?all=yes",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn a_kind_refused_in_the_namespace_is_named_and_the_rest_is_served() {
        let cluster = cluster_with_a_certificate();
        cluster.update(|s| {
            s.forbidden_in_namespaces.insert("secrets".to_owned());
        });
        let project = get(&cluster, "/api/v1/projects/shop").await;
        assert_eq!(project["unreadable"][0]["resource"], "secrets");
        assert_eq!(project["unreadable"][0]["kubernetes"]["code"], 403);
        assert_ne!(project["services"], json!([]), "the rest is served");

        let list = get(&cluster, "/api/v1/projects/shop/objects").await;
        assert_eq!(list["unreadable"][0]["resource"], "secrets");
        assert_eq!(kind_of(&list, "Secret").unwrap()["access"], "denied");
        assert_eq!(kind_of(&list, "ConfigMap").unwrap()["listed"], true);
    }

    #[tokio::test]
    async fn every_object_of_a_project_is_readable_as_yaml() {
        let cluster = cluster_with_a_certificate();
        let object = get(
            &cluster,
            "/api/v1/projects/shop/objects/cert-manager.io/v1/certificates/shop-tls",
        )
        .await;
        let yaml = object["yaml"].as_str().unwrap();
        assert!(
            yaml.starts_with("apiVersion: cert-manager.io/v1\nkind: Certificate\n"),
            "{yaml}"
        );
        assert!(yaml.contains("secretName: shop-tls"), "{yaml}");
        assert!(
            !yaml.contains("status") && !yaml.contains("creationTimestamp"),
            "{yaml}"
        );
        assert_eq!(object["origin"]["namespace"], "shop");
        assert_eq!(object["provenance"]["tool"], "Helm");
        assert_eq!(
            object["status"],
            json!({"conditions": [{"type": "Ready", "status": "True"}]}),
            "the status comes beside the YAML"
        );

        let secret = get(&cluster, "/api/v1/projects/shop/objects/core/v1/secrets/db").await;
        assert!(secret.get("status").is_none(), "{secret}");
        assert!(
            secret["yaml"]
                .as_str()
                .unwrap()
                .contains("password: (hidden)"),
            "{secret}"
        );
        for missing in [
            "/api/v1/projects/shop/objects/core/v1/secrets/nope",
            "/api/v1/projects/shop/objects/example.com/v1/widgets/a",
            "/api/v1/projects/shop/objects/core/v1/nodes/vps-1",
        ] {
            let (status, _, body) = call(app(&cluster), "GET", missing).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{missing}: {body}");
        }

        cluster.update(|s| {
            s.forbidden_namespaces.insert("shop".to_owned());
        });
        let (status, ..) = call(app(&cluster), "GET", "/api/v1/projects/shop/objects").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn secret_values_appear_only_when_revealed_and_are_never_cached() {
        let cluster = cluster();
        for uri in [
            "/api/v1/projects",
            "/api/v1/projects/shop",
            "/api/v1/projects/shop/services/deployment/web",
            "/api/v1/projects/shop/configmaps",
            "/api/v1/projects/shop/objects",
            "/api/v1/projects/shop/objects/core/v1/secrets/db",
        ] {
            let (_, _, body) = call(app(&cluster), "GET", uri).await;
            assert!(!body.contains(SECRET_VALUE), "{uri} leaked the value");
        }
        assert_eq!(cluster.inspect(|s| s.secret_reads), Some(0));

        let (status, _, body) = call(
            app(&cluster),
            "GET",
            "/api/v1/projects/shop/secrets/db/reveal",
        )
        .await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "{body}");

        let (status, cache, body) = call(
            app(&cluster),
            "POST",
            "/api/v1/projects/shop/secrets/db/reveal",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(cache.as_deref(), Some("no-store"));
        let revealed = json(&body);
        assert_eq!(revealed["values"]["password"]["value"], SECRET_VALUE);
        assert_eq!(revealed["origin"]["kind"], "Secret");
        assert_eq!(cluster.inspect(|s| s.secret_reads), Some(1));
    }

    #[tokio::test]
    async fn removing_the_right_denies_the_next_request_with_kubernetes_own_answer() {
        let cluster = cluster();
        let (status, ..) = call(app(&cluster), "GET", "/api/v1/projects/shop").await;
        assert_eq!(status, StatusCode::OK);

        cluster.update(|s| {
            s.forbidden_namespaces.insert("shop".to_owned());
        });
        let (status, _, body) = call(app(&cluster), "GET", "/api/v1/projects/shop").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let problem = json(&body);
        assert_eq!(problem["kind"], "forbidden");
        assert_eq!(problem["kubernetes"]["reason"], "Forbidden");
        assert!(problem["fix"].as_str().unwrap().contains("administrator"));
        // Every kind was refused; the first one read names the missing right.
        assert_eq!(
            problem["denied"],
            json!({"verb": "list", "group": "apps", "resource": "deployments", "namespace": "shop"})
        );

        // The list still answers; the project it cannot read has no health counts.
        let list = get(&cluster, "/api/v1/projects").await;
        assert_eq!(list["items"][0]["name"], "shop");
        assert!(list["items"][0].get("health").is_none());
    }

    #[tokio::test]
    async fn without_namespace_listing_the_person_is_told_how_to_go_on() {
        let cluster = cluster();
        cluster.update(|s| s.forbid_everything = true);
        let (status, _, body) = call(app(&cluster), "GET", "/api/v1/projects").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let problem = json(&body);
        let fix = problem["fix"].as_str().unwrap();
        assert!(fix.contains("`list` on `namespaces`"));
        assert_eq!(problem["kubernetes"]["reason"], "Forbidden");
        assert_eq!(
            problem["denied"],
            json!({"verb": "list", "group": "", "resource": "namespaces"})
        );
    }

    #[tokio::test]
    async fn logs_are_returned_as_lines() {
        let (status, _, body) = call(
            app(&cluster()),
            "GET",
            "/api/v1/projects/shop/pods/web-1/logs?container=web",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let body = json(&body);
        assert_eq!(body["lines"], json!(["one", "two"]));
        assert_eq!(body["origin"]["fieldPath"], "spec.containers{web}");
        assert_eq!(body["origin"]["namespace"], "shop");

        let (status, _, body) = call(
            app(&cluster()),
            "GET",
            "/api/v1/projects/shop/pods/web-1/logs/follow",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "data: one\n\ndata: two\n\n");
    }

    #[tokio::test]
    async fn a_log_the_kubelet_cannot_give_is_a_problem_not_a_line() {
        let cluster = cluster();
        // The pod has not restarted: kube-apiserver passes the kubelet's 400 on.
        for path in ["logs?previous=true", "logs/follow?previous=true"] {
            let (status, _, body) = call(
                app(&cluster),
                "GET",
                &format!("/api/v1/projects/shop/pods/web-1/{path}"),
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
            let problem = json(&body);
            assert_eq!(problem["kind"], "log-unavailable", "{path}");
            assert_eq!(problem["unavailableLog"], "no-previous-run", "{path}");
            assert_eq!(problem["kubernetes"]["code"], 400, "{path}");
        }

        // The kubelet's refusal as the whole body of a 200.
        cluster.update(|s| {
            s.logs.insert(
                "shop/web-1".to_owned(),
                "unable to retrieve container logs for containerd://4b1c0e9f".to_owned(),
            );
        });
        let (status, _, body) = call(
            app(&cluster),
            "GET",
            "/api/v1/projects/shop/pods/web-1/logs",
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let problem = json(&body);
        assert_eq!(problem["unavailableLog"], "removed");
        assert!(problem.get("lines").is_none());
        assert_eq!(
            problem["kubernetes"]["message"],
            "unable to retrieve container logs for containerd://4b1c0e9f"
        );
    }

    #[tokio::test]
    async fn the_cluster_has_a_version_and_capabilities_for_those_who_can_see_it_all() {
        let cluster = cluster();
        cluster.update(|s| {
            s.name = "k3d-zeabur".to_owned();
            s.facts.version = "v1.36.5+k3s1".to_owned();
            s.facts.nodes = Some(1);
        });
        let about = get(&cluster, "/api/v1/cluster").await;
        assert_eq!(
            about,
            json!({"name": "zeabur", "nameSource": "kubeconfig", "nameProvider": "k3d",
                   "kubernetesVersion": "v1.36.5+k3s1", "distribution": "k3s", "domains": {}, "builds": {},
                   "can": {"installComponents": "allowed", "changeNodes": "allowed", "changeBackups": "allowed"}})
        );

        let capabilities = get(&cluster, "/api/v1/cluster/capabilities").await;
        assert!(capabilities["items"][0]["name"].is_string());
        let mut sorted = names(&capabilities);
        sorted.sort();
        assert_eq!(names(&capabilities), sorted);
        assert!(capabilities["catalogueCommit"].is_string());

        cluster.update(|s| {
            s.facts.unreadable.push(zeabur_kube::Unreadable {
                resource: "customresourcedefinitions.apiextensions.k8s.io".to_owned(),
                status: KubernetesStatus::new(403, "Forbidden", "forbidden"),
            });
        });
        let (status, _, body) = call(app(&cluster), "GET", "/api/v1/cluster/capabilities").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(!body.contains("\"items\""), "{body}");
        assert_eq!(get(&cluster, "/api/v1/cluster").await, about);
    }

    fn node(name: &str, kubelet: &str, cordoned: bool) -> Node {
        serde_json::from_value(json!({
            "metadata": {"name": name, "labels": {"node-role.kubernetes.io/control-plane": "true"}},
            "spec": {"unschedulable": cordoned},
            "status": {
                "capacity": {"cpu": "2", "memory": "2Gi", "pods": "110"},
                "allocatable": {"cpu": "1900m", "memory": "1800Mi", "pods": "110"},
                "conditions": [{"type": "Ready", "status": "True"},
                               {"type": "MemoryPressure", "status": "True", "reason": "KubeletHasInsufficientMemory"}],
                "nodeInfo": {"kubeletVersion": kubelet, "kubeProxyVersion": "", "osImage": "Debian",
                    "kernelVersion": "6.12", "architecture": "arm64", "operatingSystem": "linux",
                    "containerRuntimeVersion": "containerd://2.1", "machineID": "", "systemUUID": "", "bootID": ""}
            }
        }))
        .unwrap()
    }

    fn with_nodes() -> MemoryCluster {
        let cluster = cluster();
        cluster.update(|s| {
            s.facts.version = "v1.34.10+k3s1".to_owned();
            s.nodes = vec![node("vps-1", "v1.34.10+k3s1", false)];
            if let Some(shop) = s.objects.get_mut("shop") {
                let mut pod = serde_json::to_value(&shop.pods[0]).unwrap();
                pod["spec"]["nodeName"] = json!("vps-1");
                pod["spec"]["containers"][0]["resources"] =
                    json!({"requests": {"cpu": "250m", "memory": "512Mi"}, "limits": {"memory": "1Gi"}});
                shop.pods = vec![serde_json::from_value(pod).unwrap()];
            }
        });
        cluster
    }

    #[tokio::test]
    async fn nodes_sum_their_pods_requests_against_allocatable() {
        let cluster = with_nodes();
        let list = get(&cluster, "/api/v1/cluster/nodes").await;
        let node = &list["items"][0];
        assert_eq!(node["id"], "cluster/nodes/vps-1");
        assert_eq!(node["roles"], json!(["control-plane"]));
        assert_eq!(node["ready"], "True");
        assert_eq!(node["problems"][0]["type"], "MemoryPressure");
        assert_eq!(
            node["allocatable"],
            json!({"cpuMillicores": 1900, "memoryBytes": 1800 << 20, "pods": 110})
        );
        assert_eq!(
            node["requests"],
            json!({"cpuMillicores": 250, "memoryBytes": 512 << 20, "pods": 1})
        );
        assert_eq!(node["limits"]["memoryBytes"], 1 << 30);
        assert!(node.get("usage").is_none());
        assert_eq!(list["metrics"], "not-installed");
        assert_eq!(list["podScan"], "complete");
        assert_eq!(list["totals"]["requests"]["cpuMillicores"], 250);

        cluster.update(|s| {
            s.served.push(ServedResource::readable(
                "metrics.k8s.io",
                "v1beta1",
                "nodes",
                "NodeMetrics",
                false,
            ));
            s.dynamic.push(
                serde_json::from_value(json!({
                    "apiVersion": "metrics.k8s.io/v1beta1", "kind": "NodeMetrics",
                    "metadata": {"name": "vps-1"},
                    "usage": {"cpu": "480m", "memory": "1Gi"}
                }))
                .unwrap(),
            );
            s.forbidden_resources.insert("pods".to_owned());
        });
        let list = get(&cluster, "/api/v1/cluster/nodes").await;
        assert_eq!(list["items"][0]["usage"]["cpuMillicores"], 480);
        assert_eq!(list["items"][0]["usage"]["memoryBytes"], 1 << 30);
        assert_eq!(list["metrics"], "available");
        assert_eq!(list["podScan"], "unreadable");
        assert!(list["items"][0].get("requests").is_none());
        assert_eq!(list["unreadable"][0]["resource"], "pods");

        cluster.update(|s| {
            s.forbidden_resources
                .insert("nodes.metrics.k8s.io".to_owned());
        });
        assert_eq!(
            get(&cluster, "/api/v1/cluster/nodes").await["metrics"],
            "unreadable"
        );
        cluster.update(|s| {
            s.forbidden_resources.insert("nodes".to_owned());
        });
        let (status, _, body) = call(app(&cluster), "GET", "/api/v1/cluster/nodes").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(body.contains("`list` on `nodes`"), "{body}");
    }

    #[tokio::test]
    async fn health_names_only_the_failing_checks_and_a_refusal_is_its_own_403() {
        let cluster = with_nodes();
        let health = get(&cluster, "/api/v1/cluster/health").await;
        assert_eq!(health["ready"], true);
        assert_eq!(health["failing"], json!([]));
        assert_eq!(health["leaders"], json!([]));

        cluster.update(|s| {
            s.readyz = "[+]ping ok\n[-]etcd failed: reason withheld\n[+]log ok\n\
                        readyz check failed\n"
                .to_owned();
        });
        let health = get(&cluster, "/api/v1/cluster/health").await;
        assert_eq!(health["ready"], false);
        assert_eq!(health["checks"], 3);
        assert_eq!(
            health["failing"],
            json!([{"name": "etcd", "reason": "reason withheld"}])
        );

        cluster.update(|s| {
            s.forbidden_resources.insert("/readyz".to_owned());
        });
        let (status, _, body) = call(app(&cluster), "GET", "/api/v1/cluster/health").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(body.contains("`/readyz`"), "{body}");
    }

    #[tokio::test]
    async fn upgrades_warn_of_end_of_life_from_the_bundled_table() {
        let cluster = with_nodes();
        let on = |clock: fn() -> Timestamp| {
            let app =
                zeabur_api::router_at(Arc::new(SingleCluster(Arc::new(cluster.clone()))), clock);
            async move {
                let (status, _, body) = call(app, "GET", "/api/v1/cluster/upgrades").await;
                assert_eq!(status, StatusCode::OK, "{body}");
                json(&body)
            }
        };
        let before = on(|| "2026-10-15T00:00:00Z".parse().unwrap()).await;
        assert_eq!(before["minor"], "1.34");
        assert_eq!(before["release"]["endOfLife"], "2026-10-27");
        assert!(before.get("patch").is_none() && before.get("nextMinor").is_none());
        assert!(before.get("updateCheck").is_none() && before.get("latestFrom").is_none());
        assert_eq!(before["states"], json!(["maintenance"]));
        let after = on(|| "2026-10-28T00:00:00Z".parse().unwrap()).await;
        assert_eq!(after["states"], json!(["end-of-life"]));
    }

    fn plans_served() -> ServedResource {
        ServedResource::readable("upgrade.cattle.io", "v1", "plans", "Plan", true)
    }

    #[tokio::test]
    async fn upgrades_take_newer_versions_from_the_plans() {
        let cluster = with_nodes();
        cluster.update(|s| {
            s.served.push(plans_served());
            s.dynamic.push(
                serde_json::from_value(json!({
                    "apiVersion": "upgrade.cattle.io/v1", "kind": "Plan",
                    "metadata": {"name": "server", "namespace": "system-upgrade"},
                    "spec": {"channel": "https://update.k3s.io/v1-release/channels/stable"},
                    "status": {"latestVersion": "v1.34.12-k3s1"}
                }))
                .unwrap(),
            );
        });
        let upgrades = get(&cluster, "/api/v1/cluster/upgrades").await;
        assert_eq!(upgrades["patch"], "v1.34.12-k3s1");
        assert_eq!(
            upgrades["extensions"]["systemUpgradeController"]["plans"][0]["latestVersion"],
            "v1.34.12-k3s1"
        );
        assert!(
            upgrades["states"]
                .as_array()
                .unwrap()
                .contains(&json!("patch-available"))
        );

        // Nodes the person may not list leave the version picture standing.
        cluster.update(|s| {
            s.forbidden_resources.insert("nodes".to_owned());
        });
        let upgrades = get(&cluster, "/api/v1/cluster/upgrades").await;
        assert_eq!(upgrades["unreadable"][0]["resource"], "nodes");
        assert_eq!(upgrades["kubernetesVersion"], "v1.34.10+k3s1");
    }

    #[tokio::test]
    async fn upgrades_with_plans_refused_say_so() {
        let cluster = with_nodes();
        cluster.update(|s| {
            s.forbidden_resources
                .insert("plans.upgrade.cattle.io".to_owned());
        });
        // Not installed: discovery says so before any permission matters, so nothing is called
        // missing.
        let upgrades = get(&cluster, "/api/v1/cluster/upgrades").await;
        assert!(
            upgrades.get("unreadable").is_none_or(|u| u == &json!([])),
            "{upgrades}"
        );

        // Installed but refused: say which permission is missing.
        cluster.update(|s| s.served.push(plans_served()));
        let upgrades = get(&cluster, "/api/v1/cluster/upgrades").await;
        assert_eq!(
            upgrades["unreadable"][0]["resource"],
            "plans.upgrade.cattle.io"
        );
        assert!(upgrades.get("patch").is_none());
    }

    #[tokio::test]
    async fn cluster_events_filter_by_type_and_age_and_say_what_to_ask_for() {
        let cluster = cluster();
        let recent: Event = serde_json::from_value(json!({
            "metadata": {"name": "recent", "namespace": "kube-system"},
            "eventTime": Timestamp::now().to_string(),
            "regarding": {"kind": "Node", "name": "vps-1", "apiVersion": "v1"},
            "type": "Warning", "reason": "NodeNotReady", "note": "x",
            "reportingController": "test", "reportingInstance": "test", "action": "x"
        }))
        .unwrap();
        cluster.update(|s| {
            s.events.insert("kube-system".to_owned(), vec![recent]);
        });
        let all = get(&cluster, "/api/v1/cluster/events").await;
        assert_eq!(all["totalSize"], 8);
        let newest = get(&cluster, "/api/v1/cluster/events?limit=1").await;
        assert_eq!(names(&newest).len(), 1);
        assert_eq!(newest["totalSize"], 8);
        assert_eq!(all["items"][0]["id"], "projects/kube-system/events/recent");
        assert_eq!(all["partial"], false);
        let warnings = get(
            &cluster,
            "/api/v1/cluster/events?type=warning&sinceSeconds=3600",
        )
        .await;
        assert_eq!(names(&warnings), ["recent"]);
        let (status, ..) = call(
            app(&cluster),
            "GET",
            "/api/v1/cluster/events?sinceSeconds=x",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        cluster.update(|s| {
            s.forbidden_resources
                .insert("events.events.k8s.io".to_owned());
        });
        let (status, _, body) = call(app(&cluster), "GET", "/api/v1/cluster/events").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(body.contains("events.k8s.io"), "{body}");
    }

    #[tokio::test]
    async fn cluster_scope_keeps_events_about_the_cluster_itself() {
        let cluster = cluster();
        let event = |name: &str, namespace: &str, kind: &str| -> Event {
            serde_json::from_value(json!({
                "metadata": {"name": name, "namespace": namespace},
                "eventTime": Timestamp::now().to_string(),
                "regarding": {"kind": kind, "name": "x", "apiVersion": "v1"},
                "type": "Warning", "reason": "R", "note": "x",
                "reportingController": "test", "reportingInstance": "test", "action": "x"
            }))
            .unwrap()
        };
        cluster.update(|s| {
            s.events.insert(
                "kube-system".to_owned(),
                vec![event("system", "kube-system", "Pod")],
            );
            s.events.insert(
                "default".to_owned(),
                vec![
                    event("node", "default", "Node"),
                    event("app", "default", "Pod"),
                ],
            );
        });
        let scoped = get(
            &cluster,
            "/api/v1/cluster/events?scope=cluster&type=warning",
        )
        .await;
        let mut found = names(&scoped);
        found.sort();
        assert_eq!(found, ["node", "system"]);
        assert_eq!(scoped["totalSize"], 2);
        let all = get(&cluster, "/api/v1/cluster/events?scope=all&type=warning").await;
        assert!(names(&all).contains(&"app".to_owned()), "{all}");
        let (status, ..) = call(app(&cluster), "GET", "/api/v1/cluster/events?scope=x").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn installed_components_carry_versions_and_upgrades() {
        let cluster = cluster();
        cluster.update(|s| {
            s.facts.version = "v1.36.5+k3s1".to_owned();
            s.facts.workload_labels.push((
                "Deployment".to_owned(),
                BTreeMap::from([(
                    "helm.sh/chart".to_owned(),
                    "cert-manager-v1.19.1".to_owned(),
                )]),
            ));
        });
        let certificates = |list: &Value| {
            list["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["name"] == "certificates")
                .unwrap()["installations"][0]
                .clone()
        };
        let installed = certificates(&get(&cluster, "/api/v1/cluster/capabilities").await);
        assert_eq!(installed["source"], "helm");
        assert_eq!(installed["version"], "v1.19.1");
        assert_eq!(installed["upgrade"], "behind-minor");
        assert_eq!(installed["tested"], "v1.21.2");
    }

    #[tokio::test]
    async fn kube_apiservers_retry_after_is_passed_on() {
        let cluster = cluster();
        cluster.update(|s| s.busy = Some(2));
        let (status, headers, _) = send(app(&cluster), "GET", "/api/v1/projects", &[]).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(headers.get("retry-after").unwrap(), "2");
    }

    #[tokio::test]
    async fn a_malformed_log_parameter_is_a_bad_request() {
        let (status, _, body) = call(
            app(&cluster()),
            "GET",
            "/api/v1/projects/shop/pods/web-1/logs?tailLines=ten",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            json(&body)["detail"]
                .as_str()
                .unwrap()
                .contains("tailLines")
        );
    }

    #[tokio::test]
    async fn unknown_api_paths_are_problems() {
        for uri in [
            "/api/v1/nothing",
            "/api/v1/projects/shop/services/deployment/web/nothing",
        ] {
            let (status, _, body) = call(app(&cluster()), "GET", uri).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
            assert_eq!(json(&body)["type"], "/problems/not-found");
        }
    }

    #[test]
    fn every_operation_has_its_own_id() {
        // Generated clients key operations by id: two handlers both named `get` would make one
        // path's answer typed as the other's.
        let description = serde_json::to_value(zeabur_api::openapi()).unwrap();
        let mut ids = Vec::new();
        for item in description["paths"].as_object().unwrap().values() {
            for operation in item.as_object().unwrap().values() {
                if let Some(id) = operation.get("operationId").and_then(Value::as_str) {
                    ids.push(id.to_owned());
                }
            }
        }
        let count = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate operationId among {ids:?}");
    }

    /// Every schema an answer uses is in the description as that type describes it.
    ///
    /// The description gathers component schemas by name, and a second type with the same name
    /// silently replaces the first (a module's `Upgrades` once replaced the cluster's), leaving
    /// generated clients with the wrong shape. Comparing each answer type's own schemas, nested
    /// ones included, with the description catches that.
    #[test]
    fn no_schema_is_replaced_by_another_of_the_same_name() {
        use utoipa::ToSchema;
        use zeabur_api::views::*;

        fn own<T: ToSchema>(into: &mut Vec<(String, Value)>) {
            let mut schemas = vec![(T::name().into_owned(), T::schema())];
            T::schemas(&mut schemas);
            into.extend(
                schemas
                    .into_iter()
                    .map(|(name, schema)| (name, serde_json::to_value(schema).unwrap())),
            );
        }
        let mut expected = Vec::new();
        own::<WhoAmI>(&mut expected);
        own::<Cluster>(&mut expected);
        own::<CapabilityList>(&mut expected);
        own::<NodeList>(&mut expected);
        own::<Upgrades>(&mut expected);
        own::<ClusterEventList>(&mut expected);
        own::<zeabur_api::resources::objects::ObjectList>(&mut expected);
        own::<zeabur_api::resources::objects::ObjectDocument>(&mut expected);
        own::<zeabur_api::services::releases::ReleaseList>(&mut expected);
        own::<zeabur_plan::PlanRequest>(&mut expected);
        own::<zeabur_api::projects::writes::PlanResponse>(&mut expected);
        own::<zeabur_api::projects::writes::ApplyRequest>(&mut expected);
        own::<zeabur_api::projects::writes::ApplyResponse>(&mut expected);
        own::<zeabur_api::gaps::GapList>(&mut expected);
        own::<zeabur_api::cluster::install::InstallAnswer>(&mut expected);
        own::<ProjectList>(&mut expected);
        own::<ProjectDocument>(&mut expected);
        own::<ServiceDocument>(&mut expected);
        own::<EventList>(&mut expected);
        own::<LogSnapshot>(&mut expected);
        own::<ConfigMapList>(&mut expected);
        own::<RevealedSecret>(&mut expected);
        own::<zeabur_api::projects::usage::ProjectUsage>(&mut expected);
        own::<zeabur_api::services::history::ServiceHistory>(&mut expected);
        own::<zeabur_api::settings::MetricsStoreSetting>(&mut expected);
        own::<zeabur_api::settings::MetricsStoreChange>(&mut expected);
        own::<zeabur_core::problem::Problem>(&mut expected);

        let description = serde_json::to_value(zeabur_api::openapi()).unwrap();
        let components = &description["components"]["schemas"];
        for (name, schema) in expected {
            assert_eq!(
                components.get(&name),
                Some(&schema),
                "the description's schema {name:?} is not the one its type gives"
            );
        }
    }

    #[tokio::test]
    async fn a_crash_looping_service_carries_its_diagnosis() {
        let document = get(&cluster(), "/api/v1/projects/shop").await;
        let diagnoses = &document["services"][0]["diagnoses"];
        assert_eq!(diagnoses[0]["kind"], "crashLoop", "{diagnoses:#}");
        assert_eq!(diagnoses[0]["container"], "web");
        assert!(
            diagnoses[0]["cause"]["key"]
                .as_str()
                .unwrap()
                .starts_with("diagnosis.")
        );
    }

    #[tokio::test]
    async fn project_gaps_carry_citations_and_a_snippet_to_copy() {
        let gaps = get(&cluster(), "/api/v1/projects/shop/gaps").await;
        let limits = gaps["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|g| g["rule"] == "resource-limits")
            .expect("the web container sets no resources");
        assert_eq!(limits["severity"], "warning");
        assert!(
            limits["citations"][0]["url"]
                .as_str()
                .unwrap()
                .starts_with("https://")
        );
        let snippet = limits["snippet"].as_str().unwrap();
        assert!(snippet.starts_with("# Deployment/web\n"), "{snippet}");
        assert!(snippet.contains("resources:"), "{snippet}");
        let cluster_gaps = get(&cluster(), "/api/v1/cluster/gaps").await;
        assert_eq!(cluster_gaps["items"], json!([]));
    }

    /// A route to a Service the namespace lacks is a gap, unless the Services could not be
    /// listed: then their absence from the read proves nothing.
    #[tokio::test]
    async fn a_missing_backend_is_a_gap_only_when_the_services_could_be_listed() {
        let cluster = cluster();
        cluster.update(|s| {
            s.objects.get_mut("shop").unwrap().http_routes.push(json!({
                "apiVersion": "gateway.networking.k8s.io/v1", "kind": "HTTPRoute",
                "metadata": {"name": "typo", "namespace": "shop"},
                "spec": {"rules": [{"backendRefs": [{"name": "frontned", "port": 80}]}]}
            }));
        });
        let no_service = |gaps: &Value| {
            gaps["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|g| g["message"]["key"] == "gap.danglingRoute.noService")
                .cloned()
        };
        let readable = get(&cluster, "/api/v1/projects/shop/gaps").await;
        let gap = no_service(&readable).expect("frontned does not exist");
        assert_eq!(gap["message"]["params"]["services"], "frontned");

        cluster.update(|s| {
            s.forbidden_in_namespaces.insert("services".to_owned());
        });
        let refused = get(&cluster, "/api/v1/projects/shop/gaps").await;
        assert_eq!(no_service(&refused), None, "{refused}");
        assert!(
            refused["unreadable"]
                .as_array()
                .unwrap()
                .iter()
                .any(|u| u["resource"] == "services"),
            "{refused}"
        );
    }

    /// A workload a route sends to by name is diagnosed as having no Service, unless the
    /// Services could not be listed: then their absence from the read proves nothing. The other
    /// diagnoses stay.
    #[tokio::test]
    async fn no_service_is_diagnosed_only_when_the_services_could_be_listed() {
        let cluster = cluster();
        cluster.update(|s| {
            s.objects.get_mut("shop").unwrap().http_routes.push(json!({
                "apiVersion": "gateway.networking.k8s.io/v1", "kind": "HTTPRoute",
                "metadata": {"name": "web", "namespace": "shop"},
                "spec": {"rules": [{"backendRefs": [{"name": "web", "port": 80}]}]}
            }));
        });
        let causes = |document: &Value| -> Vec<String> {
            document["services"][0]["diagnoses"]
                .as_array()
                .unwrap()
                .iter()
                .map(|d| d["cause"]["key"].as_str().unwrap().to_owned())
                .collect()
        };
        let no_service = "diagnosis.unreachable.cause.noService".to_owned();
        let readable = get(&cluster, "/api/v1/projects/shop").await;
        assert!(causes(&readable).contains(&no_service), "{readable}");

        cluster.update(|s| {
            s.forbidden_in_namespaces.insert("services".to_owned());
        });
        let refused = get(&cluster, "/api/v1/projects/shop").await;
        let refused_causes = causes(&refused);
        assert!(!refused_causes.contains(&no_service), "{refused}");
        assert!(
            refused_causes
                .iter()
                .any(|c| c.starts_with("diagnosis.crashLoop.")),
            "{refused}"
        );
        assert!(
            refused["unreadable"]
                .as_array()
                .unwrap()
                .iter()
                .any(|u| u["resource"] == "services"),
            "{refused}"
        );
    }

    #[tokio::test]
    async fn a_project_without_network_policy_is_one_gap_rated_by_enforcement() {
        let cluster = cluster();
        let find = |gaps: &Value| {
            gaps["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|g| g["rule"] == "no-network-policy")
                .cloned()
        };
        // No k3s node to read: whether policies are enforced is unknown.
        let unknown = find(&get(&cluster, "/api/v1/projects/shop/gaps").await).unwrap();
        assert_eq!(unknown["severity"], "advice");
        assert_eq!(unknown["message"]["params"]["enforcement"], "unknown");
        assert_eq!(unknown["fix"]["id"], "networkPolicy.isolateProject");
        let k3s = |args: &str| -> Node {
            serde_json::from_value(json!({"metadata": {"name": "vps-1",
                "annotations": {"k3s.io/node-args": args}}}))
            .unwrap()
        };
        assert!(
            !unknown["fix"]["params"]["objects"]
                .as_str()
                .unwrap()
                .contains("zeabur-allow-ingress-web")
        );
        // A k3s server with its policy controller on, and an HTTPRoute sending traffic to web.
        cluster.update(|s| {
            s.nodes = vec![k3s(r#"["server"]"#)];
            let shop = s.objects.get_mut("shop").unwrap();
            shop.services.push(
                serde_json::from_value(json!({
                    "metadata": {"name": "web", "namespace": "shop"},
                    "spec": {"selector": {"app": "web"}, "ports": [{"port": 80}]}
                }))
                .unwrap(),
            );
            shop.http_routes.push(json!({
                "apiVersion": "gateway.networking.k8s.io/v1", "kind": "HTTPRoute",
                "metadata": {"name": "web", "namespace": "shop"},
                "spec": {"rules": [{"backendRefs": [{"name": "web", "port": 80}]}]}
            }));
        });
        let enforced = find(&get(&cluster, "/api/v1/projects/shop/gaps").await).unwrap();
        assert_eq!(enforced["severity"], "warning");
        assert_eq!(enforced["subject"]["name"], "shop");
        assert_eq!(enforced["message"]["params"]["services"], "web");
        let objects: Value =
            serde_json::from_str(enforced["fix"]["params"]["objects"].as_str().unwrap()).unwrap();
        assert_eq!(objects[1]["metadata"]["name"], "zeabur-allow-ingress-web");
        cluster.update(|s| s.nodes = vec![k3s(r#"["server","--disable-network-policy"]"#)]);
        assert!(find(&get(&cluster, "/api/v1/projects/shop/gaps").await).is_none());
        cluster.update(|s| {
            s.nodes = vec![k3s(r#"["server"]"#)];
            s.dynamic.push(
                serde_json::from_value(json!({
                    "apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
                    "metadata": {"name": "default-deny", "namespace": "shop"},
                    "spec": {"podSelector": {}}
                }))
                .unwrap(),
            );
        });
        assert!(find(&get(&cluster, "/api/v1/projects/shop/gaps").await).is_none());
    }

    #[tokio::test]
    async fn install_texts_come_in_four_forms_for_the_profile_asked() {
        let cluster = cluster();
        let guide = get(
            &cluster,
            "/api/v1/cluster/components/certificates/cert-manager/install",
        )
        .await;
        assert_eq!(guide["defaultProfile"], "single-node");
        assert_eq!(guide["profile"], "single-node");
        let formats: Vec<&str> = guide["formats"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["format"].as_str().unwrap())
            .collect();
        assert_eq!(formats, ["helm", "k3s", "flux", "argoCd"]);
        assert!(
            guide["formats"][0]["text"]
                .as_str()
                .unwrap()
                .contains("helm upgrade --install cert-manager oci://")
        );
        let multi = get(
            &cluster,
            "/api/v1/cluster/components/certificates/cert-manager/install?profile=multi-node",
        )
        .await;
        assert_eq!(multi["profile"], "multi-node");
        let (status, ..) = call(
            app(&cluster),
            "GET",
            "/api/v1/cluster/components/certificates/nope/install",
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, ..) = call(
            app(&cluster),
            "GET",
            "/api/v1/cluster/components/certificates/cert-manager/install?profile=huge",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn the_served_description_is_the_committed_one() {
        let committed =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/openapi.yaml"))
                .unwrap_or_default();
        let generated = serde_saphyr::to_string(&zeabur_api::openapi()).unwrap();
        if std::env::var_os("ZEABUR_UPDATE_OPENAPI").is_some() {
            std::fs::write(
                concat!(env!("CARGO_MANIFEST_DIR"), "/openapi.yaml"),
                &generated,
            )
            .unwrap();
            return;
        }
        assert!(
            committed == generated,
            "crates/zeabur-api/openapi.yaml is stale; regenerate it with \
             ZEABUR_UPDATE_OPENAPI=1 cargo nextest run -p zeabur-api"
        );
        let (status, _, body) = call(app(&cluster()), "GET", "/api/openapi.json").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json(&body)["openapi"], "3.1.0");
    }
}
