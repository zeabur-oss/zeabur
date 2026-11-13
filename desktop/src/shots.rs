//! README screenshot hooks.

use tauri::{WebviewWindow, WebviewWindowBuilder, Wry};

pub(crate) fn label_hash(key: &str) -> String {
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for byte in key.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01B3);
    }
    format!("{hash:016x}")
}

/// `ZEABUR_DESKTOP_THEME=light|dark` fixes the window's theme; otherwise it follows the system.
pub(crate) fn apply_theme<M: tauri::Manager<Wry>>(
    builder: WebviewWindowBuilder<'_, Wry, M>,
) -> WebviewWindowBuilder<'_, Wry, M> {
    match std::env::var("ZEABUR_DESKTOP_THEME").as_deref() {
        Ok("dark") => builder.theme(Some(tauri::Theme::Dark)),
        Ok("light") => builder.theme(Some(tauri::Theme::Light)),
        _ => builder,
    }
}

/// Whether a window of this kind (`main` or `cluster`) is the one to capture.
pub(crate) fn wants(kind: &str) -> bool {
    std::env::var_os("ZEABUR_DESKTOP_SHOT").is_some()
        && std::env::var("ZEABUR_DESKTOP_SHOT_TARGET")
            .as_deref()
            .unwrap_or("main")
            == kind
}

/// With `ZEABUR_DESKTOP_SHOT=<file.png>`, saves a PNG of the web view after
/// `ZEABUR_DESKTOP_SHOT_DELAY` milliseconds (3000) and quits.
#[cfg(target_os = "macos")]
pub(crate) fn capture(window: WebviewWindow) {
    use tauri::Manager;

    use crate::output;

    let Some(path) = std::env::var_os("ZEABUR_DESKTOP_SHOT") else {
        return;
    };
    let delay = std::env::var("ZEABUR_DESKTOP_SHOT_DELAY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000);
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(delay));
        let app = window.app_handle().clone();
        let taken = window.with_webview(move |webview| {
            crate::webkit::snapshot_png(&webview, move |png| {
                match png {
                    Some(png) => {
                        if let Err(err) = std::fs::write(&path, png) {
                            output::warn(&format!("cannot write the screenshot: {err}"));
                        }
                    }
                    None => output::warn("WebKit returned no snapshot"),
                }
                app.exit(0);
            });
        });
        if let Err(err) = taken {
            output::warn(&format!("cannot take the screenshot: {err}"));
        }
    });
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn capture(_window: WebviewWindow) {}
