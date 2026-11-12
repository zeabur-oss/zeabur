//! Keyboard shortcuts on Linux and Windows. Neither has a menu bar here to carry them, so the
//! window handles the keys before the page sees them. macOS gets the same shortcuts from its menu
//! bar (`menu.rs`).
//!
//! Linux: the GNOME HIG's standard shortcuts
//! (<https://developer.gnome.org/hig/reference/keyboard.html>): Ctrl+W closes the window, Ctrl+Q
//! closes the app with all its windows, Ctrl+N creates a new item (here the chooser), Ctrl+R
//! reloads, and Ctrl++, Ctrl+- and Ctrl+0 zoom.
//!
//! Windows: the common keyboard accelerators
//! (<https://learn.microsoft.com/en-us/windows/apps/develop/input/keyboard-accelerators>) Ctrl+W
//! (close) and Ctrl+N (new item, here the chooser). WebView2 already handles Ctrl+R and F5 and the
//! zoom keys, and Alt+F4 is the system's.

/// What a shortcut does.
#[cfg_attr(
    all(windows, not(test)),
    expect(dead_code, reason = "Windows uses Close and NewWindow only")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    Close,
    Quit,
    NewWindow,
    Reload,
    ZoomIn,
    ZoomOut,
    ActualSize,
}

/// The shortcut for a key, by its GDK key name in lower case, pressed with Ctrl. Shift is allowed
/// only where it types the key (`+` is Shift+`=` on many layouts); Alt is never part of one.
#[cfg(any(target_os = "linux", test))]
pub fn shortcut(ctrl: bool, shift: bool, alt: bool, key: &str) -> Option<Shortcut> {
    if !ctrl || alt {
        return None;
    }
    let found = match key {
        "plus" | "kp_add" => return Some(Shortcut::ZoomIn),
        "w" => Shortcut::Close,
        "q" => Shortcut::Quit,
        "n" => Shortcut::NewWindow,
        "r" => Shortcut::Reload,
        "equal" => Shortcut::ZoomIn,
        "minus" | "kp_subtract" => Shortcut::ZoomOut,
        "0" | "kp_0" => Shortcut::ActualSize,
        _ => return None,
    };
    if shift { None } else { Some(found) }
}

/// The shortcut for a Windows virtual-key code pressed with Ctrl alone. Only the keys WebView2 does
/// not handle itself.
#[cfg(any(windows, test))]
pub fn windows_shortcut(ctrl: bool, shift: bool, alt: bool, key: u32) -> Option<Shortcut> {
    if !ctrl || shift || alt {
        return None;
    }
    match char::from_u32(key) {
        Some('W') => Some(Shortcut::Close),
        Some('N') => Some(Shortcut::NewWindow),
        _ => None,
    }
}

/// Handles the shortcuts in this window, before the page sees the keys.
#[cfg(target_os = "linux")]
pub fn attach(window: &tauri::WebviewWindow) {
    use gtk::prelude::*;

    let Ok(gtk_window) = window.gtk_window() else {
        return;
    };
    let window = window.clone();
    gtk_window.connect_key_press_event(move |_, event| {
        let state = event.state();
        let name = event.keyval().to_lower().name();
        let found = shortcut(
            state.contains(gtk::gdk::ModifierType::CONTROL_MASK),
            state.contains(gtk::gdk::ModifierType::SHIFT_MASK),
            state.contains(gtk::gdk::ModifierType::MOD1_MASK),
            &name.map(|n| n.to_lowercase()).unwrap_or_default(),
        );
        match found {
            Some(found) => {
                run(&window, found);
                gtk::glib::Propagation::Stop
            }
            None => gtk::glib::Propagation::Proceed,
        }
    });
}

#[cfg(target_os = "linux")]
fn run(window: &tauri::WebviewWindow, shortcut: Shortcut) {
    use tauri::Manager;

    use crate::chrome;

    match shortcut {
        Shortcut::Close => {
            let _ = window.close();
        }
        Shortcut::Quit => window.app_handle().exit(0),
        Shortcut::NewWindow => {
            if let Err(err) = chrome::show_chooser(window.app_handle()) {
                eprintln!("zeabur-desktop: cannot open the chooser: {err}");
            }
        }
        Shortcut::Reload => {
            let _ = window.reload();
        }
        Shortcut::ZoomIn => chrome::zoom(window, Some(true)),
        Shortcut::ZoomOut => chrome::zoom(window, Some(false)),
        Shortcut::ActualSize => chrome::zoom(window, None),
    }
}

