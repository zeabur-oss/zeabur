//! How the windows look and behave as a desktop app: the macOS title bar, window sizes that are
//! kept between launches, zoom, and what happens when the last window closes.

use std::path::PathBuf;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::sync::PoisonError;

use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, Wry};
use tauri_plugin_window_state::StateFlags;

use crate::{output, shots};

/// The label of the cluster chooser window.
pub(crate) const CHOOSER: &str = "main";

/// Below this width the web interface hides its sidebar, and the traffic lights would sit on the
/// page.
#[cfg(target_os = "macos")]
const CLUSTER_MIN: (f64, f64) = (800.0, 560.0);

/// Snap layouts need a minimum width of at most 500 effective pixels
/// (<https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/apply-snap-layout-menu>),
/// so two cluster windows fit side by side. Without traffic lights the web interface may hide its
/// sidebar.
#[cfg(not(target_os = "macos"))]
const CLUSTER_MIN: (f64, f64) = (500.0, 560.0);

/// Room for the system title bar above a window's content: 32 px on Windows
/// (<https://learn.microsoft.com/en-us/windows/apps/design/basics/titlebar-design>). The macOS title
/// bar is drawn over the page.
#[cfg(target_os = "macos")]
const FRAME_TOP: f64 = 0.0;
#[cfg(not(target_os = "macos"))]
const FRAME_TOP: f64 = 32.0;

/// The height of the macOS title bar in points. The page leaves this much room at the top, in CSS
/// pixels divided by the page's zoom (`--zeabur-title-bar`).
#[cfg(target_os = "macos")]
const TITLE_BAR: f64 = 28.0;

/// The zoom steps of View > Zoom In and Zoom Out on macOS and of Ctrl++ and Ctrl+- on Linux, as in
/// Safari and Chrome.
#[cfg(any(target_os = "macos", target_os = "linux", test))]
const ZOOM_STEPS: [f64; 11] = [0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];

/// The zoom of each open window by label; WebKit has no call to read it back. A window that
/// closes leaves the map, so a new window with the same label starts at 1.0, as its web view does.
#[cfg(any(target_os = "macos", target_os = "linux"))]
static ZOOM: std::sync::Mutex<std::collections::BTreeMap<String, f64>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

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
#[cfg(target_os = "macos")]
fn title_bar<M: Manager<Wry>>(
    builder: WebviewWindowBuilder<'_, Wry, M>,
) -> WebviewWindowBuilder<'_, Wry, M> {
    builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .initialization_script(title_bar_script())
        .on_page_load(|window, payload| {
            if payload.event() == tauri::webview::PageLoadEvent::Finished {
                fit_title_bar(&window, zoom_of(&window));
            }
        })
}

