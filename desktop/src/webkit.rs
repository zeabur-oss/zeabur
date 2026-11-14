//! The WebKit calls of the README screenshots. This is the only macOS code that uses `unsafe`.
#![expect(
    unsafe_code,
    reason = "objc2-web-kit has no safe API for a snapshot of the web view"
)]

use block2::RcBlock;
use objc2::rc::Retained;
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
use objc2_foundation::{NSData, NSDictionary, NSError};
use objc2_web_kit::WKWebView;
use tauri::webview::PlatformWebview;

/// Takes a snapshot of the web view and calls `done` on the main thread with the PNG, or `None`
/// when WebKit gives no image.
pub(crate) fn snapshot_png(webview: &PlatformWebview, done: impl Fn(Option<Vec<u8>>) + 'static) {
    let block = RcBlock::new(move |image: *mut NSImage, _error: *mut NSError| {
        // SAFETY: WebKit gives a valid image or null.
        let image = unsafe { image.as_ref() };
        done(image.and_then(png).map(|data| data.to_vec()));
    });
    // SAFETY: on macOS the platform web view is a WKWebView that the window owns.
    let view = unsafe { &*webview.inner().cast::<WKWebView>() };
    // SAFETY: no configuration takes the whole visible view, and WebKit keeps the block until it
    // calls it.
    unsafe { view.takeSnapshotWithConfiguration_completionHandler(None, &block) };
}

fn png(image: &NSImage) -> Option<Retained<NSData>> {
    let tiff = image.TIFFRepresentation()?;
    let rep = NSBitmapImageRep::imageRepWithData(&tiff)?;
    // SAFETY: an empty property dictionary is valid for PNG.
    unsafe {
        rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }
}
