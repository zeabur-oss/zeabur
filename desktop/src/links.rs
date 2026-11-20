//! Links in a cluster window. The window shows the app's own page and then its local service; a web
//! page anywhere else opens in the default browser. `WKWebView` drops a new-window request that
//! nobody handles, so without this a `target="_blank"` link does nothing.

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, Url, WebviewWindowBuilder, Wry};

use crate::output;

/// Where a cluster window loads a URL.
#[derive(Debug, PartialEq, Eq)]
enum Route {
    Window,
    Browser,
}

/// A web page outside the local service goes to the browser. Until the service runs, the window
/// shows only the app's own page, and every URL stays.
fn route(url: &Url, service: Option<&Url>) -> Route {
    match service {
        Some(service)
            if matches!(url.scheme(), "http" | "https") && url.origin() != service.origin() =>
        {
            Route::Browser
        }
        _ => Route::Window,
    }
}

/// Sends the cluster window `label`'s links to the browser when they leave its local service.
pub(crate) fn keep_to_service<'a>(
    handle: &AppHandle,
    label: &str,
    builder: WebviewWindowBuilder<'a, Wry, AppHandle>,
) -> WebviewWindowBuilder<'a, Wry, AppHandle> {
    let service = {
        let handle = handle.clone();
        let label = label.to_owned();
        move || handle.state::<crate::App>().clusters.service(&label)
    };
    let on_navigation = {
        let service = service.clone();
        move |url: &Url| match route(url, service().as_ref()) {
            Route::Window => true,
            Route::Browser => {
                open_or_warn(url);
                false
            }
        }
    };
    let handle = handle.clone();
    let label = label.to_owned();
    builder
        .on_navigation(on_navigation)
        .on_new_window(move |url, _| {
            match route(&url, service().as_ref()) {
                Route::Browser => open_or_warn(&url),
                // A link to the service itself loads in this window. The navigation runs later,
                // outside the web view's callback.
                Route::Window if matches!(url.scheme(), "http" | "https") => {
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
                Route::Window => {}
            }
            NewWindowResponse::Deny
        })
}

fn open_or_warn(url: &Url) {
    if let Err(err) = open_in_browser(url.as_str()) {
        output::warn(&format!("cannot open {url} in the browser: {err}"));
    }
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
    fn the_service_stays_and_other_web_pages_go_to_the_browser() {
        let service = url("http://127.0.0.1:52011/?code=one-time");
        let at = |text| route(&url(text), Some(&service));
        assert_eq!(at("http://127.0.0.1:52011/projects/a"), Route::Window);
        assert_eq!(at("https://app.example.com/"), Route::Browser);
        assert_eq!(at("http://127.0.0.1:3000/"), Route::Browser);
        assert_eq!(at("https://127.0.0.1:52011/"), Route::Browser);
        assert_eq!(at("about:blank"), Route::Window);
    }

    #[test]
    fn before_the_service_runs_the_app_page_stays() {
        assert_eq!(
            route(&url("http://tauri.localhost/cluster.html"), None),
            Route::Window
        );
        assert_eq!(
            route(&url("tauri://localhost/cluster.html"), None),
            Route::Window
        );
    }
}
