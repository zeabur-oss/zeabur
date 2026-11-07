//! The HTTP side: one handler, then the order of [`crate::rules`].

use std::convert::Infallible;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Method, Request, Response, StatusCode, Uri, header};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt as _, Full};
use hyper_util::server::graceful::GracefulShutdown;
use k8s_openapi::jiff::Timestamp;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use crate::access::{Reviewer, Token};
use crate::audit::Audit;
use crate::logs::{Logs, LogsLimits};
use crate::metrics::{Metrics, MetricsLimits};
use crate::reject::Reject;
use crate::rules::{Authorized, Backend, Scoped as _};
use crate::upstream::{self, Answer, Capped, Client};

/// The answer's body.
pub type ProxyBody = BoxBody<Bytes, io::Error>;

/// Where a backend is and how far its requests may go.
#[derive(Debug, Clone)]
pub struct BackendConfig<L> {
    /// Its base URL, `http://host:port`.
    pub url: Uri,
    /// Its module's limits.
    pub limits: L,
    /// How long it may take to start answering, and to finish.
    pub timeout: Duration,
    /// The most bytes an answer may have.
    pub max_bytes: u64,
}

/// The proxy's settings.
#[derive(Debug, Clone)]
pub struct Settings {
    /// VictoriaLogs, when one is configured.
    pub logs: Option<BackendConfig<LogsLimits>>,
    /// VictoriaMetrics, when one is configured.
    pub metrics: Option<BackendConfig<MetricsLimits>>,
}

/// The proxy: settings, the access review and the backends' client.
#[derive(Debug)]
pub struct Proxy<R> {
    reviewer: R,
    settings: Settings,
    client: Client,
}

impl<R: Reviewer> Proxy<R> {
    /// A proxy asking `reviewer`.
    #[must_use]
    pub fn new(reviewer: R, settings: Settings) -> Arc<Self> {
        Arc::new(Self {
            reviewer,
            client: upstream::client(Duration::from_secs(5)),
            settings,
        })
    }

    /// Answers one request.
    pub async fn handle<B>(self: Arc<Self>, request: Request<B>) -> Response<ProxyBody> {
        let (method, uri, headers) = {
            let (parts, _) = request.into_parts();
            (parts.method, parts.uri, parts.headers)
        };
        if uri.path() == "/healthz" && method == Method::GET {
            return text(StatusCode::OK, "ok");
        }
        let mut audit = Audit::start(uri.path());
        let answer = match bearer(&headers) {
            Ok(token) if uri.path().starts_with(Logs::PREFIX) => {
                audit.backend("logs");
                match &self.settings.logs {
                    Some(config) => {
                        self.serve::<Logs>(config, &method, &uri, &token, &mut audit)
                            .await
                    }
                    None => Err(Reject::UnknownBackend),
                }
            }
            Ok(token) if uri.path().starts_with(Metrics::PREFIX) => {
                audit.backend("metrics");
                match &self.settings.metrics {
                    Some(config) => {
                        self.serve::<Metrics>(config, &method, &uri, &token, &mut audit)
                            .await
                    }
                    None => Err(Reject::UnknownBackend),
                }
            }
            Ok(_) => Err(Reject::UnknownBackend),
            Err(reject) => Err(reject),
        };
        match answer {
            Ok(Served::Data {
                content_type,
                body,
                max_bytes,
                timeout,
            }) => {
                let body = Capped::new(body, max_bytes, timeout, audit).boxed();
                let mut response = Response::new(body);
                if let Some(content_type) = content_type {
                    response
                        .headers_mut()
                        .insert(header::CONTENT_TYPE, content_type);
                }
                no_store(response)
            }
            Ok(Served::Refused(detail)) => {
                audit.query_refused();
                problem(StatusCode::BAD_REQUEST, "query_refused", &detail)
            }
            Err(reject) => {
                audit.refused(reject);
                problem(reject.status(), reject.code(), &reject.to_string())
            }
        }
    }

