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
    assert!(scheduled(&mut jobs, &objects, now).is_empty());
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
    assert!(scheduled(&mut jobs, &objects, at("2026-11-27T00:00:30Z")).is_empty());
    assert!(scheduled(&mut jobs, &[], now).is_empty());
    assert!(jobs.is_empty());
}

#[test]
fn calendar_checks_use_utc_and_skip_impossible_schedules() {
    let now = at("2026-11-27T12:00:00Z");
    for (schedule, next) in [
        ("@hourly", "2026-11-27T13:00:00Z"),
        ("0 0 3 * * SUN", "2026-11-29T03:00:00Z"),
        ("0 0 3 * * *", "2026-11-28T03:00:00Z"),
    ] {
        assert_eq!(
            Target::read(&database("db", schedule)).unwrap().next(now),
            Some(at(next))
        );
    }
    for schedule in ["0 0 0 31 FEB *", "@every 0s", "bad"] {
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
async fn a_private_registry_updates_only_the_digest_with_either_docker_secret_type() {
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
    let transport = zeabur_http::Https::with_proxies(
        zeabur_http::Reach::Any,
        &zeabur_http::tls(None).unwrap(),
        zeabur_http::Proxies::none(),
    );
    for (kind, key, config) in [
        (
            "kubernetes.io/dockerconfigjson",
            ".dockerconfigjson",
            json!({"auths":{&host:{"auth":"dTpw"}}}),
        ),
        (
            "kubernetes.io/dockercfg",
            ".dockercfg",
            json!({&host:{"auth":"dTpw"}}),
        ),
        (
            "Opaque",
            ".dockerconfigjson",
            json!({"auths":{&host:{"auth":"dTpw"}}}),
        ),
    ] {
        let mut db = database("db", "@hourly");
        let tagged = format!("{host}/pg:17-standard");
        db.data["spec"]["imageName"] = json!(format!("{tagged}@sha256:{}", "a".repeat(64)));
        db.data["spec"]["imagePullSecrets"] = json!([{"name":"pull"}]);
        let cluster = memory(db.clone());
        cluster.update(|s| {
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
