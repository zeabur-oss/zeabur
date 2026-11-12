//! How the windows look and behave as a desktop app: the macOS title bar, window sizes that are
//! kept between launches, and what happens when the last window closes.

use std::path::PathBuf;

use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, Wry};
use tauri_plugin_window_state::StateFlags;

use crate::shots;

/// The label of the cluster chooser window.
pub const CHOOSER: &str = "main";

/// Below this width the web interface hides its sidebar, and the traffic lights would sit on the
/// page.
const CLUSTER_MIN: (f64, f64) = (800.0, 560.0);

/// The height of the macOS title bar in points. The page leaves this much room at the top, in CSS
/// pixels divided by the page's zoom (`--zeabur-title-bar`).
#[cfg(target_os = "macos")]
const TITLE_BAR: f64 = 28.0;

/// The zoom steps of View > Zoom In and Zoom Out, as in Safari and Chrome.
#[cfg(target_os = "macos")]
const ZOOM_STEPS: [f64; 11] = [0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];

/// The zoom of each window by label; WebKit has no call to read it back.
#[cfg(target_os = "macos")]
static ZOOM: std::sync::Mutex<Option<std::collections::HashMap<String, f64>>> =
    std::sync::Mutex::new(None);

/// Puts `data-titlebar` on the page and, once the page has a body, a strip that moves the
/// window. The strip is above the page and below dialogs and menus (`z-index` 50 and more).
#[cfg(target_os = "macos")]
fn title_bar_script() -> String {
    let css = "[data-titlebar] [data-slot=frame-sidebar] > div,\
         [data-titlebar] [data-slot=frame-panel],\
         [data-titlebar] [data-titlebar-inset] > * { border-top: var(--zeabur-title-bar) solid transparent }\
         [data-titlebar] body:not(:has([data-slot=app-frame], [data-titlebar-inset])) { padding-top: var(--zeabur-title-bar) }\
         #zeabur-title-bar { position: fixed; inset: 0 0 auto 0; height: var(--zeabur-title-bar); z-index: 40 }";
    format!(
        "document.documentElement.dataset.titlebar = 'overlay';\
         document.documentElement.style.setProperty('--zeabur-title-bar', '{TITLE_BAR}px');\
         document.addEventListener('DOMContentLoaded', () => {{\
           const style = document.createElement('style');\
           style.textContent = {css:?};\
           document.head.append(style);\
           const bar = document.createElement('div');\
           bar.id = 'zeabur-title-bar';\
           bar.setAttribute('data-tauri-drag-region', '');\
           document.body.append(bar);\
         }});"
    )
}

/// On macOS the page fills the window and the title bar is drawn over it, as in Finder and Notes.
/// Windows and Linux keep the system title bar.
fn title_bar<'a, M: Manager<Wry>>(
    builder: WebviewWindowBuilder<'a, Wry, M>,
) -> WebviewWindowBuilder<'a, Wry, M> {
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .initialization_script(title_bar_script())
        .on_page_load(|window, payload| {
            if payload.event() == tauri::webview::PageLoadEvent::Finished {
                fit_title_bar(&window, zoom_of(&window));
            }
        });
    builder
}

#[cfg(target_os = "macos")]
fn zoom_of(window: &tauri::WebviewWindow) -> f64 {
    let zooms = ZOOM.lock().unwrap_or_else(|e| e.into_inner());
    zooms
        .as_ref()
        .and_then(|z| z.get(window.label()).copied())
        .unwrap_or(1.0)
}

/// A zoomed page has larger CSS pixels, but the traffic lights keep their size.
#[cfg(target_os = "macos")]
fn fit_title_bar(window: &tauri::WebviewWindow, zoom: f64) {
    let height = TITLE_BAR / zoom;
    let _ = window.eval(format!(
        "document.documentElement.style.setProperty('--zeabur-title-bar', '{height}px')"
    ));
}

/// View > Zoom In (`Some(true)`), Zoom Out (`Some(false)`) or Actual Size (`None`).
#[cfg(target_os = "macos")]
pub fn zoom(window: &tauri::WebviewWindow, up: Option<bool>) {
    let next = match up {
        Some(up) => next_zoom(zoom_of(window), up),
        None => 1.0,
    };
    if window.set_zoom(next).is_ok() {
        let mut zooms = ZOOM.lock().unwrap_or_else(|e| e.into_inner());
        zooms
            .get_or_insert_with(Default::default)
            .insert(window.label().to_owned(), next);
        drop(zooms);
        fit_title_bar(window, next);
    }
}