    async fn serve<B: Backend>(
        &self,
        config: &BackendConfig<B::Limits>,
        method: &Method,
        uri: &Uri,
        token: &Token,
        audit: &mut Audit,
    ) -> Result<Served, Reject> {
        let query = B::parse(method, uri.path(), uri.query())?;
        audit.namespace(query.namespace().as_str());
        let grant = self
            .reviewer
            .review(token, query.namespace(), B::RESOURCE)
            .await?;
        audit.user(grant.user());
        let authorized = Authorized::new::<B>(query, grant)?;
        let upstream = B::build(&authorized, &config.limits, Timestamp::now())?;
        match upstream::send(&self.client, &config.url, &upstream, config.timeout).await? {
            Answer::Data { content_type, body } => Ok(Served::Data {
                content_type,
                body,
                max_bytes: config.max_bytes,
                timeout: config.timeout,
            }),
            Answer::Refused(detail) => Ok(Served::Refused(detail)),
        }
    }
}

enum Served {
    Data {
        content_type: Option<HeaderValue>,
        body: hyper::body::Incoming,
        max_bytes: u64,
        timeout: Duration,
    },
    Refused(String),
}

/// The token of `Authorization: Bearer`. Cookies and every other header are ignored.
fn bearer(headers: &HeaderMap) -> Result<Token, Reject> {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let (Some(value), None) = (values.next(), values.next()) else {
        return Err(Reject::NoToken);
    };
    let value = value.to_str().map_err(|_error| Reject::NoToken)?;
    let (scheme, token) = value.split_once(' ').ok_or(Reject::NoToken)?;
    if !scheme.eq_ignore_ascii_case("bearer")
        || token.is_empty()
        || !token.bytes().all(|b| b.is_ascii_graphic())
    {
        return Err(Reject::NoToken);
    }
    Ok(Token::new(token))
}

fn text(status: StatusCode, body: &'static str) -> Response<ProxyBody> {
    let mut response = Response::new(full(Bytes::from_static(body.as_bytes())));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    no_store(response)
}

/// An RFC 9457 problem with a stable `code`.
fn problem(status: StatusCode, code: &str, detail: &str) -> Response<ProxyBody> {
    let body = serde_json::json!({
        "type": "about:blank",
        "title": status.canonical_reason().unwrap_or_default(),
        "status": status.as_u16(),
        "code": code,
        "detail": detail,
    });
    let mut response = Response::new(full(Bytes::from(body.to_string())));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/problem+json"),
    );
    if status == StatusCode::UNAUTHORIZED {
        response
            .headers_mut()
            .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    }
    no_store(response)
}

fn full(bytes: Bytes) -> ProxyBody {
    Full::new(bytes)
        .map_err(|never: Infallible| match never {})
        .boxed()
}

