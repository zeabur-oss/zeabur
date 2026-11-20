//! New-window requests in a cluster window: a `target="_blank"` link or a `window.open` call.
//! `WKWebView` drops a request that nobody handles, so without this such a link does nothing.
//! Navigation in the window itself is not changed: flows such as GitHub's App manifest post to
//! another site and come back to the local service in the same window.

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, Url, WebviewWindowBuilder, Wry};

use crate::output;

/// Where a cluster window opens a new-window request.
#[derive(Debug, PartialEq, Eq)]
enum Route {
    /// A page of the local service: in this window, which has the session.
    Window,
    /// Any other web page: in the default browser.
    Browser,
    /// Not a web page, such as `about:blank`: nowhere.
    Nothing,
}

fn route(url: &Url, service: Option<&Url>) -> Route {
    if !matches!(url.scheme(), "http" | "https") {
        Route::Nothing
    } else if service.is_some_and(|service| url.origin() == service.origin()) {
        Route::Window
    } else {
        Route::Browser
    }
}

/// Opens the new-window requests of the cluster window `label` in the browser, or in the window
/// when they are pages of its local service. The app never opens a second web view.
pub(crate) fn open_new_windows<'a>(
    handle: &AppHandle,
    label: &str,
    builder: WebviewWindowBuilder<'a, Wry, AppHandle>,
) -> WebviewWindowBuilder<'a, Wry, AppHandle> {
    let handle = handle.clone();
    let label = label.to_owned();
    builder.on_new_window(move |url, _| {
        let service = handle.state::<crate::App>().clusters.service(&label);
        match route(&url, service.as_ref()) {
            Route::Browser => {
                if let Err(err) = open_in_browser(url.as_str()) {
                    output::warn(&format!("cannot open {url} in the browser: {err}"));
                }
            }
            // The navigation runs later, outside the web view's callback.
            Route::Window => {
                let handle = handle.clone();
                let label = label.clone();
                tauri::async_runtime::spawn(async move {
                    if let Some(window) = handle.get_webview_window(&label)
                        && let Err(err) = window.navigate(url)
                    {
                        output::warn(&format!("cannot open the link: {err}"));
                    }
                });
            }
            Route::Nothing => {}
        }
        NewWindowResponse::Deny
    })
}

/// Opens a web page in the default browser.
pub(crate) fn open_in_browser(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("/usr/bin/open");
    #[cfg(windows)]
    let mut command = {
        let mut command = std::process::Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    command.arg(url).spawn().map(drop)
}

#[cfg(test)]
mod tests {
    use super::{Route, Url, route};

    fn url(text: &str) -> Url {
        text.parse().unwrap()
    }

    #[test]
    fn the_service_opens_in_the_window_and_other_web_pages_in_the_browser() {
        let service = url("http://127.0.0.1:52011/?code=one-time");
        let at = |text| route(&url(text), Some(&service));
        assert_eq!(at("http://127.0.0.1:52011/projects/a"), Route::Window);
        assert_eq!(at("https://app.example.com/"), Route::Browser);
        assert_eq!(at("http://127.0.0.1:3000/"), Route::Browser);
        assert_eq!(at("https://127.0.0.1:52011/"), Route::Browser);
    }

    #[test]
    fn a_request_that_is_not_a_web_page_opens_nowhere() {
        let service = url("http://127.0.0.1:52011/");
        assert_eq!(route(&url("about:blank"), Some(&service)), Route::Nothing);
        assert_eq!(
            route(&url("file:///Applications/Calculator.app"), Some(&service)),
            Route::Nothing
        );
    }

    #[test]
    fn before_the_service_runs_a_web_page_goes_to_the_browser() {
        assert_eq!(
            route(&url("https://github.com/zeabur-oss/zeabur"), None),
            Route::Browser
        );
    }
}
