//! The file manager on a real cluster, as the person: a claim's folder is listed and a file comes
//! back byte for byte, odd names stay whole and run nothing, a Secret's files are named but never
//! read, a refused `pods/exec` names the right it needs, and an image without `tar` (the pause
//! image) has its volume read through a debug container that mounts it.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use kube::api::Api;
use serde_json::{Value, json};
use tower::ServiceExt as _;
use zeabur_api::access::SingleCluster;
use zeabur_kube::k8s_openapi::api::core::v1::{PersistentVolumeClaim, Pod, Secret};
use zeabur_test_support::sandbox::{Sandbox, create};

use crate::common::send;

/// Long enough for a node to pull an image and provision a volume.
const WAIT: Duration = Duration::from_secs(180);

const SECRET_VALUE: &str = "s3cr3t-file-that-must-not-leak";

fn app(sandbox: &Sandbox) -> Router {
    zeabur_api::router(Arc::new(SingleCluster(Arc::new(sandbox.user().clone()))))
}

fn rules(exec: bool) -> Value {
    let mut rules = vec![
        json!({"apiGroups": [""], "resources": ["pods"], "verbs": ["get", "list"]}),
        json!({"apiGroups": [""], "resources": ["pods/ephemeralcontainers"],
               "verbs": ["get", "patch"]}),
    ];
    if exec {
        rules.push(json!({"apiGroups": [""], "resources": ["pods/exec"],
                          "verbs": ["get", "create"]}));
    }
    Value::Array(rules)
}

/// Every byte value, 300 times: what the pod writes to `/data/bytes.bin`.
fn bytes() -> Vec<u8> {
    (0..=255_u8).cycle().take(256 * 300).collect()
}

/// A claim and a pod that mounts it at `/data`, with `image` and `command`, running.
async fn pod(sandbox: &Sandbox, name: &str, image: &str, command: Option<Value>, extra: Value) {
    let ns = sandbox.namespace();
    let claims = Api::<PersistentVolumeClaim>::namespaced(sandbox.admin().clone(), ns);
    create(
        &claims,
        json!({"metadata": {"name": name},
               "spec": {"accessModes": ["ReadWriteOnce"],
                        "resources": {"requests": {"storage": "16Mi"}}}}),
    )
    .await;
    let mut container = json!({"name": "app", "image": image,
        "volumeMounts": [{"name": "data", "mountPath": "/data"}]});
    if let Some(command) = command {
        container["command"] = command;
    }
    let mut volumes = vec![json!({"name": "data", "persistentVolumeClaim": {"claimName": name}})];
    if let Some(more) = extra["volumes"].as_array() {
        volumes.extend(more.iter().cloned());
    }
    if let Some(more) = extra["volumeMounts"].as_array()
        && let Some(list) = container["volumeMounts"].as_array_mut()
    {
        list.extend(more.iter().cloned());
    }
    let pods = Api::<Pod>::namespaced(sandbox.admin().clone(), ns);
    create(
        &pods,
        json!({"metadata": {"name": name},
               "spec": {"securityContext": {"runAsUser": 65535, "runAsGroup": 65535,
                                            "fsGroup": 65535, "runAsNonRoot": true},
                        "containers": [container], "volumes": volumes}}),
    )
    .await;
    let running = async {
        loop {
            let pod = pods.get(name).await.unwrap();
            let ready = pod
                .status
                .and_then(|s| s.container_statuses)
                .is_some_and(|c| c.iter().all(|c| c.ready));
            if ready {
                return;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    };
    tokio::time::timeout(WAIT, running)
        .await
        .expect("the pod runs");
}

fn query(pairs: &[(&str, &str)]) -> String {
    let mut query = form_urlencoded::Serializer::new(String::new());
    for (name, value) in pairs {
        query.append_pair(name, value);
    }
    query.finish()
}

async fn raw(app: &Router, request: Request<Body>) -> (StatusCode, Vec<u8>) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, bytes.to_vec())
}

async fn download(app: &Router, base: &str, pairs: &[(&str, &str)]) -> (StatusCode, Vec<u8>) {
    let request = Request::get(format!("{base}/content?{}", query(pairs)))
        .body(Body::empty())
        .unwrap();
    raw(app, request).await
}

async fn upload(app: &Router, base: &str, pairs: &[(&str, &str)], body: &[u8]) -> StatusCode {
    let request = Request::put(format!("{base}/content?{}", query(pairs)))
        .header("content-length", body.len())
        .header("zeabur-request", "1")
        .body(Body::from(body.to_vec()))
        .unwrap();
    let (status, answer) = raw(app, request).await;
    assert!(status.is_success(), "{}", String::from_utf8_lossy(&answer));
    status
}