/// Handles Ctrl+W and Ctrl+N in this window through WebView2's `AcceleratorKeyPressed`, which runs
/// before the page sees a key pressed with Ctrl or Alt
/// (<https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2controller#add_acceleratorkeypressed>).
#[cfg(windows)]
pub fn attach(window: &tauri::WebviewWindow) {
    use webview2_com::AcceleratorKeyPressedEventHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_KEY_EVENT_KIND, COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN,
        COREWEBVIEW2_PHYSICAL_KEY_STATUS,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, VIRTUAL_KEY, VK_CONTROL, VK_MENU, VK_SHIFT,
    };

    let target = window.clone();
    let _ = window.with_webview(move |webview| {
        let handler = AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else {
                return Ok(());
            };
            let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
            let mut key = 0;
            let mut status = COREWEBVIEW2_PHYSICAL_KEY_STATUS::default();
            // SAFETY: plain out-parameters of the event's arguments, valid during the call.
            unsafe {
                args.KeyEventKind(&mut kind)?;
                args.VirtualKey(&mut key)?;
                args.PhysicalKeyStatus(&mut status)?;
            }
            if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN {
                return Ok(());
            }
            // SAFETY: GetKeyState only reads this thread's keyboard state.
            let held = |vk: VIRTUAL_KEY| unsafe { GetKeyState(i32::from(vk.0)) } < 0;
            let Some(found) =
                windows_shortcut(held(VK_CONTROL), held(VK_SHIFT), held(VK_MENU), key)
            else {
                return Ok(());
            };
            // SAFETY: as above.
            unsafe { args.SetHandled(true)? };
            // A held key repeats; act on the first press only. Closing or opening a window inside
            // a WebView2 event handler deadlocks, so it happens on another thread.
            if !status.WasKeyDown.as_bool() {
                let window = target.clone();
                std::thread::spawn(move || run_windows(&window, found));
            }
            Ok(())
        }));
        let mut token = 0;
        // SAFETY: the controller lives as long as the window, and the handler is reference counted.
        if let Err(err) = unsafe {
            webview
                .controller()
                .add_AcceleratorKeyPressed(&handler, &mut token)
        } {
            eprintln!("zeabur-desktop: cannot handle shortcuts: {err}");
        }
    });
}

#[cfg(windows)]
fn run_windows(window: &tauri::WebviewWindow, shortcut: Shortcut) {
    use tauri::Manager;

    match shortcut {
        Shortcut::Close => {
            let _ = window.close();
        }
        Shortcut::NewWindow => {
            if let Err(err) = crate::chrome::show_chooser(window.app_handle()) {
                eprintln!("zeabur-desktop: cannot open the chooser: {err}");
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{Shortcut, shortcut, windows_shortcut};

    #[test]
    fn ctrl_keys_map_to_the_gnome_shortcuts() {
        assert_eq!(shortcut(true, false, false, "w"), Some(Shortcut::Close));
        assert_eq!(shortcut(true, false, false, "q"), Some(Shortcut::Quit));
        assert_eq!(shortcut(true, false, false, "n"), Some(Shortcut::NewWindow));
        assert_eq!(shortcut(true, false, false, "r"), Some(Shortcut::Reload));
        assert_eq!(
            shortcut(true, false, false, "minus"),
            Some(Shortcut::ZoomOut)
        );
        assert_eq!(
            shortcut(true, false, false, "kp_0"),
            Some(Shortcut::ActualSize)
        );
    }

    #[test]
    fn zoom_in_takes_plus_with_or_without_shift() {
        assert_eq!(
            shortcut(true, false, false, "equal"),
            Some(Shortcut::ZoomIn)
        );
        assert_eq!(shortcut(true, true, false, "plus"), Some(Shortcut::ZoomIn));
        assert_eq!(
            shortcut(true, false, false, "kp_add"),
            Some(Shortcut::ZoomIn)
        );
    }

    #[test]
    fn other_modifiers_and_keys_reach_the_page() {
        assert_eq!(shortcut(false, false, false, "w"), None);
        assert_eq!(shortcut(true, true, false, "w"), None);
        assert_eq!(shortcut(true, false, true, "w"), None);
        assert_eq!(shortcut(true, false, false, "k"), None);
    }

    #[test]
    fn windows_handles_ctrl_w_and_ctrl_n_and_leaves_the_rest_to_webview2() {
        assert_eq!(
            windows_shortcut(true, false, false, u32::from(b'W')),
            Some(Shortcut::Close)
        );
        assert_eq!(
            windows_shortcut(true, false, false, u32::from(b'N')),
            Some(Shortcut::NewWindow)
        );
        assert_eq!(windows_shortcut(true, false, false, u32::from(b'R')), None);
        assert_eq!(windows_shortcut(true, true, false, u32::from(b'W')), None);
        assert_eq!(windows_shortcut(true, false, true, u32::from(b'W')), None);
        assert_eq!(windows_shortcut(false, false, false, u32::from(b'W')), None);
    }
}
