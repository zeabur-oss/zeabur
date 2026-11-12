//! The application menu. Its words come from the web interface's catalogue: the chooser page sends
//! them after every language change, because native menus cannot read the TypeScript catalogue.

use serde::Deserialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Wry};

pub const CLUSTERS_ID: &str = "clusters";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Labels {
    pub clusters: String,
    pub edit: String,
    pub undo: String,
    pub redo: String,
    pub cut: String,
    pub copy: String,
    pub paste: String,
    pub select_all: String,
    pub window: String,
    pub minimize: String,
    pub close: String,
    pub hide: String,
    pub hide_others: String,
    pub show_all: String,
    pub quit: String,
}

impl Default for Labels {
    /// English, used until the chooser page has sent the words of the app language.
    fn default() -> Self {
        let s = |text: &str| text.to_owned();
        Labels {
            clusters: s("Clusters"),
            edit: s("Edit"),
            undo: s("Undo"),
            redo: s("Redo"),
            cut: s("Cut"),
            copy: s("Copy"),
            paste: s("Paste"),
            select_all: s("Select All"),
            window: s("Window"),
            minimize: s("Minimize"),
            close: s("Close Window"),
            hide: s("Hide Zeabur"),
            hide_others: s("Hide Others"),
            show_all: s("Show All"),
            quit: s("Quit Zeabur"),
        }
    }
}

/// What the Clusters item does to the chooser window.
#[derive(Debug, PartialEq, Eq)]
pub enum ChooserAction {
    Create,
    Show { unminimize: bool },
}

/// `window` is the chooser's state, or `None` when it was closed.
pub fn chooser_action(window: Option<ChooserState>) -> ChooserAction {
    match window {
        None => ChooserAction::Create,
        Some(state) => ChooserAction::Show { unminimize: state.minimized },
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ChooserState {
    pub minimized: bool,
}

pub fn build(app: &AppHandle, labels: &Labels) -> tauri::Result<Menu<Wry>> {
    let clusters = MenuItem::with_id(app, CLUSTERS_ID, &labels.clusters, true, Some("CmdOrCtrl+Shift+L"))?;
    let edit = Submenu::with_items(
        app,
        &labels.edit,
        true,
        &[
            &PredefinedMenuItem::undo(app, Some(&labels.undo))?,
            &PredefinedMenuItem::redo(app, Some(&labels.redo))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, Some(&labels.cut))?,
            &PredefinedMenuItem::copy(app, Some(&labels.copy))?,
            &PredefinedMenuItem::paste(app, Some(&labels.paste))?,
            &PredefinedMenuItem::select_all(app, Some(&labels.select_all))?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        &labels.window,
        true,
        &[
            &clusters,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::minimize(app, Some(&labels.minimize))?,
            &PredefinedMenuItem::close_window(app, Some(&labels.close))?,
        ],
    )?;
    let menu = Menu::new(app)?;
    if cfg!(target_os = "macos") {
        let application = Submenu::with_items(
            app,
            "Zeabur",
            true,
            &[
                &PredefinedMenuItem::hide(app, Some(&labels.hide))?,
                &PredefinedMenuItem::hide_others(app, Some(&labels.hide_others))?,
                &PredefinedMenuItem::show_all(app, Some(&labels.show_all))?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::quit(app, Some(&labels.quit))?,
            ],
        )?;
        menu.append(&application)?;
    }
    menu.append(&edit)?;
    menu.append(&window)?;
    Ok(menu)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_chooser_is_created_again() {
        assert_eq!(chooser_action(None), ChooserAction::Create);
    }

    #[test]
    fn an_open_chooser_is_shown_and_a_minimized_one_is_restored() {
        assert_eq!(chooser_action(Some(ChooserState { minimized: false })), ChooserAction::Show { unminimize: false });
        assert_eq!(chooser_action(Some(ChooserState { minimized: true })), ChooserAction::Show { unminimize: true });
    }
}