/// Windows and Linux keep the system title bar.
#[cfg(not(target_os = "macos"))]
const fn title_bar<M: Manager<Wry>>(
    builder: WebviewWindowBuilder<'_, Wry, M>,
) -> WebviewWindowBuilder<'_, Wry, M> {
    builder
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn zoom_of(window: &tauri::WebviewWindow) -> f64 {
    stored_zoom(window.label())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn stored_zoom(label: &str) -> f64 {
    ZOOM.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(label)
        .copied()
        .unwrap_or(1.0)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn store_zoom(label: &str, zoom: Option<f64>) {
    let mut zooms = ZOOM.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(zoom) = zoom {
        zooms.insert(label.to_owned(), zoom);
    } else {
        zooms.remove(label);
    }
}

/// A zoomed page has larger CSS pixels, but the traffic lights keep their size.
#[cfg(target_os = "macos")]
fn fit_title_bar(window: &tauri::WebviewWindow, zoom: f64) {
    let height = TITLE_BAR / zoom;
    if let Err(err) = window.eval(format!(
        "document.documentElement.style.setProperty('--zeabur-title-bar', '{height}px')"
    )) {
        output::warn(&format!("cannot fit the title bar to the zoom: {err}"));
    }
}

/// View > Zoom In (`Some(true)`), Zoom Out (`Some(false)`) or Actual Size (`None`).
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn zoom(window: &tauri::WebviewWindow, up: Option<bool>) {
    let next = up.map_or(1.0, |up| next_zoom(zoom_of(window), up));
    if window.set_zoom(next).is_ok() {
        store_zoom(window.label(), Some(next));
        #[cfg(target_os = "macos")]
        fit_title_bar(window, next);
    }
}

/// The next zoom step from `current`, up or down; the same step at either end.
#[cfg(any(target_os = "macos", target_os = "linux", test))]
fn next_zoom(current: f64, up: bool) -> f64 {
    let mut next = current;
    if up {
        for step in ZOOM_STEPS {
            next = step;
            if step > current + 0.001 {
                break;
            }
        }
    } else {
        for step in ZOOM_STEPS.into_iter().rev() {
            next = step;
            if step < current - 0.001 {
                break;
            }
        }
    }
    next
}

/// Lets the strip move the window, and zoom it on a double click. Cluster windows load the web
/// interface from `web_url`, which gets these two permissions and nothing else.
#[cfg(target_os = "macos")]
pub(crate) fn allow_title_bar(app: &AppHandle, web_url: &tauri::Url) -> tauri::Result<()> {
    let capability = tauri::ipc::CapabilityBuilder::new("title-bar")
        .windows([CHOOSER, "cluster-*"])
        .remote(format!("{}/*", web_url.origin().ascii_serialization()))
        .permission("core:window:allow-start-dragging")
        .permission("core:window:allow-internal-toggle-maximize");
    app.add_capability(capability)
}

/// On Linux the program name is the Wayland app ID and the X11 window class, which GNOME matches to
/// the `.desktop` file of the same name for the app's name and icon. The freedesktop desktop entry
/// specification asks for a reverse DNS name
/// (<https://specifications.freedesktop.org/desktop-entry/latest/file-naming.html>), so it is the
/// app's identifier. Tauri's `enableGTKAppId` would do the same through a D-Bus name, which allows
/// one instance only.
#[cfg(target_os = "linux")]
pub(crate) fn set_app_id(identifier: &str) {
    gtk::glib::set_prgname(Some(identifier));
}

/// On Windows the AppUserModelID groups the app's windows under one taskbar button and names its
/// jump list. It must be set before any window is shown
/// (<https://learn.microsoft.com/en-us/windows/win32/shell/appids>). It is the identifier, which
/// Tauri's installers also give the Start menu shortcut.
#[cfg(windows)]
pub(crate) fn set_app_id(identifier: &str) {
    if let Err(err) = crate::win32::set_app_user_model_id(identifier) {
        output::warn(&format!("cannot set the AppUserModelID: {err}"));
    }
}

#[cfg(target_os = "macos")]
pub(crate) const fn set_app_id(_identifier: &str) {}

/// The primary screen's work area (without the taskbar or Dock) in points: place and size.
fn work_area(app: &AppHandle) -> Option<Area> {
    let mut monitor = app.primary_monitor().ok().flatten();
    if monitor.is_none() {
        monitor = app
            .available_monitors()
            .ok()
            .and_then(|all| all.into_iter().next());
    }
    let monitor = monitor?;
    let area = monitor.work_area();
    let place = area.position.to_logical::<f64>(monitor.scale_factor());
    let size = area.size.to_logical::<f64>(monitor.scale_factor());
    Some(Area {
        x: place.x,
        y: place.y,
        width: size.width,
        height: size.height,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Area {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// A new window's size: `want`, or 90 % of the work area where that is smaller. A 1080p screen at
/// 150 % is 1280 × 720 points, smaller than a cluster window.
fn fit(want: (f64, f64), area: Area) -> (f64, f64) {
    (want.0.min(area.width * 0.9), want.1.min(area.height * 0.9))
}

/// Where a new window of content `size` goes: `offset` from the work area's corner, or centred
/// when there is none, then moved up and left until the whole window, title bar included, is inside
/// the work area. Windows' guidance: "adjust the initial location so that the entire window is
/// visible within the target monitor"
/// (<https://learn.microsoft.com/en-us/windows/win32/uxguide/win-window-mgt>).
fn place(offset: Option<(f64, f64)>, size: (f64, f64), area: Area) -> (f64, f64) {
    let outer = (size.0, size.1 + FRAME_TOP);
    let (x, y) = match offset {
        Some((dx, dy)) => (area.x + dx, area.y + dy),
        None => (
            area.x + (area.width - outer.0) / 2.0,
            area.y + (area.height - outer.1) / 2.0,
        ),
    };
    let x = x.min(area.x + area.width - outer.0).max(area.x);
    let y = y.min(area.y + area.height - outer.1).max(area.y);
    (x, y)
}

/// Sets up a window once it is built: on Windows and Linux, the shortcuts in `keys.rs`. macOS has
/// them in its menu bar.
#[cfg(any(target_os = "linux", windows))]
pub(crate) fn window_built(window: &tauri::WebviewWindow) {
    crate::keys::attach(window);
}

#[cfg(target_os = "macos")]
pub(crate) const fn window_built(_window: &tauri::WebviewWindow) {}

/// Windows also gets WebView2's own Ctrl++, Ctrl+- and Ctrl+0 zoom (reload with Ctrl+R and F5 is on
/// by default).
#[cfg(windows)]
fn zoom_keys<M: Manager<Wry>>(
    builder: WebviewWindowBuilder<'_, Wry, M>,
) -> WebviewWindowBuilder<'_, Wry, M> {
    builder.zoom_hotkeys_enabled(true)
}

#[cfg(not(windows))]
const fn zoom_keys<M: Manager<Wry>>(
    builder: WebviewWindowBuilder<'_, Wry, M>,
) -> WebviewWindowBuilder<'_, Wry, M> {
    builder
}

/// Shows the chooser, opening it again if it was closed.
pub(crate) fn show_chooser(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(CHOOSER) {
        window.unminimize()?;
        window.show()?;
        return window.set_focus();
    }
    let mut builder =
        WebviewWindowBuilder::new(app, CHOOSER, WebviewUrl::App("desktop.html".into()))
            .title("Zeabur")
            .inner_size(1040.0, 680.0)
            .min_inner_size(720.0, 480.0);
    if let Some(area) = work_area(app) {
        let size = fit((1040.0, 680.0), area);
        let (x, y) = place(None, size, area);
        builder = builder.inner_size(size.0, size.1).position(x, y);
    }
    let window = zoom_keys(title_bar(shots::apply_theme(builder))).build()?;
    window_built(&window);
    if shots::wants("main") {
        shots::capture(window);
    }
    Ok(())
}

/// Size, title bar and starting place of a cluster window. A window seen before gets its saved
/// place back from the window-state plugin; a new one is set a little below and right of the
/// last, so it does not hide it.
pub(crate) fn cluster_window<'a>(
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
    let mut builder = builder
        .inner_size(1280.0, 800.0)
        .min_inner_size(CLUSTER_MIN.0, CLUSTER_MIN.1);
    if let Some(area) = work_area(app) {
        let size = fit((1280.0, 800.0), area);
        let (x, y) = place(Some((120.0 + step, 80.0 + step)), size, area);
        builder = builder.inner_size(size.0, size.1).position(x, y);
    }
    zoom_keys(title_bar(builder))
}

/// Keeps each window's size, place and maximized state between launches, under its label: `main`
/// for the chooser and `cluster-<hash>` for each context. With `ZEABUR_DESKTOP_STATE` the file goes
/// in that folder, so the fixtures and screenshots start from nothing.
pub(crate) fn window_state() -> tauri::plugin::TauriPlugin<Wry> {
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
#[cfg_attr(
    windows,
    expect(clippy::needless_pass_by_value, reason = "the signature Tauri calls")
)]
pub(crate) fn on_run_event(
    #[cfg_attr(
        not(target_os = "macos"),
        expect(unused_variables, reason = "only macOS opens the chooser again")
    )]
    app: &AppHandle,
    event: RunEvent,
) {
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
                output::warn(&format!("cannot open the chooser: {err}"));
            }
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        RunEvent::WindowEvent {
            label,
            event: tauri::WindowEvent::Destroyed,
            ..
        } => store_zoom(&label, None),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{Area, FRAME_TOP, fit, next_zoom, place};

    fn area(x: f64, y: f64, width: f64, height: f64) -> Area {
        Area {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_window_larger_than_the_screen_shrinks_to_fit() {
        let small = area(0.0, 0.0, 1280.0, 720.0);
        assert_eq!(fit((1280.0, 800.0), small), (1152.0, 648.0));
        let large = area(0.0, 0.0, 2560.0, 1400.0);
        assert_eq!(fit((1280.0, 800.0), large), (1280.0, 800.0));
    }

    #[test]
    fn a_window_without_an_offset_is_centred_title_bar_included() {
        let screen = area(0.0, 0.0, 1512.0, 808.0);
        let (x, y) = place(None, (1040.0, 680.0), screen);
        assert_eq!(x, 236.0);
        assert_eq!(y, (808.0 - 680.0 - FRAME_TOP) / 2.0);
    }

    #[test]
    fn a_cascaded_window_moves_back_inside_the_work_area() {
        // 1512 × 808 points is a 3024 × 1712 screen at 200 % with the taskbar at the bottom.
        let screen = area(0.0, 0.0, 1512.0, 808.0);
        let size = fit((1280.0, 800.0), screen);
        let (x, y) = place(Some((120.0, 80.0)), size, screen);
        assert!(x + size.0 <= 1512.0);
        assert!(y + FRAME_TOP + size.1 <= 808.0);
        assert_eq!(x, 120.0);
    }

    #[test]
    fn an_offset_counts_from_the_work_area_corner() {
        let below_a_top_bar = area(0.0, 40.0, 1920.0, 1040.0);
        assert_eq!(
            place(Some((120.0, 80.0)), (1280.0, 800.0), below_a_top_bar),
            (120.0, 120.0)
        );
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn a_closed_window_forgets_its_zoom() {
        use super::{store_zoom, stored_zoom};
        store_zoom("cluster-test", Some(2.0));
        assert_eq!(stored_zoom("cluster-test"), 2.0);
        store_zoom("cluster-test", None);
        assert_eq!(stored_zoom("cluster-test"), 1.0);
    }

    #[test]
    fn zoom_moves_one_step_and_stops_at_the_ends() {
        assert_eq!(next_zoom(1.0, true), 1.1);
        assert_eq!(next_zoom(1.0, false), 0.9);
        assert_eq!(next_zoom(2.0, true), 2.0);
        assert_eq!(next_zoom(0.5, false), 0.5);
    }
}
