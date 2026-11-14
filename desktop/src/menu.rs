//! The macOS menu bar, in the chooser page's language.

#[cfg(target_os = "macos")]
pub(crate) use mac::on_menu_event;

/// Sets the menu bar in the chooser's language; `docs` is the docs page in that language.
#[cfg(target_os = "macos")]
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri passes a command's arguments by value"
)]
pub(crate) fn set_menu(
    app: tauri::AppHandle,
    labels: mac::MenuLabels,
    docs: String,
) -> Result<(), String> {
    mac::set(&app, &labels).map_err(|e| e.to_string())?;
    *mac::DOCS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = docs;
    Ok(())
}

/// Windows and Linux have no menu bar; the page calls this all the same.
#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub(crate) const fn set_menu() {}

#[cfg(target_os = "macos")]
mod mac {
    use std::sync::{Mutex, PoisonError};

    use serde::Deserialize;
    use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu, SubmenuBuilder};
    use tauri::{AppHandle, Manager, WebviewWindow};

    use crate::{chrome, output};

    /// Every word in the menu bar (`desktop.menu.*` in `web/src/i18n`).
    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct MenuLabels {
        about: String,
        services: String,
        hide: String,
        hide_others: String,
        show_all: String,
        quit: String,
        file: String,
        new_window: String,
        close: String,
        edit: String,
        undo: String,
        redo: String,
        cut: String,
        copy: String,
        paste: String,
        select_all: String,
        view: String,
        reload: String,
        actual_size: String,
        zoom_in: String,
        zoom_out: String,
        full_screen: String,
        window: String,
        minimize: String,
        zoom: String,
        bring_all_to_front: String,
        help: String,
        docs: String,
    }

    pub(crate) static DOCS: Mutex<String> = Mutex::new(String::new());

    pub(crate) fn set(app: &AppHandle, l: &MenuLabels) -> tauri::Result<()> {
        let info = app.package_info();
        let about = AboutMetadata {
            name: Some(info.name.clone()),
            version: Some(info.version.to_string()),
            icon: app.default_window_icon().cloned(),
            ..Default::default()
        };
        let app_menu = SubmenuBuilder::new(app, &info.name)
            .item(&PredefinedMenuItem::about(
                app,
                Some(&l.about),
                Some(about),
            )?)
            .separator()
            .item(&PredefinedMenuItem::services(app, Some(&l.services))?)
            .separator()
            .item(&PredefinedMenuItem::hide(app, Some(&l.hide))?)
            .item(&PredefinedMenuItem::hide_others(app, Some(&l.hide_others))?)
            .item(&PredefinedMenuItem::show_all(app, Some(&l.show_all))?)
            .separator()
            .item(&PredefinedMenuItem::quit(app, Some(&l.quit))?)
            .build()?;
        let file = SubmenuBuilder::new(app, &l.file)
            .item(&MenuItem::with_id(
                app,
                "new-window",
                &l.new_window,
                true,
                Some("CmdOrCtrl+N"),
            )?)
            .separator()
            .item(&PredefinedMenuItem::close_window(app, Some(&l.close))?)
            .build()?;
        let edit = SubmenuBuilder::new(app, &l.edit)
            .item(&PredefinedMenuItem::undo(app, Some(&l.undo))?)
            .item(&PredefinedMenuItem::redo(app, Some(&l.redo))?)
            .separator()
            .item(&PredefinedMenuItem::cut(app, Some(&l.cut))?)
            .item(&PredefinedMenuItem::copy(app, Some(&l.copy))?)
            .item(&PredefinedMenuItem::paste(app, Some(&l.paste))?)
            .item(&PredefinedMenuItem::select_all(app, Some(&l.select_all))?)
            .build()?;
        let view = SubmenuBuilder::new(app, &l.view)
            .item(&MenuItem::with_id(
                app,
                "reload",
                &l.reload,
                true,
                Some("CmdOrCtrl+R"),
            )?)
            .separator()
            .item(&MenuItem::with_id(
                app,
                "actual-size",
                &l.actual_size,
                true,
                Some("CmdOrCtrl+0"),
            )?)
            .item(&MenuItem::with_id(
                app,
                "zoom-in",
                &l.zoom_in,
                true,
                Some("CmdOrCtrl+="),
            )?)
            .item(&MenuItem::with_id(
                app,
                "zoom-out",
                &l.zoom_out,
                true,
                Some("CmdOrCtrl+-"),
            )?)
            .separator()
            .item(&PredefinedMenuItem::fullscreen(app, Some(&l.full_screen))?)
            .build()?;
        let window = SubmenuBuilder::new(app, &l.window)
            .item(&PredefinedMenuItem::minimize(app, Some(&l.minimize))?)
            .item(&PredefinedMenuItem::maximize(app, Some(&l.zoom))?)
            .separator()
            .item(&PredefinedMenuItem::bring_all_to_front(
                app,
                Some(&l.bring_all_to_front),
            )?)
            .build()?;
        let help = Submenu::with_items(
            app,
            &l.help,
            true,
            &[&MenuItem::with_id(
                app,
                "docs",
                &l.docs,
                true,
                None::<&str>,
            )?],
        )?;
        let menu = Menu::with_items(app, &[&app_menu, &file, &edit, &view, &window, &help])?;
        app.set_menu(menu)?;
        // macOS lists the open windows under Window and puts its search field under Help, once the
        // menu is the app's.
        window.set_as_windows_menu_for_nsapp()?;
        help.set_as_help_menu_for_nsapp()
    }

    /// Handles the menu items the app defines; macOS handles the standard ones.
    #[expect(clippy::needless_pass_by_value, reason = "the signature Tauri calls")]
    pub(crate) fn on_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
        let id = event.id().as_ref();
        match id {
            "new-window" => {
                if let Err(err) = chrome::show_chooser(app) {
                    output::warn(&format!("cannot open the chooser: {err}"));
                }
            }
            "docs" => {
                let url = DOCS.lock().unwrap_or_else(PoisonError::into_inner).clone();
                if url.starts_with("https://oss.zeabur.com/")
                    && let Err(err) = std::process::Command::new("/usr/bin/open").arg(url).spawn()
                {
                    output::warn(&format!("cannot open the docs: {err}"));
                }
            }
            "reload" => {
                if let Some(window) = focused(app)
                    && let Err(err) = window.reload()
                {
                    output::warn(&format!("cannot reload the window: {err}"));
                }
            }
            "actual-size" | "zoom-in" | "zoom-out" => {
                if let Some(window) = focused(app) {
                    let up = match id {
                        "zoom-in" => Some(true),
                        "zoom-out" => Some(false),
                        _ => None,
                    };
                    chrome::zoom(&window, up);
                }
            }
            _ => {}
        }
    }

    fn focused(app: &AppHandle) -> Option<WebviewWindow> {
        app.webview_windows()
            .into_values()
            .find(|window| window.is_focused().unwrap_or(false))
    }
}
