//! Just enough HTTP/1.1 for a page and a JSON API on one machine: one request per connection,
//! bounded headers and bodies, no keep-alive.

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpStream;
use std::time::Duration;

const MAX_HEADER: usize = 16 * 1024;
const MAX_BODY: usize = 1024 * 1024;

/// A request.
#[derive(Debug)]
pub(crate) struct Request {
    pub method: String,
    /// The path without the query.
    pub path: String,
    pub query: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub(crate) fn query_param(&self, name: &str) -> Option<&str> {
        self.query.split('&').find_map(|pair| {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            (k == name).then_some(v)
        })
    }

    pub(crate) fn cookie(&self, name: &str) -> Option<&str> {
        self.header("cookie")?.split(';').find_map(|pair| {
            let (k, v) = pair.trim().split_once('=')?;
            (k == name).then_some(v)
        })
    }
}

/// A response.
#[derive(Debug)]
pub(crate) struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub(crate) fn new(status: u16, content_type: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            content_type,
            headers: Vec::new(),
            body: body.into(),
        }
    }

    pub(crate) fn json(status: u16, value: &serde_json::Value) -> Self {
        Self::new(status, "application/json", value.to_string())
    }

    pub(crate) fn with_header(mut self, name: &'static str, value: String) -> Self {
        self.headers.push((name, value));
        self
    }
}

/// Reads one request; `None` when the client sent something unusable.
pub(crate) fn read(stream: &TcpStream) -> Option<Request> {
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .ok()?;
    let mut reader = BufReader::new(stream.take((MAX_HEADER + MAX_BODY) as u64));
    let mut head = Vec::new();
    loop {
        let mut line = Vec::new();
        reader.read_until(b'\n', &mut line).ok()?;
        if line.is_empty() || head.len() + line.len() > MAX_HEADER {
            return None;
        }
        let end = line == b"\r\n" || line == b"\n";
        head.extend_from_slice(&line);
        if end {
            break;
        }
    }
    let head = String::from_utf8(head).ok()?;
    let mut lines = head.lines();
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_owned();
    let target = first.next()?;
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_owned(), v.trim().to_owned()))
        .collect();
    let length: usize = headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    if length > MAX_BODY {
        return None;
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some(Request {
        method,
        path: path.to_owned(),
        query: query.to_owned(),
        headers,
        body,
    })
}

/// Writes a response and closes the connection.
pub(crate) fn write(mut stream: &TcpStream, response: &Response) {
    let reason = match response.status {
        200 => "OK",
        302 => "Found",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        _ => "Error",
    };
    let mut head = format!(
        "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\
         Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\n\
         Content-Security-Policy: default-src 'self'; frame-ancestors 'none'\r\n",
        response.status,
        response.content_type,
        response.body.len()
    );
    for (name, value) in &response.headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    // The browser may have gone; nothing to do about it.
    drop(
        stream
            .set_write_timeout(Some(Duration::from_secs(30)))
            .and_then(|()| stream.write_all(head.as_bytes()))
            .and_then(|()| stream.write_all(&response.body)),
    );
}
