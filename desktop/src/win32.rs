//! The Win32 and WebView2 calls of the app, as safe functions. The `windows` and `webview2-com`
//! crates make every system call `unsafe`, so these are the only Windows calls that use `unsafe`.
#![expect(
    unsafe_code,
    reason = "the windows and webview2-com crates have no safe API for these calls"
)]

use tauri::webview::PlatformWebview;
use webview2_com::AcceleratorKeyPressedEventHandler;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_KEY_EVENT_KIND, COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN,
    COREWEBVIEW2_PHYSICAL_KEY_STATUS,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VIRTUAL_KEY};
use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
use windows::core::HSTRING;

/// Sets the process's AppUserModelID
/// (<https://learn.microsoft.com/en-us/windows/win32/shell/appids>).
pub(crate) fn set_app_user_model_id(id: &str) -> windows::core::Result<()> {
    // SAFETY: the string outlives the call, and the system copies it.
    unsafe { SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(id)) }
}

/// Whether the key is down for this thread's last keyboard message.
pub(crate) fn key_held(key: VIRTUAL_KEY) -> bool {
    // SAFETY: GetKeyState only reads this thread's keyboard state.
    unsafe { GetKeyState(i32::from(key.0)) < 0 }
}

/// Calls `on_key` with the virtual-key code, and whether the key repeats, for each key pressed
/// with Ctrl or Alt, before the page sees it. When `on_key` returns `true`, the page does not get
/// the key
/// (<https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2controller#add_acceleratorkeypressed>).
pub(crate) fn on_accelerator_key(
    webview: &PlatformWebview,
    on_key: impl Fn(u32, bool) -> bool + 'static,
) -> windows::core::Result<()> {
    let handler = AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else {
            return Ok(());
        };
        let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
        let mut key = 0;
        let mut status = COREWEBVIEW2_PHYSICAL_KEY_STATUS::default();
        // SAFETY: plain out-parameters of the event's arguments, valid during the call.
        unsafe {
            args.KeyEventKind(&raw mut kind)?;
            args.VirtualKey(&raw mut key)?;
            args.PhysicalKeyStatus(&raw mut status)?;
        }
        if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN {
            return Ok(());
        }
        if on_key(key, status.WasKeyDown.as_bool()) {
            // SAFETY: as above.
            unsafe { args.SetHandled(true) }?;
        }
        Ok(())
    }));
    let mut token = 0;
    // SAFETY: the controller lives as long as the window, and the handler is reference counted.
    unsafe {
        webview
            .controller()
            .add_AcceleratorKeyPressed(&handler, &raw mut token)
    }
}
