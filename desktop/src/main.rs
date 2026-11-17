//! Zeabur Desktop. It lists every context of every kubeconfig source and opens Zeabur for the ones
//! the person picks, each in its own window with its own local service (`cluster.rs`).

// A Windows release build is a GUI program, with no console window beside it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod chrome;
mod cluster;
mod env;
#[cfg(any(target_os = "linux", windows, test))]
mod keys;
mod menu;
mod output;
mod sources;
mod update;
#[cfg(windows)]
mod win32;

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Mutex, PoisonError, RwLock};

use env::{ChildEnv, ShellRead};
use serde::{Deserialize, Serialize};
use sources::{ContextView, SourceKind, SourceRef, SourceView};
use tauri::{Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

/// What the app remembers between launches, in `settings.json`. Kubeconfig files are read in
/// place and never written.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Saved {
    /// Files the person added.
    added: Vec<PathBuf>,
    /// Default paths the person removed from the list.
    hidden_defaults: Vec<PathBuf>,
}

struct App {
    config_dir: PathBuf,
    saved: Mutex<Saved>,
    /// The app's own environment until the login shell's has been read, then the merged one.
    env: RwLock<ChildEnv>,
    shell: Mutex<ShellRead>,
    clusters: cluster::Clusters,
}

impl App {
    /// Changes the saved settings and writes them to `settings.json`.
    #[expect(
        clippy::significant_drop_tightening,
        reason = "the lock is held until the file is written, so two changes cannot write out of order"
    )]
    fn update(&self, change: impl FnOnce(&mut Saved)) -> Result<(), String> {
        let mut saved = self.saved.lock().unwrap_or_else(PoisonError::into_inner);
        change(&mut saved);
        std::fs::create_dir_all(&self.config_dir).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(&*saved).map_err(|e| e.to_string())?;
        std::fs::write(self.config_dir.join("settings.json"), text).map_err(|e| e.to_string())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    sources: Vec<SourceView>,
    contexts: Vec<ContextView>,
    shell: ShellRead,
}

fn source_list(saved: &Saved, env: &ChildEnv) -> Vec<(String, SourceKind, PathBuf)> {
    let mut list = Vec::new();
    let kubeconfig = env.get(OsStr::new("KUBECONFIG")).map(OsString::from);
    let defaults = sources::default_paths(kubeconfig, std::env::home_dir());
    list.extend(
        defaults
            .into_iter()
            .filter(|path| !saved.hidden_defaults.contains(path) && path.exists())
            .map(|path| {
                (
                    format!("default:{}", path.display()),
                    SourceKind::Default,
                    path,
                )
            }),
    );
    list.extend(saved.added.iter().map(|path| {
        (
            format!("added:{}", path.display()),
            SourceKind::File,
            path.clone(),
        )
    }));
    list
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri passes a command's arguments by value"
)]
fn snapshot(app: State<'_, App>) -> Snapshot {
    read_snapshot(&app)
}

fn read_snapshot(app: &App) -> Snapshot {
    let env = app
        .env
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let list = source_list(
        &app.saved.lock().unwrap_or_else(PoisonError::into_inner),
        &env,
    );
    let refs: Vec<_> = list
        .iter()
        .map(|(id, kind, path)| SourceRef {
            id: id.clone(),
            kind: *kind,
            path,
        })
        .collect();
    let (sources, contexts) = sources::read_all(&refs, &env);
    Snapshot {
        sources,
        contexts,
        shell: app
            .shell
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone(),
    }
}

/// Adds kubeconfig files from the file picker.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri passes a command's arguments by value"
)]
fn add_paths(app: State<'_, App>, paths: Vec<PathBuf>) -> Result<(), String> {
    app.update(|saved| {
        for path in paths {
            saved.hidden_defaults.retain(|p| p != &path);
            if !saved.added.contains(&path) {
                saved.added.push(path);
            }
        }
    })
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri passes a command's arguments by value"
)]
fn remove_source(app: State<'_, App>, id: String) -> Result<(), String> {
    app.update(|saved| {
        if let Some(path) = id.strip_prefix("default:") {
            saved.hidden_defaults.push(PathBuf::from(path));
        } else if let Some(path) = id.strip_prefix("added:") {
            let path = PathBuf::from(path);
            saved.added.retain(|added| added != &path);
        }
    })
}

