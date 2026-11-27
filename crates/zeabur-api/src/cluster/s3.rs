//! Requests to an object store's S3 gateway, signed with the store's key as AWS Signature Version
//! 4 [sigv4] and sent from the server, so the key never reaches the page.
//!
//! The payload is not hashed (`UNSIGNED-PAYLOAD` [sigv4-header]), so an upload streams to the
//! gateway as it arrives and is never held whole. Requests use path-style addresses
//! (`<gateway>/<bucket>/<key>`) [s3-paths].
//!
//! The answers are S3's XML. Zeabur reads only the few elements it needs ([`raw_elements`]); S3
//! escapes the five predefined entities and character references in text [xml-entities].
//!
//! [sigv4]: https://docs.aws.amazon.com/AmazonS3/latest/API/sig-v4-authenticating-requests.html
//! [sigv4-header]: https://docs.aws.amazon.com/AmazonS3/latest/API/sigv4-auth-using-authorization-header.html
//! [s3-paths]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/VirtualHosting.html#path-style-access
//! [xml-entities]: https://www.w3.org/TR/xml/#sec-predefined-ent

use std::fmt::Write as _;
use std::time::Duration;

use aws_lc_rs::{digest, hmac};
use axum::body::Body;
use axum::http::{self, HeaderName, HeaderValue, Method, Request, Response};
use hyper_util::client::legacy::Client;
use zeabur_http::{self, Connector, IDLE, Tls, connector, describe, pooled, refuse_named};
use zeabur_kube::k8s_openapi::jiff;

/// How long the gateway may take to start answering.
const FIRST_BYTE: Duration = Duration::from_secs(30);
/// The region in the signature's scope. A `SeaweedFS` gateway takes any; `us-east-1` is S3's
/// default.
const REGION: &str = "us-east-1";
/// The payload hash of a request whose body is not hashed.
const UNSIGNED: &str = "UNSIGNED-PAYLOAD";

/// A store's access key.
#[derive(Clone)]
pub(crate) struct Keys {
    /// The access key ID.
    pub(crate) id: String,
    /// The secret access key.
    pub(crate) secret: String,
}

impl std::fmt::Debug for Keys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Keys").finish_non_exhaustive()
    }
}

/// The client for stores' gateways.
#[derive(Clone)]
pub struct Gateways {
    client: Result<Client<Connector, Body>, String>,
    reach: zeabur_http::Reach,
    endpoint: Option<String>,
}

impl std::fmt::Debug for Gateways {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gateways")
            .field("reach", &self.reach)
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

/// One request to a gateway.
#[derive(Debug)]
pub(crate) struct S3Request<'a> {
    pub(crate) method: Method,
    pub(crate) bucket: &'a str,
    /// The object's key; `None` for the bucket itself.
    pub(crate) key: Option<&'a str>,
    /// The query, unencoded.
    pub(crate) query: Vec<(&'a str, String)>,
    /// Headers sent and signed besides `host`, `x-amz-date` and `x-amz-content-sha256`.
    pub(crate) headers: Vec<(HeaderName, HeaderValue)>,
    pub(crate) body: Body,
}

impl<'a> S3Request<'a> {
    /// A request without a body or headers.
    pub(crate) fn new(method: Method, bucket: &'a str, key: Option<&'a str>) -> Self {
        Self {
            method,
            bucket,
            key,
            query: Vec::new(),
            headers: Vec::new(),
            body: Body::empty(),
        }
    }

    pub(crate) fn query(mut self, name: &'a str, value: impl Into<String>) -> Self {
        self.query.push((name, value.into()));
        self
    }

    pub(crate) fn header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.headers.push((name, value));
        self
    }

    pub(crate) fn body(mut self, body: Body) -> Self {
        self.body = body;
        self
    }
}

impl Gateways {
    /// A client that connects where `reach` allows and trusts what `tls` trusts; `endpoint`, when
    /// given, is where every store's gateway is reached instead of its Service (for tests).
    #[must_use]
    pub fn new(reach: zeabur_http::Reach, tls: &Tls, endpoint: Option<String>) -> Self {
        let client = tls
            .as_ref()
            .map(|tls| pooled(connector(reach, tls), IDLE))
            .map_err(Clone::clone);
        Self {
            client,
            reach,
            endpoint,
        }
    }

    /// The address of the gateway at `host:port`.
    pub(crate) fn endpoint(&self, host: &str, port: u16) -> String {
        self.endpoint
            .clone()
            .unwrap_or_else(|| format!("http://{host}:{port}"))
    }

