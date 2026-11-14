//! Newer templates from Zeabur Catalog's API, fetched by the server (docs/reference/telemetry.mdx,
//! "Template updates").
//!
//! A template list request starts a fetch in the background once 6 hours have passed since the
//! last one, or 10 minutes after a failure. It asks for `catalog.json` with the last answer's
//! `ETag`, then for `templates/<id>.json` of each template whose `sourceDigest` is not the
//! revision offered now ([`Catalog::stale`]). The files are trusted as the HTTPS origin, or the
//! mirror configured in its place, serves them. The templates of [`Catalog::updated`] are offered
//! until the server stops; a failure keeps what is offered.

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use axum::http::{self, Request};
use tokio::sync::Mutex;
use zeabur_catalog::{Catalog, Index};
use zeabur_http::Https;

/// How long after a fetch the next one is due.
const EVERY: Duration = Duration::from_hours(6);
/// How long after a failed fetch the next one is due.
const RETRY: Duration = Duration::from_mins(10);
/// The most one file of the API may hold.
const LIMIT: usize = 1024 * 1024;

/// The templates offered, and how newer ones are fetched.
pub struct Templates {
    current: RwLock<Arc<Catalog>>,
    fetch: Option<Fetch>,
}

impl std::fmt::Debug for Templates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Templates")
            .field("fetch", &self.fetch.as_ref().map(|f| &f.base))
            .finish_non_exhaustive()
    }
}

struct Fetch {
    /// Where the API is, ending in `/`.
    base: String,
    http: Https,
    kept: Mutex<Kept>,
}

#[derive(Default)]
struct Kept {
    /// The `ETag` of the `catalog.json` the offered templates come from.
    etag: Option<String>,
    /// No fetch before this.
    next: Option<Instant>,
}

impl Templates {
    /// `catalog`, and no fetching.
    #[must_use]
    pub fn fixed(catalog: Catalog) -> Self {
        Self {
            current: RwLock::new(Arc::new(catalog)),
            fetch: None,
        }
    }

    /// The templates this build carries until newer ones are fetched from `base`, a URL ending
    /// in `/`, through `http`.
    #[must_use]
    pub fn fetching(base: String, http: Https) -> Self {
        Self {
            fetch: Some(Fetch {
                base,
                http,
                kept: Mutex::default(),
            }),
            ..Self::fixed(Catalog::embedded())
        }
    }

    /// The templates offered now.
    pub(crate) fn current(&self) -> Arc<Catalog> {
        let current = self
            .current
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Arc::clone(&current)
    }

    /// Starts [`Self::refresh`] in the background.
    pub(crate) fn refresh_in_background(self: &Arc<Self>) {
        if self.fetch.is_none() {
            return;
        }
        let templates = Arc::clone(self);
        tokio::spawn(async move { templates.refresh(Instant::now()).await });
    }

    /// Fetches newer templates when a fetch is due. One fetch runs at a time; a request that
    /// comes meanwhile does not wait for it.
    pub(crate) async fn refresh(&self, now: Instant) {
        let Some(fetch) = &self.fetch else {
            return;
        };
        let Ok(mut kept) = fetch.kept.try_lock() else {
            return;
        };
        if kept.next.is_some_and(|next| now < next) {
            return;
        }
        match fetch.fetch(&self.current(), kept.etag.as_deref()).await {
            Ok(None) => kept.next = Some(now + EVERY),
            Ok(Some((catalog, etag))) => {
                *self
                    .current
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Arc::new(catalog);
                kept.etag = etag;
                kept.next = Some(now + EVERY);
                tracing::info!(from = %fetch.base, "fetched templates are offered");
            }
            Err(error) => {
                tracing::warn!(from = %fetch.base, %error, "templates were not fetched");
                kept.next = Some(now + RETRY);
            }
        }
    }
}

