use std::collections::BTreeMap;

use axum::http::{HeaderMap, StatusCode as HttpStatus, header};
use axum::response::IntoResponse as _;
use serde_json::json;
use zeabur_core::crypto::{hex, sha256};
use zeabur_kube::k8s_openapi::api::core::v1::Secret;
use zeabur_kube::memory::{MemoryCluster, State};
use zeabur_kube::{NamespaceObjects, SecretSummary, ServedResource};

use super::*;

fn database(name: &str, schedule: &str) -> DynamicObject {
    serde_json::from_value(json!({
        "apiVersion": "postgresql.cnpg.io/v1", "kind": "Cluster",
        "metadata": {"uid": name, "name": name, "namespace": "shop", "resourceVersion": "42",
            "annotations": {"keel.sh/policy":"force", "keel.sh/match-tag":"true", "keel.sh/trigger":"poll",
                "keel.sh/pollSchedule":schedule, "kubectl.kubernetes.io/last-applied-configuration": "original"}
        },
        "spec":{"imageName":format!("registry.test/db:17-standard@sha256:{}", "1".repeat(64)),
            "instances":1, "storage":{"size":"1Gi"}},
        "status":{"readyInstances":1,"conditions":[{"type":"Ready","status":"True"}]}
    })).unwrap()
}

fn at(time: &str) -> Timestamp {
    time.parse().unwrap()
}

#[test]
fn checks_are_due_per_uid_even_when_images_are_shared() {
    let a = database("active", "@every 15s");
    let future = database("future", "0 0 3 * * 0");
    let mut off = database("off", "@every 15s");
    off.metadata.annotations = None;
    let mut objects = vec![a, future, off];
    let mut jobs = HashMap::new();
    let now = at("2026-11-27T00:00:00Z");
    assert_eq!(scheduled(&mut jobs, &objects, now), []);
    assert_eq!(jobs.len(), 2);
    objects[0].metadata.resource_version = Some("45".into());
    let due = scheduled(&mut jobs, &objects, at("2026-11-27T00:00:15Z"));
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].0.name, "active");
    objects[0]
        .metadata
        .annotations
        .as_mut()
        .unwrap()
        .insert("keel.sh/pollSchedule".into(), "@hourly".into());
    assert_eq!(
        scheduled(&mut jobs, &objects, at("2026-11-27T00:00:30Z")),
        []
    );
    assert_eq!(scheduled(&mut jobs, &[], now), []);
    assert!(jobs.is_empty());
}

#[test]
fn calendar_checks_use_utc_and_skip_impossible_schedules() {
    let now = at("2026-11-27T12:00:00Z");
    for (schedule, next) in [
        ("@hourly", "2026-11-27T13:00:00Z"),
        ("@midnight", "2026-11-28T00:00:00Z"),
        ("0 0 3 * *", "2026-11-28T03:00:00Z"),
        ("0/15 * * * * *", "2026-11-27T12:00:15Z"),
        ("0 0 3 */2 * MON", "2026-12-07T03:00:00Z"),
        ("0 0 3 29 * MON", "2026-11-29T03:00:00Z"),
        ("0 0 3 * * SUN", "2026-11-29T03:00:00Z"),
        ("0 0 3 * * *", "2026-11-28T03:00:00Z"),
    ] {
        assert_eq!(
            Target::read(&database("db", schedule)).unwrap().next(now),
            Some(at(next)),
            "{schedule}"
        );
    }
    let year_end = at("2027-12-31T23:59:59.900Z");
    for (schedule, next) in [
        ("* * * * * *", "2028-01-01T00:00:00Z"),
        ("0 0 0 29 FEB *", "2028-02-29T00:00:00Z"),
        ("@every 1500ms", "2028-01-01T00:00:00Z"),
    ] {
        assert_eq!(
            Target::read(&database("db", schedule))
                .unwrap()
                .next(year_end),
            Some(at(next))
        );
    }
    let saturday = at("2026-11-28T12:00:00Z");
    assert_eq!(
        Target::read(&database("db", "0 0 3 * * 1/2"))
            .unwrap()
            .next(saturday),
        Some(at("2026-11-30T03:00:00Z"))
    );
    for schedule in [
        "0 0 0 31 FEB *",
        "0 0 3 * * 7",
        "0 0 3 * * MON-SUN",
        "@every 0s",
        "0 0 3 L * *",
        "0 0 3 * * * 2028",
        "bad",
    ] {
        assert_eq!(
            Target::read(&database("db", schedule)).unwrap().next(now),
            None
        );
    }
}

fn node(name: &str, arch: &str) -> Node {
    serde_json::from_value(json!({"metadata":{"name":name,"labels":{"kubernetes.io/arch":arch, "kubernetes.io/os":"linux", "pool":"db"}}})).unwrap()
}

