//! The few pages the server writes itself, outside the web interface: a sign-in that could not
//! finish, and the end of "Retry as admin" in its own window.
//!
//! Each is a short static text in the person's language (English, Traditional or Simplified
//! Chinese, from `Accept-Language`), with no inline script or style: the elevation page loads
//! one script from this origin and passes its result in `data-` attributes. The policy is the
//! web interface's, without what these pages do not use.

use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;

/// Content Security Policy of these pages.
const CSP: &str = "default-src 'none'; script-src 'self'; base-uri 'none'; form-action 'none'; \
frame-ancestors 'none'; require-trusted-types-for 'script'; trusted-types 'none'";

/// The script of the elevation page.
pub(crate) const ELEVATED_JS: &str = include_str!("elevated.js");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    En,
    ZhTw,
    ZhCn,
}

impl Lang {
    /// The first language of `Accept-Language` Zeabur speaks; weights are ignored, as browsers
    /// list languages in order of preference.
    fn of(headers: &HeaderMap) -> Self {
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

    const fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::ZhTw => "zh-TW",
            Self::ZhCn => "zh-CN",
        }
    }
}

/// Why a sign-in could not finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SignInProblem {
    /// The issuer answered with an error, or the person cancelled.
    Failed,
    /// No sign-in in progress matches the answer: expired, or started in another browser.
    Expired,
    /// Zeabur Identity could not be reached.
    IssuerUnreachable,
    /// kube-apiserver did not accept the token.
    ClusterRefused,
}

impl SignInProblem {
    const fn text(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (Self::Failed, Lang::En) => "Sign-in did not finish.",
            (Self::Failed, Lang::ZhTw) => "登入沒有完成。",
            (Self::Failed, Lang::ZhCn) => "登录没有完成。",
            (Self::Expired, Lang::En) => "This sign-in expired or was started in another browser.",
            (Self::Expired, Lang::ZhTw) => "這次登入已過期，或是從其他瀏覽器開始的。",
            (Self::Expired, Lang::ZhCn) => "这次登录已过期，或是在其他浏览器中发起的。",
            (Self::IssuerUnreachable, Lang::En) => {
                "Unable to reach the sign-in service. Try again in a moment."
            }
            (Self::IssuerUnreachable, Lang::ZhTw) => "無法連上登入服務，稍後再試一次。",
            (Self::IssuerUnreachable, Lang::ZhCn) => "无法连接登录服务，稍后再试一次。",
            (Self::ClusterRefused, Lang::En) => {
                "The cluster did not accept your sign-in. Ask a cluster administrator to check your access."
            }
            (Self::ClusterRefused, Lang::ZhTw) => "叢集不接受這次登入。請叢集管理員確認你的權限。",
            (Self::ClusterRefused, Lang::ZhCn) => "集群不接受这次登录。请集群管理员确认你的权限。",
        }
    }

    const fn status(self) -> StatusCode {
        match self {
            Self::Failed | Self::Expired => StatusCode::BAD_REQUEST,
            Self::IssuerUnreachable => StatusCode::SERVICE_UNAVAILABLE,
            Self::ClusterRefused => StatusCode::FORBIDDEN,
        }
    }
}

/// How "Retry as admin" ended; the web interface reads the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Elevated {
    /// Granted until this time (RFC 3339).
    Done(String),
    /// This client may not ask for elevation (Zeabur Identity's `invalid_scope`).
    NotAllowed,
    /// Cancelled at the issuer, or refused there.
    Cancelled,
    /// Anything else: an expired attempt, a token for someone else, an unreachable issuer.
    Failed,
    /// The session ended in the meantime.
    SignedOut,
}

impl Elevated {
    const fn result(&self) -> &'static str {
        match self {
            Self::Done(_) => "done",
            Self::NotAllowed => "not-allowed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::SignedOut => "signed-out",
        }
    }

    const fn text(&self, lang: Lang) -> &'static str {
        match (self, lang) {
            (Self::Done(_), Lang::En) => "You have admin rights now. You can close this window.",
            (Self::Done(_), Lang::ZhTw) => "已取得管理員權限，可以關閉這個視窗。",
            (Self::Done(_), Lang::ZhCn) => "已获得管理员权限，可以关闭这个窗口。",
            (Self::NotAllowed | Self::Cancelled | Self::Failed | Self::SignedOut, Lang::En) => {
                "Admin rights were not granted. You can close this window."
            }
            (Self::NotAllowed | Self::Cancelled | Self::Failed | Self::SignedOut, Lang::ZhTw) => {
                "沒有取得管理員權限，可以關閉這個視窗。"
            }
            (Self::NotAllowed | Self::Cancelled | Self::Failed | Self::SignedOut, Lang::ZhCn) => {
                "没有获得管理员权限，可以关闭这个窗口。"
            }
        }
    }
}

fn html(status: StatusCode, lang: Lang, attributes: &str, script: bool, body: &str) -> Response {
    let script = if script {
        r#"<script src="/auth/elevated.js"></script>"#
    } else {
        ""
    };
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
    secure(headers);
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CSP),
    );
    response
}

/// Headers every answer of the sign-in routes carries: not stored, not framed, not sniffed, and
/// no `Referer` (the issuer's answer carries a code in the address).
pub(crate) fn secure(headers: &mut HeaderMap) {
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

/// The page for a sign-in that could not finish, with a link to start again.
pub(crate) fn sign_in_problem(request: &HeaderMap, problem: SignInProblem) -> Response {
    let lang = Lang::of(request);
    let again = match lang {
        Lang::En => "Sign in again",
        Lang::ZhTw => "重新登入",
        Lang::ZhCn => "重新登录",
    };
    html(
        problem.status(),
        lang,
        "",
        false,
        &format!(
            "<p>{}</p><p><a href=\"/auth/login\">{again}</a></p>",
            problem.text(lang)
        ),
    )
}

/// The end of "Retry as admin": tells Zeabur's page and closes the window.
pub(crate) fn elevated(request: &HeaderMap, outcome: &Elevated) -> Response {
    let lang = Lang::of(request);
    let until = match outcome {
        Elevated::Done(until) => format!(" data-until=\"{until}\""),
        Elevated::NotAllowed | Elevated::Cancelled | Elevated::Failed | Elevated::SignedOut => {
            String::new()
        }
    };
    html(
        StatusCode::OK,
        lang,
        &format!(" data-result=\"{}\"{until}", outcome.result()),
        true,
        &format!("<p>{}</p>", outcome.text(lang)),
    )
}

/// The elevation page's script.
pub(crate) fn script() -> Response {
    let mut response = Response::new(Body::from(ELEVATED_JS));
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/javascript; charset=utf-8"),
    );
    secure(headers);
    response
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
        let page = elevated(&HeaderMap::new(), &Elevated::NotAllowed);
        let headers = page.headers();
        assert_eq!(headers[header::CONTENT_SECURITY_POLICY], CSP);
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
        assert_eq!(headers[header::X_FRAME_OPTIONS], "DENY");
        assert!(!CSP.contains("unsafe"));
    }
}