impl Fetch {
    /// The templates `catalog.json` lists now, with its `ETag`; `None` when it still has `etag`.
    async fn fetch(
        &self,
        current: &Catalog,
        etag: Option<&str>,
    ) -> Result<Option<(Catalog, Option<String>)>, String> {
        let (status, headers, index) = self.get("catalog.json", etag).await?;
        if status == 304 {
            return Ok(None);
        }
        let etag = headers
            .get(http::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let index: Index =
            serde_json::from_slice(&index).map_err(|e| format!("catalog.json: {e}"))?;
        let mut files = BTreeMap::new();
        for id in current.stale(&index) {
            let (_, _, file) = self.get(&format!("templates/{id}.json"), None).await?;
            files.insert(id.to_owned(), file);
        }
        let (catalog, dropped) = current.updated(&index, &files);
        for dropped in &dropped {
            tracing::warn!(template = %dropped.id, reason = %dropped.reason, "a fetched template is not offered");
        }
        Ok(Some((catalog, etag)))
    }

    /// `GET <base><name>`: `200`, or `304` when `etag` is given.
    async fn get(
        &self,
        name: &str,
        etag: Option<&str>,
    ) -> Result<(u16, http::HeaderMap, Vec<u8>), String> {
        let url = format!("{}{name}", self.base);
        let mut request = Request::get(&url).header(http::header::USER_AGENT, "zeabur");
        if let Some(etag) = etag {
            request = request.header(http::header::IF_NONE_MATCH, etag);
        }
        let request = request
            .body(http_body_util::Full::default())
            .map_err(|e| e.to_string())?;
        let answer = self.http.exchange_with_headers(request, LIMIT).await;
        let answer = answer.map_err(|e| format!("{url}: {e}"))?;
        match answer.0 {
            200 => Ok(answer),
            304 if etag.is_some() => Ok(answer),
            status => Err(format!("{url}: answered {status}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `templates/ollama.json` as Zeabur Catalog's API serves it, with a revision this build
    /// does not carry.
    fn ollama() -> String {
        include_str!("../../../zeabur-catalog/tests/catalog-api/v1/templates/ollama.json").replacen(
            r#""sourceDigest": ""#,
            r#""sourceDigest": "h1:newer-"#,
            1,
        )
    }
    const ETAG: &str = "\"v1\"";

    /// The files asked for, with the `If-None-Match` each request named.
    type Asked = Arc<std::sync::Mutex<Vec<(String, Option<String>)>>>;

    /// `catalog.json` with every embedded template at its revision, and Ollama at the API's.
    fn index() -> String {
        let embedded = Catalog::embedded().templates().unwrap();
        let templates: Vec<serde_json::Value> = embedded
            .iter()
            .map(|t| {
                let digest = if t.id == "ollama" {
                    "h1:newer"
                } else {
                    &t.revision
                };
                serde_json::json!({"id": t.id, "sourceDigest": digest})
            })
            .collect();
        serde_json::json!({"catalog": {"version": "9.9.9"}, "templates": templates}).to_string()
    }

    /// Serves an API under `/v1/` as GitHub Pages does: each file with an `ETag`, and `304` to a
    /// request that names it.
    async fn serve() -> (String, Asked) {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1/", listener.local_addr().unwrap());
        let asked = Asked::default();
        let log = Arc::clone(&asked);
        let (index, ollama) = (index(), ollama());
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 4096];
                while !request.ends_with(b"\r\n\r\n") {
                    let read = socket.read(&mut buffer).await.unwrap();
                    request.extend_from_slice(&buffer[..read]);
                }
                let request = String::from_utf8(request).unwrap();
                let path = request.split(' ').nth(1).unwrap();
                let name = path.trim_start_matches("/v1/").to_owned();
                let etag = request
                    .lines()
                    .find_map(|l| l.strip_prefix("if-none-match: "))
                    .map(str::to_owned);
                log.lock().unwrap().push((name.clone(), etag.clone()));
                let (status, body) = match name.as_str() {
                    "catalog.json" => ("200 OK", index.as_str()),
                    "templates/ollama.json" => ("200 OK", ollama.as_str()),
                    _ => ("404 Not Found", ""),
                };
                if etag.as_deref() == Some(ETAG) {
                    let head = "HTTP/1.1 304 Not Modified\r\nconnection: close\r\n\r\n";
                    socket.write_all(head.as_bytes()).await.unwrap();
                    continue;
                }
                let head = format!(
                    "HTTP/1.1 {status}\r\netag: {ETAG}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                socket.write_all(head.as_bytes()).await.unwrap();
                socket.write_all(body.as_bytes()).await.unwrap();
            }
        });
        (url, asked)
    }

    fn fetching(url: &str) -> Templates {
        let tls = zeabur_http::tls(None).unwrap();
        let http = Https::new(zeabur_http::Reach::Any, &tls);
        Templates::fetching(url.to_owned(), http)
    }

    fn take(asked: &Asked) -> Vec<(String, Option<String>)> {
        std::mem::take(&mut *asked.lock().unwrap())
    }

    fn ollama_revision(templates: &Templates) -> String {
        templates.current().template("ollama").unwrap().revision
    }

    #[tokio::test]
    async fn a_changed_template_is_fetched_and_offered_then_asked_for_again_with_its_etag() {
        let (url, asked) = serve().await;
        let templates = fetching(&url);
        let start = Instant::now();

        templates.refresh(start).await;
        assert!(ollama_revision(&templates).starts_with("h1:newer-"));
        assert_eq!(
            take(&asked),
            [
                ("catalog.json".to_owned(), None),
                ("templates/ollama.json".to_owned(), None)
            ],
            "only the changed template is fetched"
        );

        templates
            .refresh(start + EVERY.saturating_sub(Duration::from_secs(1)))
            .await;
        assert_eq!(take(&asked), [], "not due yet");

        let fetched = templates.current();
        templates.refresh(start + EVERY).await;
        assert_eq!(
            take(&asked),
            [("catalog.json".to_owned(), Some(ETAG.to_owned()))],
            "an unchanged list is all that is fetched"
        );
        assert!(Arc::ptr_eq(&fetched, &templates.current()));
    }

    #[tokio::test]
    async fn a_failed_fetch_keeps_the_templates_and_is_tried_again() {
        let (url, asked) = serve().await;
        let templates = fetching(&format!("{url}missing/"));
        let embedded = templates.current();
        let start = Instant::now();

        templates.refresh(start).await;
        assert!(Arc::ptr_eq(&embedded, &templates.current()));
        assert_ne!(take(&asked), []);

        templates
            .refresh(start + RETRY.saturating_sub(Duration::from_secs(1)))
            .await;
        assert_eq!(take(&asked), []);
        templates.refresh(start + RETRY).await;
        assert_eq!(
            take(&asked).first(),
            Some(&("missing/catalog.json".to_owned(), None)),
            "a failed fetch leaves no ETag behind"
        );
    }
}
