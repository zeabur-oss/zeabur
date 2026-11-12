//! Hooks for the README screenshots: a fixed theme, a scenario for the page, and a capture of the
//! web view.

use tauri::{WebviewWindow, WebviewWindowBuilder, Wry};

pub fn label_hash(key: &str) -> String {
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for byte in key.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01B3);
    }
    format!("{hash:016x}")
}

/// `ZEABUR_DESKTOP_THEME=light|dark` fixes the window's appearance; otherwise it follows the
/// system.
pub fn apply_theme<'a, M: tauri::Manager<Wry>>(
    builder: WebviewWindowBuilder<'a, Wry, M>,
) -> WebviewWindowBuilder<'a, Wry, M> {
    match std::env::var("ZEABUR_DESKTOP_THEME").as_deref() {
        Ok("dark") => builder.theme(Some(tauri::Theme::Dark)),
        Ok("light") => builder.theme(Some(tauri::Theme::Light)),
        _ => builder,
    }
}

/// Whether a window of this kind (`main` or `cluster`) is the one to capture.
pub fn wants(kind: &str) -> bool {
    std::env::var_os("ZEABUR_DESKTOP_SHOT").is_some()
        && std::env::var("ZEABUR_DESKTOP_SHOT_TARGET")
            .as_deref()
            .unwrap_or("main")
            == kind
}

/// With `ZEABUR_DESKTOP_SHOT=<file.png>`, waits `ZEABUR_DESKTOP_SHOT_DELAY` milliseconds (3000),
/// writes a PNG of the window's web view and quits.
#[cfg(target_os = "macos")]
pub fn capture(window: WebviewWindow) {
    use block2::RcBlock;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
    use objc2_foundation::{NSDictionary, NSError};
    use objc2_web_kit::WKWebView;

    let Some(path) = std::env::var_os("ZEABUR_DESKTOP_SHOT") else {
        return;
    };
    let delay = std::env::var("ZEABUR_DESKTOP_SHOT_DELAY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000);
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(delay));
        let _ = window.with_webview(move |webview| {
            let done = RcBlock::new(move |image: *mut NSImage, _error: *mut NSError| {
                // SAFETY: WebKit passes a valid image or null.
                let png = unsafe { image.as_ref() }.and_then(|image| {
                    let tiff = image.TIFFRepresentation()?;
                    let rep = NSBitmapImageRep::imageRepWithData(&tiff)?;
                    // SAFETY: an empty property dictionary is valid for PNG.
                    unsafe {
                        rep.representationUsingType_properties(
                            NSBitmapImageFileType::PNG,
                            &NSDictionary::new(),
                        )
                    }
                });
                match png {
                    Some(data) => {
                        if let Err(err) = std::fs::write(&path, data.to_vec()) {
                            eprintln!("zeabur-desktop: cannot write the screenshot: {err}");
                        }
                    }
                    None => eprintln!("zeabur-desktop: WebKit returned no snapshot"),
                }
                std::process::exit(0);
            });
            // SAFETY: on macOS the platform web view is a WKWebView owned by the window.
            let view = unsafe { &*webview.inner().cast::<WKWebView>() };
            unsafe { view.takeSnapshotWithConfiguration_completionHandler(None, &done) };
        });
    });
}

#[cfg(not(target_os = "macos"))]
pub fn capture(_window: WebviewWindow) {}