/// The next zoom step from `current`, up or down; the same step at either end.
#[cfg(target_os = "macos")]
fn next_zoom(current: f64, up: bool) -> f64 {
    if up {
        for step in ZOOM_STEPS {
            if step > current + 0.001 {
                return step;
            }
        }
        ZOOM_STEPS[ZOOM_STEPS.len() - 1]
    } else {
        for step in ZOOM_STEPS.iter().rev() {
            if *step < current - 0.001 {
                return *step;
            }
        }
        ZOOM_STEPS[0]
    }
}

/// Lets the strip move the window, and zoom it on a double click. Cluster windows load the web
/// interface from `web_url`, which gets these two permissions and nothing else.
#[cfg(target_os = "macos")]
pub fn allow_title_bar(app: &AppHandle, web_url: &tauri::Url) -> tauri::Result<()> {
    let capability = tauri::ipc::CapabilityBuilder::new("title-bar")
        .windows([CHOOSER, "cluster-*"])
        .remote(format!("{}/*", web_url.origin().ascii_serialization()))
        .permission("core:window:allow-start-dragging")
        .permission("core:window:allow-internal-toggle-maximize");
    app.add_capability(capability)
}

#[cfg(not(target_os = "macos"))]
pub fn allow_title_bar(_app: &AppHandle, _web_url: &tauri::Url) -> tauri::Result<()> {
    Ok(())
}

/// Shows the chooser, opening it again if it was closed.
pub fn show_chooser(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(CHOOSER) {
        window.unminimize()?;
        window.show()?;
        return window.set_focus();
    }
    let builder = WebviewWindowBuilder::new(app, CHOOSER, WebviewUrl::App("desktop.html".into()))
        .title("Zeabur")
        .inner_size(1040.0, 680.0)
        .min_inner_size(720.0, 480.0);
    let window = title_bar(shots::apply_theme(builder)).build()?;
    if shots::wants("main") {
        shots::capture(window);
    }
    Ok(())
}

/// Size, title bar and starting place of a cluster window. A window seen before gets its saved
/// place back from the window-state plugin; a new one is set a little below and right of the
/// last, so it does not hide it.
pub fn cluster_window<'a>(
    app: &AppHandle,
    builder: WebviewWindowBuilder<'a, Wry, AppHandle>,
) -> WebviewWindowBuilder<'a, Wry, AppHandle> {
    let mut open = 0.0;
    for label in app.webview_windows().keys() {
        if label.starts_with("cluster-") {
            open += 1.0;
        }
    }
    let step = 24.0 * open;
    let builder = builder
        .inner_size(1280.0, 800.0)
        .min_inner_size(CLUSTER_MIN.0, CLUSTER_MIN.1)
        .position(120.0 + step, 80.0 + step);
    title_bar(builder)
}

/// Keeps each window's size, place and maximized state between launches, under its label: `main`
/// for the chooser and `cluster-<hash>` for each context. With `ZEABUR_DESKTOP_STATE` the file goes
/// in that folder, so the fixtures and screenshots start from nothing.
pub fn window_state() -> tauri::plugin::TauriPlugin<Wry> {
    let mut builder = tauri_plugin_window_state::Builder::new()
        .with_state_flags(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED);
    if let Some(dir) = std::env::var_os("ZEABUR_DESKTOP_STATE") {
        let path = PathBuf::from(dir).join("window-state.json");
        builder = builder.with_filename(path.to_string_lossy());
    }
    builder.build()
}

/// On macOS the app stays open with no windows, like other Mac apps, and a click on the Dock icon
/// brings the chooser back. Windows and Linux quit when the last window closes.
pub fn on_run_event(app: &AppHandle, event: RunEvent) {
    match event {
        #[cfg(target_os = "macos")]
        RunEvent::ExitRequested {
            code: None, api, ..
        } => api.prevent_exit(),
        #[cfg(target_os = "macos")]
        RunEvent::Reopen {
            has_visible_windows: false,
            ..
        } => {
            if let Err(err) = show_chooser(app) {
                eprintln!("zeabur-desktop: cannot open the chooser: {err}");
            }
        }
        _ => {
            let _ = app;
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::next_zoom;

    #[test]
    fn zoom_moves_one_step_and_stops_at_the_ends() {
        assert_eq!(next_zoom(1.0, true), 1.1);
        assert_eq!(next_zoom(1.0, false), 0.9);
        assert_eq!(next_zoom(2.0, true), 2.0);
        assert_eq!(next_zoom(0.5, false), 0.5);
    }
}