/// Opens a window for one context. The window starts the context's local service from
/// `cluster.html` (`start_cluster`). Async, because building a window in a synchronous command
/// deadlocks on Windows (`WebviewWindowBuilder::build`).
#[tauri::command]
async fn open_cluster(
    handle: tauri::AppHandle,
    app: State<'_, App>,
    key: String,
    name: String,
    lang: String,
) -> Result<(), String> {
    let label = format!("cluster-{}", label_hash(&key));
    if let Some(window) = handle.get_webview_window(&label) {
        return window.set_focus().map_err(|e| e.to_string());
    }
    let context = read_snapshot(&app)
        .contexts
        .into_iter()
        .find(|c| c.key == key)
        .ok_or_else(|| format!("{name} is no longer in the kubeconfig"))?;
    let mut page = "tauri://localhost/cluster.html"
        .parse::<tauri::Url>()
        .map_err(|e| e.to_string())?;
    page.query_pairs_mut().append_pair("name", &name);
    let url = WebviewUrl::App(format!("cluster.html?{}", page.query().unwrap_or_default()).into());
    app.clusters.set_target(
        &label,
        cluster::Target {
            file: PathBuf::from(&context.file),
            context: context.name,
        },
    );
    // Without a matching locale the web interface follows the system languages.
    let locale = if ["en", "zh-TW", "zh-CN"].contains(&lang.as_str()) {
        format!("localStorage.setItem('zeabur.locale', '{lang}')")
    } else {
        "localStorage.removeItem('zeabur.locale')".to_owned()
    };
    let script = format!("try {{ {locale} }} catch {{}}");
    let builder = WebviewWindowBuilder::new(&handle, label, url)
        .title(format!("{name} - Zeabur"))
        .initialization_script(&script);
    let builder = chrome::cluster_window(&handle, builder);
    let window = builder.build().map_err(|e| e.to_string())?;
    chrome::window_built(&window);
    Ok(())
}

/// Starts the local service of the calling cluster window and loads its link in that window.
#[tauri::command]
async fn start_cluster(window: tauri::WebviewWindow, app: State<'_, App>) -> Result<(), String> {
    let env = app
        .env
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let link = app.clusters.start(window.label(), &env).await?;
    let url = link.parse::<tauri::Url>().map_err(|e| e.to_string())?;
    window.navigate(url).map_err(|e| e.to_string())
}

/// Closes the calling cluster window and shows the chooser. Async, because it can build the
/// chooser window.
#[tauri::command]
async fn back_to_chooser(window: tauri::WebviewWindow) -> Result<(), String> {
    chrome::show_chooser(window.app_handle()).map_err(|e| e.to_string())?;
    window.close().map_err(|e| e.to_string())
}

/// The app's config, icons and assets, which `build.rs` generates with `tauri-build`.
#[expect(
    clippy::default_trait_access,
    clippy::exit,
    clippy::expect_used,
    clippy::manual_string_new,
    clippy::option_if_let_else,
    clippy::print_stderr,
    clippy::single_match_else,
    clippy::str_to_string,
    clippy::unwrap_used,
    reason = "tauri-build generates this code"
)]
#[cfg_attr(
    not(windows),
    expect(
        clippy::unseparated_literal_suffix,
        reason = "tauri-build generates this code"
    )
)]
fn context() -> tauri::Context {
    tauri::tauri_build_context!()
}

/// A window label's part for a context key: the same key always gives the same label.
fn label_hash(key: &str) -> String {
    let hash = key.bytes().fold(0xCBF2_9CE4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01B3)
    });
    format!("{hash:016x}")
}

fn setup(tauri_app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let config_dir = tauri_app.path().app_config_dir()?;
    let saved = std::fs::read(config_dir.join("settings.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    tauri_app.manage(App {
        config_dir,
        saved: Mutex::new(saved),
        env: RwLock::new(ChildEnv::from_process()),
        shell: Mutex::new(ShellRead::Pending),
        clusters: cluster::Clusters::default(),
    });

    // A login shell can take seconds; the window opens without waiting and reloads on the
    // event.
    let handle = tauri_app.handle().clone();
    std::thread::spawn(move || {
        let (merged, read) = env::load(ChildEnv::from_process());
        let app = handle.state::<App>();
        *app.env.write().unwrap_or_else(PoisonError::into_inner) = merged;
        *app.shell.lock().unwrap_or_else(PoisonError::into_inner) = read;
        if let Err(err) = handle.emit("shell-env", ()) {
            output::warn(&format!("cannot reload the window: {err}"));
        }
    });

    #[cfg(target_os = "macos")]
    chrome::allow_title_bar(tauri_app.handle())?;
    chrome::show_chooser(tauri_app.handle())?;
    update::check_on_start(tauri_app.handle());
    Ok(())
}

fn main() -> ExitCode {
    let context = context();
    #[cfg(windows)]
    chrome::set_app_id(&context.config().identifier);
    let builder = tauri::Builder::default();
    // It comes first, as the plugin requires.
    #[cfg(any(windows, target_os = "linux"))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        if let Err(err) = chrome::show_chooser(app) {
            output::warn(&format!("cannot open the chooser: {err}"));
        }
    }));
    let builder = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(update::Updates::default());
    #[cfg(target_os = "macos")]
    let builder = builder.on_menu_event(menu::on_menu_event);
    let app = builder
        .setup(setup)
        .invoke_handler(tauri::generate_handler![
            snapshot,
            add_paths,
            remove_source,
            open_cluster,
            start_cluster,
            back_to_chooser,
            menu::set_menu,
            update::found_update,
            update::take_check_request,
            update::check_update,
            update::install_update,
            update::open_releases
        ])
        .build(context);
    match app {
        Ok(app) => {
            app.run(|handle, event| {
                if let tauri::RunEvent::WindowEvent {
                    label,
                    event: tauri::WindowEvent::Destroyed,
                    ..
                } = &event
                {
                    handle.state::<App>().clusters.closed(label);
                }
                chrome::on_run_event(handle, event);
            });
            ExitCode::SUCCESS
        }
        Err(err) => {
            output::warn(&err.to_string());
            ExitCode::FAILURE
        }
    }
}