#[test]
fn architecture_checks_consider_where_cnpg_may_schedule_instances() {
    let nodes = [node("first", "amd64"), node("second", "arm64")];
    let mut db = database("db", "@hourly");
    assert_eq!(
        Target::read(&db)
            .unwrap()
            .architectures(&nodes)
            .unwrap()
            .len(),
        2
    );
    db.data["spec"]["affinity"] = json!({"nodeSelector":{"pool":"db"}, "nodeAffinity":{"requiredDuringSchedulingIgnoredDuringExecution":{
        "nodeSelectorTerms":[{"matchFields":[{"key":"metadata.name","operator":"In","values":["second"]}]}]}}});
    assert_eq!(
        Target::read(&db)
            .unwrap()
            .architectures(&nodes)
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>(),
        ["arm64"]
    );
    db.data["spec"]["affinity"]["nodeSelector"]["pool"] = json!("none");
    assert!(Target::read(&db).unwrap().architectures(&nodes).is_err());
}

fn memory(db: DynamicObject) -> MemoryCluster {
    MemoryCluster::new(State {
        served: vec![ServedResource::readable(
            "postgresql.cnpg.io",
            "v1",
            "clusters",
            "Cluster",
            true,
        )],
        dynamic: vec![db],
        nodes: vec![node("first", "amd64")],
        objects: BTreeMap::from([(
            "shop".into(),
            NamespaceObjects {
                namespace: "shop".into(),
                ..Default::default()
            },
        )]),
        ..Default::default()
    })
}

#[tokio::test]
async fn an_old_check_cannot_overwrite_a_new_owner_image_schedule_or_recreated_cluster() {
    let db = database("db", "@hourly");
    let target = Target::read(&db).unwrap();
    for (path, value) in [
        ("/metadata/uid", json!("another")),
        ("/metadata/annotations/keel.sh~1policy", json!("never")),
        ("/metadata/resourceVersion", json!("43")),
        ("/spec/imageName", json!("postgres:18-standard")),
        ("/status/readyInstances", json!(0)),
    ] {
        let mut changed = serde_json::to_value(&db).unwrap();
        *changed.pointer_mut(path).unwrap() = value;
        let cluster = memory(serde_json::from_value(changed).unwrap());
        apply(
            &cluster,
            &target,
            "registry.test/db:17-standard",
            &format!("sha256:{}", "2".repeat(64)),
        )
        .await
        .unwrap();
        let current = cluster
            .find_object(&CLUSTERS.resource(), Some("shop"), "db")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            current.data["spec"]["imageName"]
                .as_str()
                .unwrap()
                .contains(&"2".repeat(64)),
            path == "/metadata/resourceVersion",
            "{path}"
        );
    }
}

#[tokio::test]
async fn a_conflict_rechecks_off_and_retries_only_when_the_setting_is_still_current() {
    for disable in [true, false] {
        let db = database("db", "@hourly");
        let target = Target::read(&db).unwrap();
        let state = Arc::new(std::sync::Mutex::new((db.clone(), Vec::new())));
        let reads = Arc::clone(&state);
        let writes = Arc::clone(&state);
        let router = Router::new().route(
            "/apis/postgresql.cnpg.io/v1/namespaces/shop/clusters/db",
            get(move || async move {
                let mut state = reads.lock().unwrap();
                state.1.push("GET");
                axum::Json(state.0.clone())
            })
            .patch(move |axum::Json(patch): axum::Json<serde_json::Value>| async move {
                let mut state = writes.lock().unwrap();
                let (current, requests) = &mut *state;
                requests.push("PATCH");
                assert_eq!(patch["metadata"]["uid"], "db");
                if current.metadata.resource_version.as_deref() == Some("42") {
                    assert_eq!(patch["metadata"]["resourceVersion"], "42");
                    // A concurrent status write or the user's Off lands before this patch.
                    current.metadata.resource_version = Some("43".into());
                    if disable {
                        current.metadata.annotations.as_mut().unwrap().remove("keel.sh/policy");
                    }
                    drop(state);
                    return (HttpStatus::CONFLICT, axum::Json(json!({"apiVersion":"v1", "kind":"Status",
                        "status":"Failure", "message":"resourceVersion changed", "reason":"Conflict", "code":409}))).into_response();
                }
                assert_eq!(patch["metadata"]["resourceVersion"], "43");
                current.data["spec"]["imageName"] = patch["spec"]["imageName"].clone();
                let updated = current.clone();
                drop(state);
                axum::Json(updated).into_response()
            }),
        );
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let server = tokio::spawn(axum::serve(listener, router).into_future());
        let client = UserClient::with_token(
            &ClusterEndpoint::new(url, None),
            &zeabur_kube::UserToken::new("test".into()).unwrap(),
        )
        .unwrap();
        let tagged = "registry.test/db:17-standard";
        let digest = format!("sha256:{}", "2".repeat(64));
        apply(&client, &target, tagged, &digest).await.unwrap();
        server.abort();
        let (current, requests) = &*state.lock().unwrap();
        if disable {
            assert_eq!(requests, &["GET", "PATCH", "GET"]);
            assert_eq!(current.data["spec"], db.data["spec"]);
            assert!(
                !current
                    .metadata
                    .annotations
                    .as_ref()
                    .unwrap()
                    .contains_key("keel.sh/policy")
            );
        } else {
            assert_eq!(requests, &["GET", "PATCH", "GET", "PATCH"]);
            assert_eq!(
                current.data["spec"]["imageName"],
                format!("{tagged}@{digest}")
            );
        }
    }
}