fn no_store(mut response: Response<ProxyBody>) -> Response<ProxyBody> {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Serves `listener`, over TLS when `tls` is given, until `stop` completes; then takes no new
/// connection, drops unfinished TLS handshakes and returns once the open connections have answered.
pub async fn serve<R: Reviewer>(
    listener: TcpListener,
    tls: Option<TlsAcceptor>,
    proxy: Arc<Proxy<R>>,
    stop: impl Future<Output = ()>,
) {
    let graceful = GracefulShutdown::new();
    let (stopping, stopped) = tokio::sync::watch::channel(false);
    let mut stop = std::pin::pin!(stop);
    loop {
        let stream = tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => stream,
                Err(error) => {
                    tracing::warn!(%error, "accept failed");
                    continue;
                }
            },
            () = &mut stop => break,
        };
        let proxy = Arc::clone(&proxy);
        let tls = tls.clone();
        let watcher = graceful.watcher();
        let mut stopped = stopped.clone();
        tokio::spawn(async move {
            let service = hyper::service::service_fn(move |request| {
                let proxy = Arc::clone(&proxy);
                async move { Ok::<_, Infallible>(proxy.handle(request).await) }
            });
            let mut http = hyper::server::conn::http1::Builder::new();
            http.timer(hyper_util::rt::TokioTimer::new());
            let served = if let Some(tls) = tls {
                let handshake = tokio::select! {
                    handshake = tls.accept(stream) => handshake,
                    _ = stopped.wait_for(|stopping| *stopping) => return,
                };
                match handshake {
                    Ok(stream) => {
                        let connection =
                            http.serve_connection(hyper_util::rt::TokioIo::new(stream), service);
                        watcher.watch(connection).await
                    }
                    Err(error) => {
                        tracing::debug!(%error, "TLS handshake failed");
                        Ok(())
                    }
                }
            } else {
                let connection =
                    http.serve_connection(hyper_util::rt::TokioIo::new(stream), service);
                watcher.watch(connection).await
            };
            if let Err(error) = served {
                tracing::debug!(%error, "connection ended");
            }
        });
    }
    stopping.send_replace(true);
    graceful.shutdown().await;
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use http_body_util::Empty;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use tokio::sync::Notify;

    use super::*;
    use crate::access::Grant;
    use crate::namespace::Namespace;

    /// Grants `get` on `logs` and `metrics` in the namespaces listed, and records what it was
    /// asked.
    #[derive(Debug, Default)]
    struct Fake {
        allowed: Vec<&'static str>,
        asked: Mutex<Vec<String>>,
        down: bool,
    }

    impl Reviewer for Fake {
        fn review(
            &self,
            _: &Token,
            namespace: &Namespace,
            resource: &'static str,
        ) -> impl Future<Output = Result<Grant, Reject>> + Send {
            self.asked.lock().unwrap().push(namespace.to_string());
            let answer = if self.down {
                Err(Reject::Unavailable)
            } else if self.allowed.contains(&namespace.as_str())
                && (resource == "logs" || resource == "metrics")
            {
                Ok(Grant::for_tests(namespace.as_str(), resource))
            } else {
                Err(Reject::Forbidden)
            };
            std::future::ready(answer)
        }
    }

    /// A backend that records each request's target and headers and answers `body`.
    async fn backend(
        status: u16,
        body: &'static str,
    ) -> (Uri, Arc<Mutex<Vec<(String, HeaderMap)>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&seen);
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let recorded = Arc::clone(&recorded);
                tokio::spawn(async move {
                    let service = hyper::service::service_fn(move |request: Request<_>| {
                        recorded
                            .lock()
                            .unwrap()
                            .push((request.uri().to_string(), request.headers().clone()));
                        async move {
                            let mut response =
                                Response::new(Full::new(Bytes::from_static(body.as_bytes())));
                            *response.status_mut() = StatusCode::from_u16(status).unwrap();
                            response.headers_mut().insert(
                                header::CONTENT_TYPE,
                                HeaderValue::from_static("application/stream+json"),
                            );
                            response
                                .headers_mut()
                                .insert("set-cookie", HeaderValue::from_static("x=y"));
                            Ok::<_, Infallible>(response)
                        }
                    });
                    hyper::server::conn::http1::Builder::new()
                        .serve_connection(hyper_util::rt::TokioIo::new(stream), service)
                        .await
                        .unwrap_or(());
                });
            }
        });
        (format!("http://{address}").parse().unwrap(), seen)
    }

    fn proxy(url: Uri, fake: Fake, max_bytes: u64) -> Arc<Proxy<Fake>> {
        Proxy::new(
            fake,
            Settings {
                logs: Some(BackendConfig {
                    url: url.clone(),
                    limits: LogsLimits::default(),
                    timeout: Duration::from_secs(5),
                    max_bytes,
                }),
                metrics: Some(BackendConfig {
                    url,
                    limits: MetricsLimits::default(),
                    timeout: Duration::from_secs(5),
                    max_bytes,
                }),
            },
        )
    }

    fn get(target: &str, token: Option<&str>) -> Request<Empty<Bytes>> {
        let mut request = Request::get(target);
        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        request
            .header("AccountID", "7")
            .header("ProjectID", "7")
            .header(header::COOKIE, "zeabur-session=s")
            .body(Empty::new())
            .unwrap()
    }

    async fn body(response: Response<ProxyBody>) -> Result<String, io::Error> {
        let bytes = response.into_body().collect().await?.to_bytes();
        Ok(String::from_utf8(bytes.to_vec()).unwrap())
    }

    fn allowing(namespaces: &[&'static str]) -> Fake {
        Fake {
            allowed: namespaces.to_vec(),
            ..Fake::default()
        }
    }

    #[tokio::test]
    async fn an_allowed_query_reaches_the_backend_rebuilt_without_the_persons_headers() {
        let (url, seen) = backend(200, "{\"_msg\":\"hi\"}\n").await;
        let proxy = proxy(url, allowing(&["a"]), 1 << 20);
        let response = Arc::clone(&proxy)
            .handle(get(
                "/logs/select/logsql/query?namespace=a&query=error",
                Some("good"),
            ))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let headers = response.headers().clone();
        assert_eq!(headers[header::CONTENT_TYPE], "application/stream+json");
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
        assert!(headers.get("set-cookie").is_none());
        assert_eq!(body(response).await.unwrap(), "{\"_msg\":\"hi\"}\n");
        let seen = seen.lock().unwrap().clone();
        let (target, sent) = &seen[0];
        assert!(
            target.starts_with("/select/logsql/query?query=error&start="),
            "{target}"
        );
        assert!(
            target.contains(
                "&extra_stream_filters=%7Bkubernetes.pod_namespace%3D%22a%22%7D&limit=100"
            ),
            "{target}"
        );
        for name in ["authorization", "cookie", "accountid", "projectid"] {
            assert!(sent.get(name).is_none(), "{name}");
        }
    }

    #[tokio::test]
    async fn refusals_never_reach_the_backend() {
        let (url, seen) = backend(200, "secret\n").await;
        let fake = Fake {
            allowed: vec!["a"],
            ..Fake::default()
        };
        let proxy = proxy(url, fake, 1 << 20);
        for (target, token, status, code) in [
            (
                "/logs/select/logsql/query?namespace=a",
                None,
                401,
                "no_token",
            ),
            (
                "/logs/select/logsql/query?namespace=b",
                Some("good"),
                403,
                "forbidden",
            ),
            (
                "/logs/select/logsql/query?namespace=a&namespace=b",
                Some("good"),
                400,
                "repeated_parameter",
            ),
            (
                "/logs/select/logsql/query?namespace=a&extra_filters=x",
                Some("good"),
                400,
                "unknown_parameter",
            ),
            (
                "/logs/select/logsql/tail?namespace=a",
                Some("good"),
                400,
                "path",
            ),
            (
                "/traces/api/v1/query?namespace=a",
                Some("good"),
                404,
                "unknown_backend",
            ),
        ] {
            let response = Arc::clone(&proxy).handle(get(target, token)).await;
            assert_eq!(response.status().as_u16(), status, "{target}");
            let text = body(response).await.unwrap();
            assert!(
                text.contains(&format!("\"code\":\"{code}\"")),
                "{target}: {text}"
            );
            assert!(!text.contains("secret"));
        }
        assert!(seen.lock().unwrap().is_empty());
        assert_eq!(
            *proxy.reviewer.asked.lock().unwrap(),
            ["b"],
            "only well-formed requests are reviewed"
        );
    }

    #[tokio::test]
    async fn a_metrics_query_reaches_the_backend_held_to_the_granted_namespace() {
        let (url, seen) = backend(200, "{\"status\":\"success\"}").await;
        let proxy = proxy(url, allowing(&["a"]), 1 << 20);
        let response = Arc::clone(&proxy)
            .handle(get(
                "/metrics/api/v1/query_range?namespace=a\
                 &query=up%7Bnamespace%3D%22b%22%7D&step=60",
                Some("good"),
            ))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let seen = seen.lock().unwrap().clone();
        let (target, sent) = &seen[0];
        assert!(target.starts_with("/api/v1/query_range?query="), "{target}");
        assert!(target.ends_with("&extra_label=namespace%3Da"), "{target}");
        for name in ["authorization", "cookie", "accountid", "projectid"] {
            assert!(sent.get(name).is_none(), "{name}");
        }
    }

    #[tokio::test]
    async fn a_query_under_way_when_the_proxy_stops_still_gets_its_answer() {
        let backend = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url: Uri = format!("http://{}", backend.local_addr().unwrap())
            .parse()
            .unwrap();
        let (arrived, release) = (Arc::new(Notify::new()), Arc::new(Notify::new()));
        let (arrived_at_backend, released) = (Arc::clone(&arrived), Arc::clone(&release));
        tokio::spawn(async move {
            let (stream, _) = backend.accept().await.unwrap();
            let service = hyper::service::service_fn(move |_request: Request<_>| {
                let (arrived, release) = (Arc::clone(&arrived_at_backend), Arc::clone(&released));
                async move {
                    arrived.notify_one();
                    release.notified().await;
                    Ok::<_, Infallible>(Response::new(Full::new(Bytes::from_static(b"late"))))
                }
            });
            let io = hyper_util::rt::TokioIo::new(stream);
            hyper::server::conn::http1::Builder::new()
                .serve_connection(io, service)
                .await
                .unwrap();
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let proxy = proxy(url, allowing(&["a"]), 1 << 20);
        let mut server = tokio::spawn(serve(listener, None, proxy, async {
            stopped.await.unwrap();
        }));

        let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
        client
            .write_all(
                b"GET /metrics/api/v1/query?namespace=a&query=up HTTP/1.1\r\n\
                  host: proxy\r\nauthorization: Bearer good\r\n\r\n",
            )
            .await
            .unwrap();
        arrived.notified().await;
        stop.send(()).unwrap();
        let waiting = tokio::time::timeout(Duration::from_millis(200), &mut server).await;
        assert!(waiting.is_err(), "the proxy stopped before answering");
        release.notify_one();
        let mut answer = String::new();
        client.read_to_string(&mut answer).await.unwrap();
        assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
        assert!(answer.contains("connection: close"), "{answer}");
        assert!(answer.ends_with("\r\nlate\r\n0\r\n\r\n"), "{answer}");
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn an_unfinished_tls_handshake_does_not_hold_up_stopping() {
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(rustls::server::ResolvesServerCertUsingSni::new()));
        let tls = TlsAcceptor::from(Arc::new(config));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let (url, _) = backend(200, "{}").await;
        let server = tokio::spawn(serve(
            listener,
            Some(tls),
            proxy(url, allowing(&["a"]), 1 << 20),
            async {
                stopped.await.unwrap();
            },
        ));

        let _silent = tokio::net::TcpStream::connect(address).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        stop.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .expect("stopping waited for the handshake")
            .unwrap();
    }

    // Usage history asks for CPU, memory and restarts at once; VictoriaMetrics queues what it
    // cannot run yet, so the proxy refuses none of them.
    #[tokio::test]
    async fn one_persons_queries_at_once_all_reach_the_backend() {
        let (url, seen) = backend(200, "{\"status\":\"success\"}").await;
        let proxy = proxy(url, allowing(&["a"]), 1 << 20);
        let mut answers = Vec::new();
        for _ in 0..3 {
            let target = "/metrics/api/v1/query?namespace=a&query=up";
            answers.push(Arc::clone(&proxy).handle(get(target, Some("good"))).await);
        }
        for answer in &answers {
            assert_eq!(answer.status(), StatusCode::OK);
        }
        assert_eq!(seen.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn metrics_of_another_namespace_never_reach_the_person() {
        let (url, seen) = backend(200, "secret").await;
        let proxy = proxy(url, allowing(&["a"]), 1 << 20);
        for (target, status, code) in [
            (
                "/metrics/api/v1/query?namespace=b&query=up",
                403,
                "forbidden",
            ),
            (
                "/metrics/api/v1/label/namespace/values?namespace=b",
                403,
                "forbidden",
            ),
            (
                "/metrics/api/v1/query?namespace=a&query=up&extra_label=namespace%3Db",
                400,
                "unknown_parameter",
            ),
            (
                "/metrics/api/v1/series?namespace=a&match%5B%5D=up\
                 &extra_filters%5B%5D=%7Bnamespace%3D%22b%22%7D",
                400,
                "unknown_parameter",
            ),
            (
                "/metrics/api/v1/export?namespace=a&match%5B%5D=up",
                400,
                "path",
            ),
            ("/metrics/federate?namespace=a&match%5B%5D=up", 400, "path"),
            ("/metrics/api/v1/status/tsdb?namespace=a", 400, "path"),
        ] {
            let response = Arc::clone(&proxy).handle(get(target, Some("good"))).await;
            assert_eq!(response.status().as_u16(), status, "{target}");
            let text = body(response).await.unwrap();
            assert!(
                text.contains(&format!("\"code\":\"{code}\"")),
                "{target}: {text}"
            );
            assert!(!text.contains("secret"));
        }
        assert!(seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_query_the_metrics_store_cannot_run_is_explained() {
        let (url, _) = backend(422, "cannot parse query").await;
        let response = proxy(url, allowing(&["a"]), 1 << 20)
            .handle(get(
                "/metrics/api/v1/query?namespace=a&query=sum(",
                Some("good"),
            ))
            .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(body(response).await.unwrap().contains("cannot parse query"));
    }

    #[tokio::test]
    async fn without_an_answer_from_kube_apiserver_nothing_is_read() {
        let (url, seen) = backend(200, "secret\n").await;
        let fake = Fake {
            allowed: vec!["a"],
            down: true,
            ..Fake::default()
        };
        let response = proxy(url, fake, 1 << 20)
            .handle(get("/logs/select/logsql/query?namespace=a", Some("good")))
            .await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_refused_query_is_explained_and_a_large_answer_is_cut() {
        let (url, _) = backend(400, "cannot parse query").await;
        let response = proxy(url, allowing(&["a"]), 1 << 20)
            .handle(get(
                "/logs/select/logsql/query?namespace=a&query=(",
                Some("good"),
            ))
            .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(body(response).await.unwrap().contains("cannot parse query"));

        let (url, _) = backend(200, "0123456789").await;
        let response = proxy(url, allowing(&["a"]), 4)
            .handle(get("/logs/select/logsql/query?namespace=a", Some("good")))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(body(response).await.is_err());
    }

    #[tokio::test]
    async fn a_failing_backend_is_a_bad_gateway() {
        let (url, _) = backend(500, "boom").await;
        let response = proxy(url, allowing(&["a"]), 1 << 20)
            .handle(get("/logs/select/logsql/query?namespace=a", Some("good")))
            .await;
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        assert!(!body(response).await.unwrap().contains("boom"));
    }

    #[test]
    fn only_one_well_formed_bearer_header_is_a_token() {
        let header = |values: &[&str]| {
            let mut headers = HeaderMap::new();
            for value in values {
                headers.append(header::AUTHORIZATION, HeaderValue::from_str(value).unwrap());
            }
            bearer(&headers).is_ok()
        };
        assert!(header(&["Bearer abc.def"]));
        assert!(header(&["bearer abc"]));
        assert!(!header(&[]));
        assert!(!header(&["Bearer "]));
        assert!(!header(&["Basic YTpi"]));
        assert!(!header(&["Bearer a b"]));
        assert!(!header(&["Bearer a", "Bearer b"]));
    }
}
