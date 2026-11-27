//! A bucket's files through Zeabur: browsing by folder, preview, download, uploads whole and in
//! parts, folders, renames and deletes, all against a gateway that takes only signed requests;
//! the store's key in no answer; and the refusals of someone who may not read the store's Secret
//! or change the bucket.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fmt::Write as _;
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::body::{Body, Bytes};
    use axum::extract::{Path, RawQuery, State as Shared};
    use axum::http::{HeaderMap, Method, Request, StatusCode};
    use axum::response::{IntoResponse as _, Response};
    use http_body_util::BodyExt as _;
    use serde_json::{Value, json};
    use tower::ServiceExt as _;
    use zeabur_api::access::SingleCluster;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{DynamicObject, ServedResource};

    const ACCESS: &str = "ZK4Q8N2M7RXW";
    const SECRET: &str = "s3-secret-that-stays-on-the-server";
    const BASE: &str = "/api/v1/projects/lab/object-storage/files/buckets/uploads";

    /// What the fake gateway holds: objects by key with their type, and open uploads' parts.
    #[derive(Default)]
    struct Gateway {
        objects: BTreeMap<String, (Vec<u8>, String)>,
        uploads: BTreeMap<String, BTreeMap<u16, Vec<u8>>>,
        unsigned: usize,
        /// Copies, then breaks off the answer to `CopyObject` halfway.
        broken_copy: bool,
        /// Answers `HeadObject` with a 503.
        broken_head: bool,
    }

    type Held = Arc<Mutex<Gateway>>;

    fn query_of(query: Option<&str>) -> BTreeMap<String, String> {
        form_urlencoded::parse(query.unwrap_or_default().as_bytes())
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    }

    fn xml(text: String) -> Response {
        ([("content-type", "application/xml")], text).into_response()
    }

    fn escape(text: &str) -> String {
        text.replace('&', "&amp;").replace('<', "&lt;")
    }

    /// The text of each `<Key>` in `body`.
    fn keys_in(body: &str) -> Vec<String> {
        body.split("<Key>")
            .skip(1)
            .filter_map(|part| part.split_once("</Key>"))
            .map(|(key, _)| key.replace("&lt;", "<").replace("&amp;", "&"))
            .collect()
    }

    /// Plays a path-style S3 gateway for bucket `uploads`, refusing a request that does not carry
    /// a signature by `ACCESS`.
    #[expect(
        clippy::significant_drop_tightening,
        reason = "the gateway holds its objects for the whole request"
    )]
    async fn gateway(
        Shared(held): Shared<Held>,
        method: Method,
        Path(path): Path<String>,
        RawQuery(raw): RawQuery,
        headers: HeaderMap,
        body: Bytes,
    ) -> Response {
        let signed = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| {
                v.starts_with(&format!("AWS4-HMAC-SHA256 Credential={ACCESS}/"))
                    && v.contains("Signature=")
                    && !v.contains(SECRET)
            });
        if !signed || !headers.contains_key("x-amz-content-sha256") {
            held.lock().unwrap().unsigned += 1;
            return (
                StatusCode::FORBIDDEN,
                "<Error><Code>AccessDenied</Code></Error>",
            )
                .into_response();
        }
        let query = query_of(raw.as_deref());
        let (bucket, key) = path.split_once('/').unwrap_or((path.as_str(), ""));
        assert_eq!(bucket, "uploads");
        let mut held = held.lock().unwrap();
        match (method, key) {
            (Method::GET, "") => xml(list(&held, &query)),
            (Method::POST, "") if query.contains_key("delete") => {
                assert!(headers.contains_key("content-md5"));
                for key in keys_in(&String::from_utf8_lossy(&body)) {
                    held.objects.remove(&key);
                }
                xml("<DeleteResult></DeleteResult>".to_owned())
            }
            (Method::POST | Method::PUT, key)
                if query.contains_key("uploads") || query.contains_key("uploadId") =>
            {
                multipart(&mut held, key, &query, &body)
            }
            (Method::PUT, key) => {
                if let Some(source) = headers.get("x-amz-copy-source") {
                    let source = source.to_str().unwrap().trim_start_matches("/uploads/");
                    let source = form_urlencoded::parse(format!("k={source}").as_bytes())
                        .next()
                        .unwrap()
                        .1
                        .into_owned();
                    let object = held.objects[&source].clone();
                    held.objects.insert(key.to_owned(), object);
                    if held.broken_copy {
                        use futures_util::StreamExt as _;
                        // The status and the first bytes go out before the connection drops.
                        let half = futures_util::stream::once(async {
                            Ok::<_, std::io::Error>(Bytes::from_static(b"<CopyObjectRes"))
                        })
                        .chain(futures_util::stream::once(async {
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                            Err(std::io::Error::other("the gateway went away"))
                        }));
                        return Body::from_stream(half).into_response();
                    }
                    return xml("<CopyObjectResult/>".to_owned());
                }
                let kind = headers
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("binary/octet-stream")
                    .to_owned();
                held.objects.insert(key.to_owned(), (body.to_vec(), kind));
                ([("etag", "\"x\"")], "").into_response()
            }
            (Method::HEAD, _) if held.broken_head => {
                StatusCode::SERVICE_UNAVAILABLE.into_response()
            }
            (Method::GET | Method::HEAD, key) => match held.objects.get(key) {
                Some((bytes, kind)) => {
                    ([("content-type", kind.clone())], bytes.clone()).into_response()
                }
                None => (
                    StatusCode::NOT_FOUND,
                    "<Error><Code>NoSuchKey</Code></Error>",
                )
                    .into_response(),
            },
            (Method::DELETE, key) => {
                held.objects.remove(key);
                StatusCode::NO_CONTENT.into_response()
            }
            _ => StatusCode::NOT_IMPLEMENTED.into_response(),
        }
    }

    /// A `ListObjectsV2` answer, grouped by `/` when the query asks for it.
    fn list(held: &Gateway, query: &BTreeMap<String, String>) -> String {
        let prefix = query.get("prefix").cloned().unwrap_or_default();
        let delimiter = query.contains_key("delimiter");
        let mut contents = String::new();
        let mut folders = std::collections::BTreeSet::new();
        for (key, (bytes, _)) in held.objects.range(prefix.clone()..) {
            let Some(rest) = key.strip_prefix(&prefix) else {
                break;
            };
            match rest.split_once('/').filter(|_| delimiter) {
                Some((folder, _)) => {
                    folders.insert(format!("{prefix}{folder}/"));
                }
                None => {
                    let _ignored = write!(
                        contents,
                        "<Contents><Key>{}</Key><Size>{}</Size>\
                         <LastModified>2026-11-28T01:02:03.000Z</LastModified></Contents>",
                        escape(key),
                        bytes.len()
                    );
                }
            }
        }
        let folders = folders.iter().fold(String::new(), |mut out, f| {
            let _ignored = write!(
                out,
                "<CommonPrefixes><Prefix>{}</Prefix></CommonPrefixes>",
                escape(f)
            );
            out
        });
        format!(
            "<ListBucketResult><Prefix>{}</Prefix>{contents}{folders}\
             <IsTruncated>false</IsTruncated></ListBucketResult>",
            escape(&prefix)
        )
    }

    /// Starts an upload, takes a part, or completes an upload from its parts.
    fn multipart(
        held: &mut Gateway,
        key: &str,
        query: &BTreeMap<String, String>,
        body: &[u8],
    ) -> Response {
        if query.contains_key("uploads") {
            let id = format!("upload-{}", held.uploads.len() + 1);
            held.uploads.insert(id.clone(), BTreeMap::new());
            return xml(format!(
                "<InitiateMultipartUploadResult><Key>{key}</Key><UploadId>{id}</UploadId>\
                 </InitiateMultipartUploadResult>"
            ));
        }
        let id = &query["uploadId"];
        if let Some(number) = query.get("partNumber") {
            let number: u16 = number.parse().unwrap();
            held.uploads
                .get_mut(id)
                .unwrap()
                .insert(number, body.to_vec());
            return ([("etag", format!("\"etag-{number}\""))], "").into_response();
        }
        let parts = held.uploads.remove(id).unwrap();
        let text = String::from_utf8_lossy(body);
        assert_eq!(text.matches("<Part>").count(), parts.len(), "{text}");
        let bytes = parts.into_values().flatten().collect();
        held.objects.insert(
            key.to_owned(),
            (bytes, "application/octet-stream".to_owned()),
        );
        xml("<CompleteMultipartUploadResult/>".to_owned())
    }

    async fn serve(held: Held) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let app = Router::new()
            .route("/{*path}", axum::routing::any(gateway))
            .with_state(held);
        tokio::spawn(async move { axum::serve(listener, app).await });
        format!("http://127.0.0.1:{port}")
    }

    fn object(value: Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    fn store() -> MemoryCluster {
        let served = [
            ("seaweeds", "Seaweed"),
            ("buckets", "Bucket"),
            ("s3credentials", "S3Credentials"),
        ]
        .into_iter()
        .map(|(resource, kind)| {
            ServedResource::readable("seaweed.seaweedfs.com", "v1", resource, kind, true)
        })
        .collect();
        let cluster = MemoryCluster::new(State {
            served,
            dynamic: vec![
                object(json!({
                    "apiVersion": "seaweed.seaweedfs.com/v1", "kind": "Seaweed",
                    "metadata": {"name": "files", "namespace": "lab"},
                    "spec": {"s3": {"replicas": 1}}
                })),
                object(json!({
                    "apiVersion": "seaweed.seaweedfs.com/v1", "kind": "S3Credentials",
                    "metadata": {"name": "files-app", "namespace": "lab"},
                    "spec": {"seaweedRef": {"name": "files"}, "identityRef": {"name": "files-app"}},
                    "status": {"phase": "Ready", "accessKey": ACCESS}
                })),
                object(json!({
                    "apiVersion": "seaweed.seaweedfs.com/v1", "kind": "Bucket",
                    "metadata": {"name": "uploads", "namespace": "lab"},
                    "spec": {"clusterRef": {"name": "files"}},
                    "status": {"phase": "Ready", "bucketName": "uploads"}
                })),
            ],
            ..State::default()
        });
        cluster.update(|s| {
            s.secret_values.insert(
                "lab/files-app".to_owned(),
                [
                    ("accessKey".to_owned(), ACCESS.as_bytes().to_vec()),
                    ("secretKey".to_owned(), SECRET.as_bytes().to_vec()),
                ]
                .into(),
            );
        });
        cluster
    }

    fn app(cluster: &MemoryCluster, endpoint: String) -> Router {
        zeabur_api::router_with(
            Arc::new(SingleCluster(Arc::new(cluster.clone()))),
            zeabur_api::Options {
                s3_endpoint: Some(endpoint),
                ..zeabur_api::Options::default()
            },
        )
    }

    /// The status, headers and body of one request; the body never holds the store's keys.
    async fn call(
        app: &Router,
        method: &str,
        uri: &str,
        body: Body,
        headers: &[(&str, &str)],
    ) -> (StatusCode, HeaderMap, Bytes) {
        let mut request = Request::builder().method(method).uri(uri);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = app
            .clone()
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains(SECRET),
            "{uri} answered the secret key: {text}"
        );
        for value in headers.values() {
            assert!(
                !value.to_str().unwrap_or_default().contains(SECRET),
                "{uri}"
            );
        }
        (status, headers, bytes)
    }

    async fn json_of(
        app: &Router,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let (status, _, bytes) = call(
            app,
            method,
            uri,
            body.map_or_else(Body::empty, |b| Body::from(b.to_string())),
            &[("content-type", "application/json")],
        )
        .await;
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn put(app: &Router, uri: &str, bytes: &'static [u8], kind: &str) -> (StatusCode, Bytes) {
        let length = bytes.len().to_string();
        let (status, _, body) = call(
            app,
            "PUT",
            uri,
            Body::from(bytes),
            &[("content-length", &length), ("content-type", kind)],
        )
        .await;
        (status, body)
    }

    /// The gateway and the API over it, with `docs/readme.txt`, `docs/img/a.png`,
    /// `top & more.bin` and the folder `empty/` uploaded through Zeabur.
    async fn filled() -> (Held, Router) {
        let held = Held::default();
        let endpoint = serve(Arc::clone(&held)).await;
        let app = app(&store(), endpoint);
        let (status, body) = put(
            &app,
            &format!("{BASE}/object?key=docs/readme.txt"),
            b"hello",
            "text/plain",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body:?}");
        let (status, _) = put(
            &app,
            &format!("{BASE}/object?key=docs/img/a.png"),
            b"\x89PNG",
            "image/png",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, _) = put(
            &app,
            &format!("{BASE}/object?key=top%20%26%20more.bin"),
            b"\0\x01",
            "application/x-thing",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, made) = json_of(
            &app,
            "POST",
            &format!("{BASE}/folders"),
            Some(json!({"prefix": "empty/"})),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{made}");
        (held, app)
    }

    #[tokio::test]
    async fn a_person_browses_and_previews_a_bucket_without_the_key() {
        let (held, app) = filled().await;
        let (status, top) = json_of(&app, "GET", &format!("{BASE}/objects"), None).await;
        assert_eq!(status, StatusCode::OK, "{top}");
        assert_eq!(top["folders"], json!(["docs/", "empty/"]));
        assert_eq!(top["objects"][0]["key"], "top & more.bin");
        assert_eq!(top["objects"][0]["size"], 2);
        assert_eq!(top["limits"]["part"], 64 * 1024 * 1024);
        let (_, docs) = json_of(&app, "GET", &format!("{BASE}/objects?prefix=docs/"), None).await;
        assert_eq!(docs["folders"], json!(["docs/img/"]));
        assert_eq!(docs["objects"][0]["key"], "docs/readme.txt");
        let (_, empty) = json_of(&app, "GET", &format!("{BASE}/objects?prefix=empty/"), None).await;
        assert_eq!(
            empty["objects"],
            json!([]),
            "the folder's own object is not listed"
        );

        let (_, text) = json_of(
            &app,
            "GET",
            &format!("{BASE}/object/preview?key=docs/readme.txt"),
            None,
        )
        .await;
        assert_eq!(
            (text["text"].as_str(), text["size"].as_u64()),
            (Some("hello"), Some(5))
        );
        let (_, image) = json_of(
            &app,
            "GET",
            &format!("{BASE}/object/preview?key=docs/img/a.png"),
            None,
        )
        .await;
        assert_eq!(
            (image["image"].as_bool(), image.get("text")),
            (Some(true), None)
        );
        let (_, binary) = json_of(
            &app,
            "GET",
            &format!("{BASE}/object/preview?key=top%20%26%20more.bin"),
            None,
        )
        .await;
        assert_eq!(
            (binary["image"].as_bool(), binary.get("text")),
            (Some(false), None)
        );

        let (status, headers, bytes) = call(
            &app,
            "GET",
            &format!("{BASE}/object?key=docs/readme.txt"),
            Body::empty(),
            &[],
        )
        .await;
        assert_eq!((status, &bytes[..]), (StatusCode::OK, &b"hello"[..]));
        assert_eq!(headers["content-type"], "application/octet-stream");
        assert!(
            headers["content-disposition"]
                .to_str()
                .unwrap()
                .contains("readme.txt")
        );
        assert_eq!(headers["x-content-type-options"], "nosniff");
        let (_, headers, _) = call(
            &app,
            "GET",
            &format!("{BASE}/object?key=docs/img/a.png&inline=true"),
            Body::empty(),
            &[],
        )
        .await;
        assert_eq!(
            (
                headers["content-type"].to_str().unwrap(),
                headers["content-disposition"].to_str().unwrap()
            ),
            ("image/png", "inline")
        );
        let (_, headers, _) = call(
            &app,
            "GET",
            &format!("{BASE}/object?key=docs/readme.txt&inline=true"),
            Body::empty(),
            &[],
        )
        .await;
        assert_eq!(
            headers["content-type"], "application/octet-stream",
            "only images are shown inline"
        );

        assert_eq!(held.lock().unwrap().unsigned, 0);
    }

    #[tokio::test]
    async fn a_person_renames_one_object_and_deletes_a_folder() {
        let (held, app) = filled().await;
        let (status, moved) = json_of(
            &app,
            "POST",
            &format!("{BASE}/moves"),
            Some(json!({"from": "docs/readme.txt", "to": "docs/README.md"})),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{moved}");
        let (status, taken) = json_of(
            &app,
            "POST",
            &format!("{BASE}/moves"),
            Some(json!({"from": "docs/README.md", "to": "docs/img/a.png"})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{taken}");
        assert!(held.lock().unwrap().objects.contains_key("docs/README.md"));
        assert!(!held.lock().unwrap().objects.contains_key("docs/readme.txt"));

        let (_, counted) = json_of(
            &app,
            "GET",
            &format!("{BASE}/objects/count?prefix=docs/"),
            None,
        )
        .await;
        assert_eq!(counted, json!({"objects": 2, "bytes": 9, "more": false}));
        let (status, refused) =
            json_of(&app, "DELETE", &format!("{BASE}/objects?prefix="), None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
        let (status, deleted) = json_of(
            &app,
            "DELETE",
            &format!("{BASE}/objects?prefix=docs/"),
            None,
        )
        .await;
        assert_eq!((status, deleted), (StatusCode::OK, json!({"objects": 2})));
        let (_, deleted) = json_of(
            &app,
            "DELETE",
            &format!("{BASE}/objects?key=top%20%26%20more.bin"),
            None,
        )
        .await;
        assert_eq!(deleted, json!({"objects": 1}));
        let keys: Vec<String> = held.lock().unwrap().objects.keys().cloned().collect();
        assert_eq!(keys, ["empty/"]);
        assert_eq!(held.lock().unwrap().unsigned, 0);
    }

    #[tokio::test]
    async fn a_rename_keeps_the_source_when_the_gateway_is_unsure() {
        let (held, app) = filled().await;
        held.lock().unwrap().broken_head = true;
        let (status, moved) = json_of(
            &app,
            "POST",
            &format!("{BASE}/moves"),
            Some(json!({"from": "docs/readme.txt", "to": "docs/img/a.png"})),
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{moved}");
        assert_eq!(held.lock().unwrap().objects["docs/img/a.png"].0, b"\x89PNG");
        held.lock().unwrap().broken_head = false;

        held.lock().unwrap().broken_copy = true;
        let (status, moved) = json_of(
            &app,
            "POST",
            &format!("{BASE}/moves"),
            Some(json!({"from": "docs/readme.txt", "to": "docs/README.md"})),
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{moved}");
        assert!(
            moved["detail"].as_str().unwrap().contains("broke off"),
            "{moved}"
        );
        assert!(held.lock().unwrap().objects.contains_key("docs/readme.txt"));
    }

    #[tokio::test]
    async fn a_full_page_of_long_keys_is_listed() {
        let (held, app) = filled().await;
        {
            let mut held = held.lock().unwrap();
            for i in 0..1000 {
                // 1024 bytes each, and five times that once `&` is escaped.
                let key = format!("long/{i:04}{}", "&".repeat(1015));
                held.objects
                    .insert(key, (Vec::new(), "text/plain".to_owned()));
            }
        }
        let (status, page) =
            json_of(&app, "GET", &format!("{BASE}/objects?prefix=long/"), None).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["objects"].as_array().unwrap().len(), 1000);
    }

    #[tokio::test]
    async fn a_large_file_goes_in_parts() {
        let held = Held::default();
        let app = app(&store(), serve(Arc::clone(&held)).await);
        let (status, started) =
            json_of(&app, "POST", &format!("{BASE}/uploads?key=big.bin"), None).await;
        assert_eq!(status, StatusCode::OK, "{started}");
        let upload = started["upload"].as_str().unwrap().to_owned();
        let mut parts = Vec::new();
        for (number, bytes) in [(1, &b"first-"[..]), (2, &b"second"[..])] {
            let length = bytes.len().to_string();
            let (status, _, answer) = call(
                &app,
                "PUT",
                &format!("{BASE}/uploads/{upload}/parts/{number}?key=big.bin"),
                Body::from(bytes),
                &[("content-length", &length)],
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            parts.push(serde_json::from_slice::<Value>(&answer).unwrap());
        }
        assert_eq!(parts[1], json!({"number": 2, "etag": "\"etag-2\""}));
        let (status, done) = json_of(
            &app,
            "POST",
            &format!("{BASE}/uploads/{upload}/complete?key=big.bin"),
            Some(json!({"parts": parts})),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{done}");
        assert_eq!(held.lock().unwrap().objects["big.bin"].0, b"first-second");

        let too_big = (64 * 1024 * 1024 + 1).to_string();
        let (status, ..) = call(
            &app,
            "PUT",
            &format!("{BASE}/uploads/{upload}/parts/3?key=big.bin"),
            Body::empty(),
            &[("content-length", &too_big)],
        )
        .await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "a part over the limit is refused before it is read"
        );
    }

    #[tokio::test]
    async fn someone_who_may_not_read_the_secret_or_change_the_bucket_is_told_why() {
        let held = Held::default();
        let endpoint = serve(Arc::clone(&held)).await;
        let cluster = store();
        cluster.update(|s| {
            s.refused_rights.insert("update buckets".to_owned());
        });
        let app = app(&cluster, endpoint.clone());
        let (status, listed) = json_of(&app, "GET", &format!("{BASE}/objects"), None).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "reading needs no change right: {listed}"
        );
        let (status, problem) = put(
            &app,
            &format!("{BASE}/object?key=a.txt"),
            b"a",
            "text/plain",
        )
        .await;
        let problem: Value = serde_json::from_slice(&problem).unwrap();
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(problem["denied"]["verb"], "update", "{problem}");
        assert_eq!(problem["denied"]["resource"], "buckets", "{problem}");
        let (status, _) = json_of(
            &app,
            "DELETE",
            &format!("{BASE}/objects?prefix=docs/"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let secretless = store();
        secretless.update(|s| {
            s.forbidden_in_namespaces.insert("secrets".to_owned());
        });
        let app = self::app(&secretless, endpoint);
        let (status, problem) = json_of(&app, "GET", &format!("{BASE}/objects"), None).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(problem["denied"]["resource"], "secrets", "{problem}");
        assert!(held.lock().unwrap().objects.is_empty());

        let (status, missing) = json_of(
            &self::app(&store(), "http://127.0.0.1:9".to_owned()),
            "GET",
            "/api/v1/projects/lab/object-storage/files/buckets/other/objects",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{missing}");
    }
}