async fn private_registry() -> (String, String, tokio::task::JoinHandle<std::io::Result<()>>) {
    let config = r#"{"os":"linux","architecture":"amd64"}"#;
    let config_digest = format!("sha256:{}", hex(&sha256(config.as_bytes())));
    let manifest = json!({"schemaVersion":2,"mediaType":"application/vnd.oci.image.manifest.v1+json","config":{"digest":config_digest}}).to_string();
    let digest = format!("sha256:{}", hex(&sha256(manifest.as_bytes())));
    let router = Router::new().fallback(get(move |headers: HeaderMap, uri: axum::http::Uri| {
        let manifest = manifest.clone();
        async move {
            if headers
                .get(header::AUTHORIZATION)
                .and_then(|h| h.to_str().ok())
                != Some("Basic dTpw")
            {
                return (
                    HttpStatus::UNAUTHORIZED,
                    [(header::WWW_AUTHENTICATE, "Basic realm=\"registry\"")],
                )
                    .into_response();
            }
            if uri.path().contains("/blobs/") {
                config.into_response()
            } else {
                manifest.into_response()
            }
        }
    }));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let host = listener.local_addr().unwrap().to_string();
    let server = tokio::spawn(axum::serve(listener, router).into_future());
    (host, digest, server)
}

#[tokio::test]
async fn a_private_registry_updates_only_the_digest_with_either_docker_secret_type() {
    let (host, digest, server) = private_registry().await;
    let transport = zeabur_http::Https::with_proxies(
        zeabur_http::Reach::Any,
        &zeabur_http::tls(None).unwrap(),
        zeabur_http::Proxies::none(),
    );
    for (kind, mixed_architectures) in [
        ("kubernetes.io/dockerconfigjson", false),
        ("kubernetes.io/dockercfg", false),
        ("Opaque", false),
        ("kubernetes.io/dockerconfigjson", true),
    ] {
        let (key, config) = if kind == "kubernetes.io/dockercfg" {
            (".dockercfg", json!({&host:{"auth":"dTpw"}}))
        } else {
            (
                ".dockerconfigjson",
                json!({"auths":{&host:{"auth":"dTpw"}}}),
            )
        };
        let mut db = database("db", "@hourly");
        let tagged = format!("{host}/pg:17-standard");
        db.data["spec"]["imageName"] = json!(format!("{tagged}@sha256:{}", "a".repeat(64)));
        db.data["spec"]["imagePullSecrets"] = json!([{"name":"pull"}]);
        let cluster = memory(db.clone());
        cluster.update(|s| {
            if mixed_architectures {
                s.nodes.push(node("second", "arm64"));
            }
            let secret: Secret = serde_json::from_value(
                json!({"metadata":{"name":"pull","namespace":"shop"}, "type":kind}),
            )
            .unwrap();
            s.objects
                .get_mut("shop")
                .unwrap()
                .secrets
                .push(SecretSummary {
                    secret,
                    key_count: 1,
                });
            s.secret_values.insert(
                "shop/pull".into(),
                BTreeMap::from([(key.into(), config.to_string().into_bytes())]),
            );
        });
        let result = check(
            &cluster,
            &transport,
            std::slice::from_ref(&host),
            &Target::read(&db).unwrap(),
            &db,
        )
        .await;
        let current = cluster
            .find_object(&CLUSTERS.resource(), Some("shop"), "db")
            .await
            .unwrap()
            .unwrap();
        if kind == "Opaque" {
            assert!(result.is_err());
            assert_eq!(current.data, db.data);
        } else if mixed_architectures {
            assert_eq!(
                result.unwrap_err(),
                "published image does not support every eligible node architecture"
            );
            assert_eq!(cluster.inspect(|s| s.writes.len()), Some(0));
            assert_eq!(current.data["spec"], db.data["spec"]);
        } else {
            result.unwrap();
            let mut expected = db.data.clone();
            expected["spec"]["imageName"] = json!(format!("{tagged}@{digest}"));
            assert_eq!(current.data.get("spec"), expected.get("spec"));
            assert_eq!(
                current.metadata.annotations.as_ref().unwrap()["kubectl.kubernetes.io/last-applied-configuration"],
                "original"
            );
        }
    }
    server.abort();
}
