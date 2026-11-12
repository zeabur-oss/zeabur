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
pub(crate) enum Shortcut {
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
pub(crate) fn shortcut(ctrl: bool, shift: bool, alt: bool, key: &str) -> Option<Shortcut> {
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
pub(crate) const fn windows_shortcut(
    ctrl: bool,
    shift: bool,
    alt: bool,
    key: u32,
) -> Option<Shortcut> {
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
pub(crate) fn attach(window: &tauri::WebviewWindow) {
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

    use crate::{chrome, output};

    match shortcut {
        Shortcut::Close => {
            if let Err(err) = window.close() {
                output::warn(&format!("cannot close the window: {err}"));
            }
        }
        Shortcut::Quit => window.app_handle().exit(0),
        Shortcut::NewWindow => {
            if let Err(err) = chrome::show_chooser(window.app_handle()) {
                output::warn(&format!("cannot open the chooser: {err}"));
            }
        }
        Shortcut::Reload => {
            if let Err(err) = window.reload() {
                output::warn(&format!("cannot reload the window: {err}"));
            }
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
pub(crate) fn attach(window: &tauri::WebviewWindow) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_MENU, VK_SHIFT};

    use crate::{output, win32};

    let target = window.clone();
    let attached = window.with_webview(move |webview| {
        let result = win32::on_accelerator_key(&webview, move |key, repeat| {
            let held = win32::key_held;
            let Some(found) =
                windows_shortcut(held(VK_CONTROL), held(VK_SHIFT), held(VK_MENU), key)
            else {
                return false;
            };
            // A held key repeats; act on the first press only. Closing or opening a window inside
            // a WebView2 event handler deadlocks, so it happens on another thread.
            if !repeat {
                let window = target.clone();
                std::thread::spawn(move || run_windows(&window, found));
            }
            true
        });
        if let Err(err) = result {
            output::warn(&format!("cannot handle shortcuts: {err}"));
        }
    });
    if let Err(err) = attached {
        output::warn(&format!("cannot handle shortcuts: {err}"));
    }
}

#[cfg(windows)]
fn run_windows(window: &tauri::WebviewWindow, shortcut: Shortcut) {
    use tauri::Manager;

    use crate::{chrome, output};

    let result = match shortcut {
        Shortcut::Close => window.close(),
        Shortcut::NewWindow => chrome::show_chooser(window.app_handle()),
        Shortcut::Quit
        | Shortcut::Reload
        | Shortcut::ZoomIn
        | Shortcut::ZoomOut
        | Shortcut::ActualSize => Ok(()),
    };
    if let Err(err) = result {
        output::warn(&format!("cannot run the shortcut: {err}"));
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
