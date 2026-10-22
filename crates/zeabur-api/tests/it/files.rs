//! The file manager against an in-memory cluster's files: listing, the cap, downloads byte for
//! byte, previews, writes and where they are refused, a Secret's files, a refused exec, and an
//! image without `tar`.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use axum::Router;
    use axum::body::Body;
    use axum::http::request::Parts;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt as _;
    use serde_json::{Value, json};
    use tower::ServiceExt as _;
    use zeabur_api::access::{Access, ClusterAccess, Elevation};
    use zeabur_core::problem::Problem;
    use zeabur_kube::k8s_openapi::api::core::v1::Pod;
    use zeabur_kube::memory::{MemoryCluster, MemoryFiles, State};
    use zeabur_kube::{Cluster, NamespaceObjects};

    use crate::common::{app, send};

    const BASE: &str = "/api/v1/projects/shop/pods/web-1/files";

    /// Every byte value, and then some, so a download that changed one would show.
    fn binary() -> Vec<u8> {
        (0..=255_u8).cycle().take(70_000).collect()
    }

    fn pod() -> Pod {
        serde_json::from_value(json!({
            "metadata": {"name": "web-1", "namespace": "shop", "uid": "u-1"},
            "spec": {
                "containers": [{"name": "web", "image": "web:1", "volumeMounts": [
                    {"name": "data", "mountPath": "/data"},
                    {"name": "config", "mountPath": "/config"},
                    {"name": "db", "mountPath": "/run/secrets/db"},
                    {"name": "scratch", "mountPath": "/tmp"}
                ]}],
                "volumes": [
                    {"name": "data", "persistentVolumeClaim": {"claimName": "uploads"}},
                    {"name": "config", "configMap": {"name": "web-config"}},
                    {"name": "db", "secret": {"secretName": "db-credentials"}},
                    {"name": "scratch", "emptyDir": {}}
                ]
            },
            "status": {"phase": "Running", "containerStatuses": [
                {"name": "web", "image": "web:1", "imageID": "", "ready": true,
                 "restartCount": 0, "state": {"running": {}}}
            ]}
        }))
        .unwrap()
    }

    fn files() -> MemoryFiles {
        let mut files = MemoryFiles::default();
        files
            .file("/data/report.csv", "a,b\n1,2\n")
            .file("/data/image.bin", binary())
            .file("/data/a b", "space")
            .file("/data/line\nbreak", "newline")
            .file("/data/-rf", "dash")
            .file("/data/日本語.txt", "unicode")
            .file("/data/$(touch x)", "quoted")
            .directory("/data/uploads")
            .file("/config/app.yaml", "port: 80\n")
            .directory("/run/secrets/db/..2026_10_22")
            .file("/run/secrets/db/..2026_10_22/password", "hunter2")
            .link("/run/secrets/db/..data", "..2026_10_22")
            .link("/run/secrets/db/password", "..data/password")
            .link("/data/shortcut", "/run/secrets/db/password")
            .file("/tmp/big.log", vec![b'x'; 1_000_001])
            .file("/proc/1/environ", "PASSWORD=x")
            .file("/app/server", "image file");
        files.read_only.push("/config".to_owned());
        files.read_only.push("/run/secrets/db".to_owned());
        files
    }

    fn cluster() -> MemoryCluster {
        MemoryCluster::new(State {
            objects: BTreeMap::from([(
                "shop".to_owned(),
                NamespaceObjects {
                    namespace: "shop".to_owned(),
                    pods: vec![pod()],
                    ..NamespaceObjects::default()
                },
            )]),
            files: BTreeMap::from([("shop/web-1".to_owned(), files())]),
            ..State::default()
        })
    }

    fn query(path: &str) -> String {
        form_urlencoded::Serializer::new(String::new())
            .append_pair("path", path)
            .finish()
    }

    async fn list(app: &Router, path: &str) -> (StatusCode, Value) {
        send(app, "GET", &format!("{BASE}/entries?{}", query(path)), None).await
    }

    async fn raw(
        app: &Router,
        request: Request<Body>,
    ) -> (StatusCode, Vec<u8>, axum::http::HeaderMap) {
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, bytes.to_vec(), headers)
    }

    async fn download(app: &Router, path: &str) -> (StatusCode, Vec<u8>, axum::http::HeaderMap) {
        let request = Request::get(format!("{BASE}/content?{}", query(path)))
            .body(Body::empty())
            .unwrap();
        raw(app, request).await
    }

    async fn upload(app: &Router, path: &str, extra: &str, body: Vec<u8>) -> (StatusCode, Value) {
        let request = Request::put(format!("{BASE}/content?{}{extra}", query(path)))
            .header("content-length", body.len())
            .header("zeabur-request", "1")
            .body(Body::from(body))
            .unwrap();
        let (status, bytes, _) = raw(app, request).await;
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    fn names(listing: &Value) -> Vec<&str> {
        listing["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["name"].as_str().unwrap())
            .collect()
    }

    #[tokio::test]
    async fn the_mounts_and_how_the_files_are_reached() {
        let app = app(&cluster());
        let (status, files) = send(&app, "GET", BASE, None).await;
        assert_eq!(status, StatusCode::OK, "{files}");
        assert_eq!(files["container"], "web");
        assert_eq!(files["reader"], json!({"kind": "container"}));
        assert_eq!(
            files["limits"],
            json!({"entries": 1000, "transfer": 100_000_000, "preview": 1_000_000})
        );
        let mounts: Vec<(&str, &str, bool)> = files["mounts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| {
                (
                    m["path"].as_str().unwrap(),
                    m["kind"].as_str().unwrap(),
                    m["readOnly"].as_bool().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            mounts,
            [
                ("/config", "configMap", true),
                ("/data", "claim", false),
                ("/run/secrets/db", "secret", true),
                ("/tmp", "emptyDir", false),
                ("/", "image", false),
            ]
        );
        assert_eq!(files["mounts"][3]["lost"], "replaced");
        assert_eq!(files["mounts"][4]["lost"], "restart");
    }

    /// Odd names arrive whole: spaces, a line break, a leading dash, unicode, shell syntax.
    #[tokio::test]
    async fn a_folder_lists_folders_first_and_keeps_odd_names_whole() {
        let app = app(&cluster());
        let (status, listing) = list(&app, "/data").await;
        assert_eq!(status, StatusCode::OK, "{listing}");
        assert_eq!(
            names(&listing),
            [
                "uploads",
                "$(touch x)",
                "-rf",
                "a b",
                "image.bin",
                "line\nbreak",
                "report.csv",
                "shortcut",
                "日本語.txt"
            ]
        );
        assert_eq!(listing["totalSize"], 9);
        assert_eq!(listing["more"], false);
        let csv = &listing["items"][6];
        assert_eq!(csv["type"], "file");
        assert_eq!(csv["size"], 8);
        assert_eq!(listing["items"][0]["type"], "directory");
        assert_eq!(listing["items"][0].get("size"), None);
        assert_eq!(listing["items"][7]["link"], true);

        let (status, filtered) = send(
            &app,
            "GET",
            &format!("{BASE}/entries?{}&filter=CSV", query("/data")),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(names(&filtered), ["report.csv"]);
    }

    #[tokio::test]
    async fn a_large_folder_lists_its_first_thousand_and_counts_the_rest() {
        let cluster = cluster();
        cluster.update(|s| {
            let files = s.files.get_mut("shop/web-1").unwrap();
            for i in 0..1_234 {
                files.file(&format!("/data/many/IMG_{i:05}.jpg"), "x");
            }
        });
        let app = app(&cluster);
        let (status, listing) = list(&app, "/data/many").await;
        assert_eq!(status, StatusCode::OK, "{listing}");
        assert_eq!(listing["items"].as_array().unwrap().len(), 1000);
        assert_eq!(listing["totalSize"], 1234);
        assert_eq!(listing["items"][999]["name"], "IMG_00999.jpg");
    }

    #[tokio::test]
    async fn a_download_is_the_file_byte_for_byte() {
        let app = app(&cluster());
        let (status, body, headers) = download(&app, "/data/image.bin").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, binary());
        assert_eq!(headers["content-length"], "70000");
        assert_eq!(headers["content-type"], "application/octet-stream");
        assert_eq!(
            headers["content-disposition"],
            "attachment; filename=\"image.bin\"; filename*=UTF-8''image.bin"
        );
        for (path, contents) in [
            ("/data/line\nbreak", "newline"),
            ("/data/-rf", "dash"),
            ("/data/日本語.txt", "unicode"),
            ("/data/$(touch x)", "quoted"),
        ] {
            let (status, body, _) = download(&app, path).await;
            assert_eq!(status, StatusCode::OK, "{path:?}");
            assert_eq!(body, contents.as_bytes(), "{path:?}");
        }
        let (status, ..) = download(&app, "/data/uploads").await;
        assert_eq!(status, StatusCode::CONFLICT);
        let (status, ..) = download(&app, "/data/missing").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    /// A Secret's files are named, never read: no sizes, no download, no preview, also through
    /// a link from elsewhere; the kubelet's own entries are left out.
    #[tokio::test]
    async fn a_secrets_files_are_never_shown_or_downloadable() {
        let app = app(&cluster());
        let (status, listing) = list(&app, "/run/secrets/db").await;
        assert_eq!(status, StatusCode::OK, "{listing}");
        assert_eq!(names(&listing), ["password"]);
        assert_eq!(listing["items"][0].get("size"), None);
        assert_eq!(listing["totalSize"], 1);
        for path in [
            "/run/secrets/db/password",
            "/run/secrets/db/..data/password",
            "/data/shortcut",
        ] {
            let (status, body, _) = download(&app, path).await;
            assert_eq!(status, StatusCode::CONFLICT, "{path}");
            let problem: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(problem["summary"]["key"], "files.secret", "{path}");
            assert_eq!(problem["summary"]["params"]["secret"], "db-credentials");
            assert!(!String::from_utf8_lossy(&body).contains("hunter2"));
            let (status, preview) = send(
                &app,
                "GET",
                &format!("{BASE}/preview?{}", query(path)),
                None,
            )
            .await;
            assert_eq!(status, StatusCode::CONFLICT, "{path}");
            assert!(!preview.to_string().contains("hunter2"));
        }
        let (status, _) = upload(
            &app,
            "/run/secrets/db/password",
            "&replace=true",
            b"x".to_vec(),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn the_kernels_files_are_not_shown() {
        let app = app(&cluster());
        let (status, problem) = list(&app, "/proc/1").await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(problem["summary"]["key"], "files.kernel");
        let (status, ..) = download(&app, "/proc/1/environ").await;
        assert_eq!(status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn a_preview_is_text_up_to_its_limit() {
        let app = app(&cluster());
        let preview = |path: &str| {
            let app = app.clone();
            let uri = format!("{BASE}/preview?{}", query(path));
            async move { send(&app, "GET", &uri, None).await }
        };
        let (status, text) = preview("/data/report.csv").await;
        assert_eq!(status, StatusCode::OK, "{text}");
        assert_eq!(
            text,
            json!({"path": "/data/report.csv", "size": 8, "text": "a,b\n1,2\n", "tooLarge": false})
        );
        let (_, binary) = preview("/data/image.bin").await;
        assert_eq!(binary["text"], Value::Null);
        assert_eq!(binary["tooLarge"], false);
        let (_, large) = preview("/tmp/big.log").await;
        assert_eq!(large["size"], 1_000_001);
        assert_eq!(large["tooLarge"], true);
        assert_eq!(large["text"], Value::Null);
    }

    #[tokio::test]
    async fn an_upload_writes_the_file_and_replaces_one_only_when_asked() {
        let app = app(&cluster());
        let (status, answer) = upload(&app, "/data/uploads/new file.bin", "", binary()).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{answer}");
        let (_, body, _) = download(&app, "/data/uploads/new file.bin").await;
        assert_eq!(body, binary());

        let (status, problem) = upload(&app, "/data/report.csv", "", b"new".to_vec()).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(problem["summary"]["key"], "files.exists");
        let (status, _) = upload(&app, "/data/report.csv", "&replace=true", b"new".to_vec()).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(download(&app, "/data/report.csv").await.1, b"new");

        let (status, problem) =
            upload(&app, "/config/app.yaml", "&replace=true", b"x".to_vec()).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(problem["summary"]["key"], "files.configMapReadOnly");
        assert_eq!(problem["summary"]["params"]["object"], "web-config");

        let request = Request::put(format!("{BASE}/content?{}", query("/data/huge")))
            .header("content-length", "100000001")
            .header("zeabur-request", "1")
            .body(Body::empty())
            .unwrap();
        let (status, body, _) = raw(&app, request).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let problem: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(problem["summary"]["key"], "files.tooLarge");
    }

    #[tokio::test]
    async fn a_folder_is_made_renamed_and_deleted() {
        let app = app(&cluster());
        let post = |uri: &'static str, body: Value| {
            let app = app.clone();
            async move { send(&app, "POST", &format!("{BASE}/{uri}"), Some(&body)).await }
        };
        let (status, answer) = post("folders", json!({"path": "/data/-new folder"})).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{answer}");
        let (status, problem) = post("folders", json!({"path": "/data/-new folder"})).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(problem["summary"]["key"], "files.exists");

        let (status, answer) = post(
            "moves",
            json!({"path": "/data/-new folder", "name": "line\nbreak 2"}),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{answer}");
        let (status, problem) =
            post("moves", json!({"path": "/data/a b", "name": "report.csv"})).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(problem["summary"]["key"], "files.exists");
        let (status, _) = post("moves", json!({"path": "/data/a b", "name": "../x"})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let delete = |path: &str| {
            let app = app.clone();
            let uri = format!("{BASE}/entries?{}", query(path));
            async move { send(&app, "DELETE", &uri, None).await }
        };
        let (status, answer) = delete("/data/line\nbreak 2").await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{answer}");
        let (_, listing) = list(&app, "/data").await;
        assert!(!names(&listing).contains(&"line\nbreak 2"));
        assert!(names(&listing).contains(&"line\nbreak"));

        for (path, key) in [
            ("/data", "files.mountPoint"),
            ("/", "files.mountPoint"),
            ("/run", "files.mountPoint"),
            ("/config/app.yaml", "files.configMapReadOnly"),
        ] {
            let (status, problem) = delete(path).await;
            assert_eq!(status, StatusCode::CONFLICT, "{path}");
            assert_eq!(problem["summary"]["key"], key, "{path}");
        }
        let (status, _) = delete("/data/missing").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    /// One person whose admin rights are offered and not in use.
    struct Elevating(Arc<dyn Cluster>);

    impl ClusterAccess for Elevating {
        fn for_request(&self, _parts: &Parts) -> Result<Access, Box<Problem>> {
            Ok(Access {
                cluster: Arc::clone(&self.0),
                elevation: Elevation::Available,
            })
        }
    }

    #[tokio::test]
    async fn a_refused_exec_names_the_right_and_offers_admin_rights() {
        let cluster = cluster();
        cluster.update(|s| {
            s.exec_refused.insert("shop/web-1".to_owned());
        });
        let (status, problem) = list(&app(&cluster), "/data").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(problem["kind"], "forbidden");
        assert_eq!(
            problem["denied"],
            json!({"verb": "get", "group": "", "resource": "pods/exec",
                   "name": "web-1", "namespace": "shop"})
        );
        let elevating = zeabur_api::router(Arc::new(Elevating(Arc::new(cluster.clone()))));
        for uri in [
            BASE.to_owned(),
            format!("{BASE}/entries?{}", query("/data")),
            format!("{BASE}/content?{}", query("/data/report.csv")),
        ] {
            let (status, problem) = send(&elevating, "GET", &uri, None).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
            assert_eq!(problem["kind"], "elevation-required", "{uri}");
        }
    }

    fn with_debug_container(cluster: &MemoryCluster, mounts: &Value) {
        cluster.update(|state| {
            let pod = &mut state.objects.get_mut("shop").unwrap().pods[0];
            let mut value = serde_json::to_value(&*pod).unwrap();
            value["spec"]["ephemeralContainers"] = json!([{"name": "zeabur-debug-abcdef",
                "image": "busybox", "targetContainerName": "web", "volumeMounts": mounts}]);
            value["status"]["ephemeralContainerStatuses"] = json!([{"name": "zeabur-debug-abcdef",
                "image": "busybox", "imageID": "", "ready": false, "restartCount": 0,
                "state": {"running": {}}}]);
            *pod = serde_json::from_value(value).unwrap();
        });
    }

    /// An image without `tar` (distroless) is read through a debug container that mounts its
    /// volumes; until there is one, the answer says whether the person may add it.
    #[tokio::test]
    async fn an_image_without_tar_is_read_through_a_debug_container() {
        let cluster = cluster();
        cluster.update(|s| {
            s.without_shell.insert("shop/web-1".to_owned());
        });
        let app = app(&cluster);
        let (status, files) = send(&app, "GET", BASE, None).await;
        assert_eq!(status, StatusCode::OK, "{files}");
        assert_eq!(
            files["reader"],
            json!({"kind": "none", "canAddDebugContainer": true})
        );
        let (status, problem) = list(&app, "/data").await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(problem["summary"]["key"], "files.noTools");

        // A debug container from before it mounted volumes (a terminal's) does not do.
        with_debug_container(&cluster, &json!([]));
        let (_, files) = send(&app, "GET", BASE, None).await;
        assert_eq!(files["reader"]["kind"], "none");

        let all = json!([{"name": "data", "mountPath": "/data"},
                         {"name": "config", "mountPath": "/config", "readOnly": true},
                         {"name": "db", "mountPath": "/run/secrets/db"},
                         {"name": "scratch", "mountPath": "/tmp"}]);
        with_debug_container(&cluster, &all);
        let (_, files) = send(&app, "GET", BASE, None).await;
        assert_eq!(
            files["reader"],
            json!({"kind": "debugContainer", "via": "zeabur-debug-abcdef"})
        );
        let via = "&via=zeabur-debug-abcdef";
        let (status, listing) = send(
            &app,
            "GET",
            &format!("{BASE}/entries?{}{via}", query("/data")),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{listing}");
        assert!(names(&listing).contains(&"report.csv"));
        let (status, problem) = send(
            &app,
            "GET",
            &format!("{BASE}/entries?{}{via}", query("/app")),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(problem["summary"]["key"], "files.imageUnreachable");
        let (status, _) = send(
            &app,
            "GET",
            &format!("{BASE}/content?{}{via}", query("/run/secrets/db/password")),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        let (status, _) = send(
            &app,
            "GET",
            &format!("{BASE}/entries?{}&via=zeabur-debug-other", query("/data")),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    /// The plan for a debug container mounts the volumes, and once it runs the files are read
    /// through it.
    #[tokio::test]
    async fn a_debug_container_planned_for_files_mounts_the_volumes() {
        let cluster = cluster();
        cluster.update(|s| {
            s.without_shell.insert("shop/web-1".to_owned());
        });
        let app = app(&cluster);
        let (status, planned) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/plan",
            Some(
                &json!({"changes": [{"op": "debugContainer", "pod": "web-1", "container": "web"}]}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        let after = &planned["objects"][0]["after"]["spec"]["ephemeralContainers"][0];
        let paths: Vec<&str> = after["volumeMounts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["mountPath"].as_str().unwrap())
            .collect();
        assert_eq!(paths, ["/data", "/config", "/run/secrets/db", "/tmp"]);
    }
}