    /// Sends `request` to the gateway at `endpoint`, signed with `keys` at `now`; answered once
    /// the gateway sends its status line.
    pub(crate) async fn send(
        &self,
        endpoint: &str,
        keys: &Keys,
        now: jiff::Timestamp,
        request: S3Request<'_>,
    ) -> Result<Response<Body>, String> {
        let client = self.client.as_ref().map_err(Clone::clone)?;
        let base: http::Uri = endpoint.parse().map_err(|e| format!("{e}"))?;
        let host = base
            .authority()
            .map(ToString::to_string)
            .ok_or_else(|| format!("{endpoint} names no host"))?;
        let path = path(request.bucket, request.key);
        let query = canonical_query(&request.query);
        let url = if query.is_empty() {
            format!("{endpoint}{path}")
        } else {
            format!("{endpoint}{path}?{query}")
        };
        let uri: http::Uri = url.parse().map_err(|e| format!("{e}"))?;
        refuse_named(self.reach, &uri)?;
        let authorization = authorization(
            keys,
            now,
            &Signed {
                method: request.method.as_str(),
                path: &path,
                query: &query,
                host: &host,
                headers: &request.headers,
                payload: UNSIGNED,
            },
        );
        let mut builder = Request::builder()
            .method(request.method)
            .uri(uri)
            .header(http::header::USER_AGENT, "zeabur")
            .header("x-amz-date", amz_date(now))
            .header("x-amz-content-sha256", UNSIGNED);
        for (name, value) in request.headers {
            builder = builder.header(name, value);
        }
        let mut authorization =
            HeaderValue::from_str(&authorization).map_err(|e| format!("{e}"))?;
        authorization.set_sensitive(true);
        let request = builder
            .header(http::header::AUTHORIZATION, authorization)
            .body(request.body)
            .map_err(|e| e.to_string())?;
        tokio::time::timeout(FIRST_BYTE, client.request(request))
            .await
            .map_err(|_elapsed| "the gateway did not answer in time".to_owned())?
            .map(|response| response.map(Body::new))
            .map_err(|e| describe(&e))
    }
}

/// What the signature covers.
struct Signed<'a> {
    method: &'a str,
    /// The path, encoded.
    path: &'a str,
    /// The query, encoded and sorted.
    query: &'a str,
    host: &'a str,
    headers: &'a [(HeaderName, HeaderValue)],
    payload: &'a str,
}

/// The `Authorization` header of `signed` for `keys` at `now` [sigv4-header].
///
/// [sigv4-header]: https://docs.aws.amazon.com/AmazonS3/latest/API/sigv4-auth-using-authorization-header.html
fn authorization(keys: &Keys, now: jiff::Timestamp, signed: &Signed<'_>) -> String {
    let stamp = amz_date(now);
    let day = now.strftime("%Y%m%d").to_string();
    let day = day.as_str();
    let mut headers: Vec<(String, String)> = vec![
        ("host".to_owned(), signed.host.to_owned()),
        ("x-amz-content-sha256".to_owned(), signed.payload.to_owned()),
        ("x-amz-date".to_owned(), stamp.clone()),
    ];
    headers.extend(signed.headers.iter().map(|(name, value)| {
        (
            name.as_str().to_owned(),
            value.to_str().unwrap_or_default().trim().to_owned(),
        )
    }));
    headers.sort();
    let names = headers
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(";");
    let canonical_headers = headers
        .iter()
        .fold(String::new(), |mut out, (name, value)| {
            let _ignored = writeln!(out, "{name}:{value}");
            out
        });
    let canonical = format!(
        "{}\n{}\n{}\n{canonical_headers}\n{names}\n{}",
        signed.method, signed.path, signed.query, signed.payload
    );
    let scope = format!("{day}/{REGION}/s3/aws4_request");
    let to_sign = format!(
        "AWS4-HMAC-SHA256\n{stamp}\n{scope}\n{}",
        hex::encode(digest::digest(&digest::SHA256, canonical.as_bytes()))
    );
    let key = [day, REGION, "s3", "aws4_request"].iter().fold(
        format!("AWS4{}", keys.secret).into_bytes(),
        |key, part| {
            hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, &key), part.as_bytes())
                .as_ref()
                .to_vec()
        },
    );
    let signature = hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, &key), to_sign.as_bytes());
    format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={names}, Signature={}",
        keys.id,
        hex::encode(signature.as_ref())
    )
}

/// `now` as `YYYYMMDD'T'HHMMSS'Z'`.
fn amz_date(now: jiff::Timestamp) -> String {
    now.strftime("%Y%m%dT%H%M%SZ").to_string()
}

/// The encoded path of `key` in `bucket`.
fn path(bucket: &str, key: Option<&str>) -> String {
    let bucket = encode(bucket, false);
    key.map_or_else(
        || format!("/{bucket}"),
        |key| format!("/{bucket}/{}", encode(key, true)),
    )
}