fn names(listing: &Value) -> Vec<String> {
    listing["items"]
        .as_array()
        .unwrap_or_else(|| panic!("{listing}"))
        .iter()
        .map(|e| e["name"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
#[ignore = "needs a disposable cluster"]
async fn a_claims_files_are_listed_and_come_back_byte_for_byte() {
    let sandbox = Sandbox::new("zeabur-files", rules(true)).await;
    let ns = sandbox.namespace().to_owned();
    let secrets = Api::<Secret>::namespaced(sandbox.admin().clone(), &ns);
    create(
        &secrets,
        json!({"metadata": {"name": "db"}, "stringData": {"password": SECRET_VALUE}}),
    )
    .await;
    let script = "i=0; while [ $i -lt 256 ]; do printf \"\\\\$(printf %o $i)\"; i=$((i+1)); \
                  done > /tmp/b; j=0; while [ $j -lt 300 ]; do cat /tmp/b; j=$((j+1)); done \
                  > /data/bytes.bin; mkdir -p /data/reports; echo hello > /data/hello.txt; \
                  ln -s /run/secrets/db/password /data/shortcut; exec sleep 3600";
    pod(
        &sandbox,
        "store",
        "busybox:1.37",
        Some(json!(["sh", "-c", script])),
        json!({"volumes": [{"name": "db", "secret": {"secretName": "db"}}],
               "volumeMounts": [{"name": "db", "mountPath": "/run/secrets/db"}]}),
    )
    .await;
    let app = app(&sandbox);
    let base = format!("/api/v1/projects/{ns}/pods/store/files");

    let (status, files) = send(&app, "GET", &base, None).await;
    assert_eq!(status, StatusCode::OK, "{files}");
    assert_eq!(files["reader"]["kind"], "container", "{files}");
    assert!(
        files["mounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["path"] == "/data" && m["kind"] == "claim" && m["object"] == "store")
    );

    let (status, listing) = send(
        &app,
        "GET",
        &format!("{base}/entries?{}", query(&[("path", "/data")])),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listing}");
    let listed = names(&listing);
    assert_eq!(
        listing["items"][0]["type"], "directory",
        "folders first: {listing}"
    );
    for name in ["reports", "bytes.bin", "hello.txt", "shortcut"] {
        assert!(listed.contains(&name.to_owned()), "{name}: {listing}");
    }
    let bin = listing["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "bytes.bin")
        .unwrap();
    assert_eq!(bin["size"], 256 * 300);

    let (status, body) = download(&app, &base, &[("path", "/data/bytes.bin")]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, bytes(), "the download is the file byte for byte");

    let (status, preview) = send(
        &app,
        "GET",
        &format!("{base}/preview?{}", query(&[("path", "/data/hello.txt")])),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["text"], "hello\n");

    odd_names_stay_whole(&app, &base).await;
    secret_files_are_never_read(&app, &base).await;
}

/// Odd names: each stays one name, and none runs anything.
async fn odd_names_stay_whole(app: &Router, base: &str) {
    let odd = [
        "-rf",
        "line\nbreak",
        "日本語 file.txt",
        "$(touch pwned)",
        "a;b|c&d",
        "--help",
    ];
    for name in odd {
        let path = format!("/data/reports/{name}");
        upload(app, base, &[("path", &path)], name.as_bytes()).await;
    }
    let (_, listing) = send(
        app,
        "GET",
        &format!("{base}/entries?{}", query(&[("path", "/data/reports")])),
        None,
    )
    .await;
    let mut listed = names(&listing);
    listed.sort();
    let mut expected: Vec<String> = odd.iter().map(|n| (*n).to_owned()).collect();
    expected.sort();
    assert_eq!(listed, expected, "{listing}");
    for name in odd {
        let path = format!("/data/reports/{name}");
        let (status, body) = download(app, base, &[("path", &path)]).await;
        assert_eq!(status, StatusCode::OK, "{name:?}");
        assert_eq!(body, name.as_bytes(), "{name:?}");
    }
    let (status, _) = send(
        app,
        "POST",
        &format!("{base}/moves"),
        Some(&json!({"path": "/data/reports/-rf", "name": "renamed -rf"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = send(
        app,
        "DELETE",
        &format!(
            "{base}/entries?{}",
            query(&[("path", "/data/reports/line\nbreak")])
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, listing) = send(
        app,
        "GET",
        &format!("{base}/entries?{}", query(&[("path", "/data/reports")])),
        None,
    )
    .await;
    let listed = names(&listing);
    assert!(listed.contains(&"renamed -rf".to_owned()), "{listing}");
    assert!(!listed.contains(&"line\nbreak".to_owned()), "{listing}");
    for root in ["/data", "/"] {
        let (_, listing) = send(
            app,
            "GET",
            &format!("{base}/entries?{}", query(&[("path", root)])),
            None,
        )
        .await;
        assert!(!names(&listing).contains(&"pwned".to_owned()), "{listing}");
    }
}

/// A Secret's files: named, never read.
async fn secret_files_are_never_read(app: &Router, base: &str) {
    let (status, listing) = send(
        app,
        "GET",
        &format!("{base}/entries?{}", query(&[("path", "/run/secrets/db")])),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listing}");
    assert_eq!(names(&listing), ["password"]);
    assert_eq!(listing["items"][0].get("size"), None);
    for path in ["/run/secrets/db/password", "/data/shortcut"] {
        let (status, body) = download(app, base, &[("path", path)]).await;
        assert_eq!(status, StatusCode::CONFLICT, "{path}");
        assert!(!String::from_utf8_lossy(&body).contains(SECRET_VALUE));
        let (status, preview) = send(
            app,
            "GET",
            &format!("{base}/preview?{}", query(&[("path", path)])),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{path}");
        assert!(!preview.to_string().contains(SECRET_VALUE));
    }
}

#[tokio::test]
#[ignore = "needs a disposable cluster"]
async fn a_refused_exec_names_the_right() {
    let sandbox = Sandbox::new("zeabur-files-refused", rules(false)).await;
    let ns = sandbox.namespace().to_owned();
    pod(
        &sandbox,
        "store",
        "busybox:1.37",
        Some(json!(["sleep", "3600"])),
        json!({}),
    )
    .await;
    let base = format!("/api/v1/projects/{ns}/pods/store/files");
    let uri = format!("{base}/entries?{}", query(&[("path", "/data")]));
    let (status, problem) = send(&app(&sandbox), "GET", &uri, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
    assert_eq!(problem["kind"], "forbidden");
    assert_eq!(problem["denied"]["resource"], "pods/exec");
}

/// The pause image has no `tar`, `find` or shell: the claim is read through a debug container
/// that mounts it.
#[tokio::test]
#[ignore = "needs a disposable cluster"]
async fn an_image_without_tar_has_its_volume_read_through_a_debug_container() {
    let sandbox = Sandbox::new("zeabur-files-distroless", rules(true)).await;
    let ns = sandbox.namespace().to_owned();
    pod(
        &sandbox,
        "paused",
        "registry.k8s.io/pause:3.10",
        None,
        json!({}),
    )
    .await;
    let app = app(&sandbox);
    let base = format!("/api/v1/projects/{ns}/pods/paused/files");

    let (status, files) = send(&app, "GET", &base, None).await;
    assert_eq!(status, StatusCode::OK, "{files}");
    assert_eq!(
        files["reader"],
        json!({"kind": "none", "canAddDebugContainer": true})
    );
    let (status, problem) = send(
        &app,
        "GET",
        &format!("{base}/entries?{}", query(&[("path", "/data")])),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{problem}");
    assert_eq!(problem["summary"]["key"], "files.noTools");

    let (status, planned) = send(
        &app,
        "POST",
        &format!("/api/v1/projects/{ns}/plan"),
        Some(&json!({"changes": [{"op": "debugContainer", "pod": "paused"}]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{planned}");
    let (status, applied) = send(
        &app,
        "POST",
        &format!("/api/v1/projects/{ns}/apply"),
        Some(&json!({"token": planned["token"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{applied}");

    let ready = async {
        loop {
            let (_, files) = send(&app, "GET", &base, None).await;
            if files["reader"]["kind"] == "debugContainer" {
                return files;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    };
    let files = tokio::time::timeout(WAIT, ready)
        .await
        .expect("the debug container runs");
    let via = files["reader"]["via"].as_str().unwrap().to_owned();

    let content = b"written through the debug container\n";
    upload(
        &app,
        &base,
        &[("path", "/data/note.txt"), ("via", &via)],
        content,
    )
    .await;
    let (status, listing) = send(
        &app,
        "GET",
        &format!(
            "{base}/entries?{}",
            query(&[("path", "/data"), ("via", &via)])
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listing}");
    assert!(
        names(&listing).contains(&"note.txt".to_owned()),
        "{listing}"
    );
    let (status, body) = download(&app, &base, &[("path", "/data/note.txt"), ("via", &via)]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, content);

    let (status, problem) = send(
        &app,
        "GET",
        &format!(
            "{base}/entries?{}",
            query(&[("path", "/etc"), ("via", &via)])
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{problem}");
    assert_eq!(problem["summary"]["key"], "files.imageUnreachable");
}
