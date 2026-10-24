//! The few pages the servers write themselves, outside the web interface.
//!
//! Each is a short static text in the person's language (English, Traditional or Simplified
//! Chinese, from `Accept-Language`), with no inline script or style. The policy is the web
//! interface's, without what these pages do not use.

use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;

/// Content Security Policy of these pages.
pub const CSP: &str = "default-src 'none'; script-src 'self'; base-uri 'none'; form-action 'none'; \
frame-ancestors 'none'; require-trusted-types-for 'script'; trusted-types 'none'";

/// A language Zeabur speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// English.
    En,
    /// Traditional Chinese, Taiwan.
    ZhTw,
    /// Simplified Chinese.
    ZhCn,
}

impl Lang {
    /// The first language of `Accept-Language` Zeabur speaks; weights are ignored, as browsers
    /// list languages in order of preference.
    #[must_use]
    pub fn of(headers: &HeaderMap) -> Self {
        let accept = headers
            .get(header::ACCEPT_LANGUAGE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        for tag in accept.split(',') {
            let tag = tag
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            match tag.as_str() {
                "zh-tw" | "zh-hk" | "zh-mo" | "zh-hant" => return Self::ZhTw,
                _ if tag.starts_with("zh-hant") => return Self::ZhTw,
                _ if tag == "zh" || tag.starts_with("zh-") => return Self::ZhCn,
                _ if tag == "en" || tag.starts_with("en-") => return Self::En,
                _ => {}
            }
        }
        Self::En
    }

    /// The tag of `<html lang>`.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::ZhTw => "zh-TW",
            Self::ZhCn => "zh-CN",
        }
    }
}

/// A page of `body`, with `attributes` on `<html>` and an optional script from this origin.
#[must_use]
pub fn html(
    status: StatusCode,
    lang: Lang,
    attributes: &str,
    script: Option<&str>,
    body: &str,
) -> Response {
    let script = script.map_or_else(String::new, |src| {
        format!(r#"<script src="{src}"></script>"#)
    });
    let page = format!(
        "<!doctype html><html lang=\"{}\"{attributes}><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Zeabur</title>\
{script}</head><body>{body}</body></html>",
        lang.code()
    );
    let mut response = Response::new(Body::from(page));
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_LANGUAGE,
        HeaderValue::from_static(lang.code()),
    );
    secure(headers);
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CSP),
    );
    response
}

/// Headers every answer of the sign-in routes carries: not stored, not framed, not sniffed, and
/// no `Referer` (the issuer's answer carries a code in the address).
pub fn secure(headers: &mut HeaderMap) {
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accept(value: &'static str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::ACCEPT_LANGUAGE, HeaderValue::from_static(value));
        headers
    }

    #[test]
    fn the_language_follows_accept_language() {
        assert_eq!(Lang::of(&HeaderMap::new()), Lang::En);
        assert_eq!(Lang::of(&accept("zh-TW,zh;q=0.9,en;q=0.8")), Lang::ZhTw);
        assert_eq!(Lang::of(&accept("zh-Hant-HK")), Lang::ZhTw);
        assert_eq!(Lang::of(&accept("zh-CN")), Lang::ZhCn);
        assert_eq!(Lang::of(&accept("fr, zh;q=0.5")), Lang::ZhCn);
        assert_eq!(Lang::of(&accept("en-GB, zh-TW")), Lang::En);
    }

    #[test]
    fn pages_run_no_inline_script_and_cannot_be_framed() {
        let page = html(StatusCode::OK, Lang::En, "", None, "<p>Zeabur</p>");
        let headers = page.headers();
        assert_eq!(headers[header::CONTENT_SECURITY_POLICY], CSP);
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
        assert_eq!(headers[header::X_FRAME_OPTIONS], "DENY");
        assert!(!CSP.contains("unsafe"));
    }
}
