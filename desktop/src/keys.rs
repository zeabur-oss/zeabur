//! Keyboard shortcuts on Linux. GNOME apps have no menu bar to carry them, so the window handles
//! the keys before the web view does. macOS gets the same shortcuts from its menu bar (`menu.rs`).
//!
//! The keys are the GNOME HIG's standard shortcuts
//! (<https://developer.gnome.org/hig/reference/keyboard.html>): Ctrl+W closes the window, Ctrl+Q
//! closes the app with all its windows, Ctrl+N creates a new item (here the chooser), Ctrl+R
//! reloads, and Ctrl++, Ctrl+- and Ctrl+0 zoom.

/// What a shortcut does.
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

#[cfg(test)]
mod tests {
    use super::{Shortcut, shortcut};

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
}