/// The query, each name and value encoded, sorted by name [sigv4-header].
///
/// [sigv4-header]: https://docs.aws.amazon.com/AmazonS3/latest/API/sigv4-auth-using-authorization-header.html
fn canonical_query(query: &[(&str, String)]) -> String {
    let mut pairs: Vec<(String, String)> = query
        .iter()
        .map(|(name, value)| (encode(name, false), encode(value, false)))
        .collect();
    pairs.sort();
    pairs
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// `text` with every byte but the unreserved characters percent-encoded, and `/` kept when
/// `slash` [sigv4-header].
///
/// [sigv4-header]: https://docs.aws.amazon.com/AmazonS3/latest/API/sigv4-auth-using-authorization-header.html
pub(crate) fn encode(text: &str, slash: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.' | b'~')
            || (slash && byte == b'/')
        {
            out.push(char::from(byte));
        } else {
            let _ignored = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// The text of the first `<tag>` element in `xml`, unescaped.
pub(crate) fn element(xml: &str, tag: &str) -> Option<String> {
    raw_elements(xml, tag).into_iter().next().map(unescape)
}

/// The contents of each `<tag>` element in `xml`, as written. The elements must not nest in
/// themselves, which none of S3's answers do.
pub(crate) fn raw_elements<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag}>");
    let empty = format!("<{tag}/>");
    let close = format!("</{tag}>");
    let mut found = Vec::new();
    let mut rest = xml;
    loop {
        let full = rest.split_once(open.as_str());
        let bare = rest.split_once(empty.as_str());
        match (full, bare) {
            (Some((before, after)), bare) if bare.is_none_or(|(b, _)| before.len() < b.len()) => {
                let Some((inner, next)) = after.split_once(close.as_str()) else {
                    break;
                };
                found.push(inner);
                rest = next;
            }
            (_, Some((_, next))) => {
                found.push("");
                rest = next;
            }
            _ => break,
        }
    }
    found
}

/// `text` with XML's predefined entities and character references replaced.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some((before, after)) = rest.split_once('&') {
        out.push_str(before);
        let Some((name, next)) = after.split_once(';') else {
            out.push('&');
            out.push_str(after);
            return out;
        };
        let decoded = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => name
                .strip_prefix("#x")
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        if let Some(c) = decoded {
            out.push(c);
            rest = next;
        } else {
            out.push('&');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// `text` with `&`, `<` and `>` escaped, for an element's text.
pub(crate) fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> Keys {
        Keys {
            id: "AKIAIOSFODNN7EXAMPLE".to_owned(),
            secret: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_owned(),
        }
    }

    fn may_24_2013() -> jiff::Timestamp {
        "2013-05-24T00:00:00Z".parse().unwrap()
    }

    /// The "GET Object" example of AWS's Signature Version 4 documentation [sigv4-header].
    ///
    /// [sigv4-header]: https://docs.aws.amazon.com/AmazonS3/latest/API/sig-v4-header-based-auth.html
    #[test]
    fn signs_the_get_object_example_as_aws_does() {
        let headers = [(
            HeaderName::from_static("range"),
            HeaderValue::from_static("bytes=0-9"),
        )];
        let value = authorization(
            &keys(),
            may_24_2013(),
            &Signed {
                method: "GET",
                path: "/test.txt",
                query: "",
                host: "examplebucket.s3.amazonaws.com",
                headers: &headers,
                payload: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            },
        );
        assert_eq!(
            value,
            "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, \
             SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, \
             Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
    }

    /// The "GET Bucket (List Objects)" example of the same page, with a sorted, encoded query.
    #[test]
    fn signs_the_list_objects_example_as_aws_does() {
        let query = canonical_query(&[("prefix", "J".to_owned()), ("max-keys", "2".to_owned())]);
        assert_eq!(query, "max-keys=2&prefix=J");
        let value = authorization(
            &keys(),
            may_24_2013(),
            &Signed {
                method: "GET",
                path: "/",
                query: &query,
                host: "examplebucket.s3.amazonaws.com",
                headers: &[],
                payload: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            },
        );
        assert!(
            value.ends_with(
                "Signature=34b48302e7b5fa45bde8084f4b7868a86f0a534bc59db6670ed5711ef69dc6f7"
            ),
            "{value}"
        );
    }

    #[test]
    fn keys_keep_their_slashes_and_escape_the_rest() {
        assert_eq!(
            path("photos", Some("2026/a b+c&ü.jpg")),
            "/photos/2026/a%20b%2Bc%26%C3%BC.jpg"
        );
        assert_eq!(canonical_query(&[("uploads", String::new())]), "uploads=");
        assert_eq!(
            canonical_query(&[("prefix", "a/b c".to_owned())]),
            "prefix=a%2Fb%20c"
        );
    }

    #[test]
    fn reads_the_elements_of_an_answer() {
        let xml = "<?xml version=\"1.0\"?><ListBucketResult xmlns=\"x\"><Prefix/>\
                   <Contents><Key>a &amp; b&#x2F;&#60;c&gt;</Key><Size>3</Size></Contents>\
                   <Contents><Key>d</Key><Size>4</Size></Contents>\
                   <CommonPrefixes><Prefix>e/</Prefix></CommonPrefixes></ListBucketResult>";
        let contents = raw_elements(xml, "Contents");
        assert_eq!(contents.len(), 2);
        assert_eq!(
            element(contents.first().unwrap(), "Key").as_deref(),
            Some("a & b/<c>")
        );
        assert_eq!(raw_elements(xml, "Prefix"), ["", "e/"]);
        assert_eq!(element(xml, "IsTruncated"), None);
        assert_eq!(unescape("x &bogus; &amp"), "x &bogus; &amp");
        assert_eq!(escape("<a & b>"), "&lt;a &amp; b&gt;");
    }
}
