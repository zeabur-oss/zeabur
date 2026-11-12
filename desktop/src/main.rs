//! Proof of concept: the Zeabur desktop app. It lists every context of every kubeconfig source and
//! opens Zeabur's web interface for the ones the person picks.

mod env;
mod probe;
mod shots;
mod sources;
mod store;

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};

use env::{ChildEnv, ShellRead};
use serde::{Deserialize, Serialize};
use sources::{AddedSource, ContextView, SourceKind, SourceRef, SourceView};
use tauri::{Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

/// What the app remembers between launches, in `settings.json`. Added files and folders are stored
/// as absolute paths and read in place; the app never writes to a kubeconfig file.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Saved {
    added: Vec<AddedSource>,
    /// Default paths the person removed from the list.
    hidden_defaults: Vec<PathBuf>,
    favourites: Vec<String>,
    /// Most recent first, at most 5.
    recents: Vec<String>,
}

struct App {
    config_dir: PathBuf,
    kubeconfigs_dir: PathBuf,
    saved: Mutex<Saved>,
    /// The app's own environment until the login shell's has been read, then the merged one.
    env: RwLock<ChildEnv>,
    shell: Mutex<ShellRead>,
    web_url: String,
}

impl App {
    fn save(&self, saved: &Saved) -> Result<(), String> {
        std::fs::create_dir_all(&self.config_dir).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(saved).map_err(|e| e.to_string())?;
        std::fs::write(self.config_dir.join("settings.json"), text).map_err(|e| e.to_string())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    sources: Vec<SourceView>,
    contexts: Vec<ContextView>,
    favourites: Vec<String>,
    recents: Vec<String>,
    shell: ShellRead,
}

fn source_list(saved: &Saved, env: &ChildEnv) -> Vec<(String, SourceKind, PathBuf)> {
    let mut list = Vec::new();
    let kubeconfig = env.get(OsStr::new("KUBECONFIG")).map(OsString::from);
    let defaults = sources::default_paths(kubeconfig, std::env::home_dir());
    for path in defaults {
        if !saved.hidden_defaults.contains(&path) && path.exists() {
            list.push((
                format!("default:{}", path.display()),
                SourceKind::Default,
                path,
            ));
        }
    }
    for added in &saved.added {
        list.push((
            format!("added:{}", added.path.display()),
            added.kind,
            added.path.clone(),
        ));
    }
    list
}

#[tauri::command]
fn snapshot(app: State<'_, App>) -> Snapshot {
    let saved = app.saved.lock().unwrap_or_else(|e| e.into_inner());
    let env = app.env.read().unwrap_or_else(|e| e.into_inner());
    let list = source_list(&saved, &env);
    let mut refs = Vec::new();
    for (id, kind, path) in &list {
        refs.push(SourceRef {
            id: id.clone(),
            kind: *kind,
            path,
        });
    }
    let (sources, contexts) = sources::read_all(&refs, &env);
    Snapshot {
        sources,
        contexts,
        favourites: saved.favourites.clone(),
        recents: saved.recents.clone(),
        shell: app.shell.lock().unwrap_or_else(|e| e.into_inner()).clone(),
    }
}

/// Adds files and folders from the file picker or a drop.
/// The screenshot scenario for the page (`ZEABUR_DESKTOP_SCENARIO`), if any.
#[tauri::command]
fn scenario() -> Option<String> {
    std::env::var("ZEABUR_DESKTOP_SCENARIO").ok()
}

#[tauri::command]
fn add_paths(app: State<'_, App>, paths: Vec<PathBuf>) -> Result<(), String> {
    let mut saved = app.saved.lock().unwrap_or_else(|e| e.into_inner());
    for path in paths {
        let kind = if path.is_dir() {
            SourceKind::Folder
        } else {
            SourceKind::File
        };
        saved.hidden_defaults.retain(|p| p != &path);
        if !saved.added.iter().any(|a| a.path == path) {
            saved.added.push(AddedSource { kind, path });
        }
    }
    app.save(&saved)
}

/// Checks pasted YAML, then keeps a copy in the app's `kubeconfigs` folder.
#[tauri::command]
fn add_pasted(app: State<'_, App>, name: String, yaml: String) -> Result<(), String> {
    let config = kube::config::Kubeconfig::from_yaml(&yaml).map_err(|e| e.to_string())?;
    if config.contexts.is_empty() {
        return Err("no-contexts".into());
    }
    let mut slug = String::new();
    for c in name.trim().chars() {
        slug.push(if c.is_alphanumeric() || c == '-' {
            c
        } else {
            '-'
        });
    }
    if slug.is_empty() {
        slug.push_str("pasted");
    }
    let path = store::write_private(
        &app.kubeconfigs_dir,
        &format!("{slug}.yaml"),
        yaml.as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    let mut saved = app.saved.lock().unwrap_or_else(|e| e.into_inner());
    if !saved.added.iter().any(|a| a.path == path) {
        saved.added.push(AddedSource {
            kind: SourceKind::Pasted,
            path,
        });
    }
    app.save(&saved)
}

#[tauri::command]
fn remove_source(app: State<'_, App>, id: String) -> Result<(), String> {
    let mut saved = app.saved.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(path) = id.strip_prefix("default:") {
        saved.hidden_defaults.push(PathBuf::from(path));
    } else if let Some(path) = id.strip_prefix("added:") {
        let path = PathBuf::from(path);
        if let Some(index) = saved.added.iter().position(|a| a.path == path) {
            let removed = saved.added.remove(index);
            if removed.kind == SourceKind::Pasted {
                let _ = std::fs::remove_file(&removed.path);
            }
        }
    }
    app.save(&saved)
}

#[tauri::command]
fn toggle_favourite(app: State<'_, App>, key: String) -> Result<(), String> {
    let mut saved = app.saved.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(index) = saved.favourites.iter().position(|k| k == &key) {
        saved.favourites.remove(index);
    } else {
        saved.favourites.push(key);
    }
    app.save(&saved)
}

#[derive(Serialize)]
struct ProbeResult {
    key: String,
    reach: probe::Reach,
}

/// Probes every server at once, one thread each.
#[tauri::command]
async fn probe(targets: Vec<(String, Option<String>)>) -> Vec<ProbeResult> {
    let mut handles = Vec::new();
    for (key, server) in targets {
        handles.push(std::thread::spawn(move || ProbeResult {
            reach: probe::probe(server.as_deref()),
            key,
        }));
    }
    let mut results = Vec::new();
    for handle in handles {
        if let Ok(result) = handle.join() {
            results.push(result);
        }
    }
    results
}

/// Opens Zeabur's web interface for one context in its own window.
#[tauri::command]
fn open_cluster(
    handle: tauri::AppHandle,
    app: State<'_, App>,
    key: String,
    name: String,
    lang: String,
) -> Result<(), String> {
    {
        let mut saved = app.saved.lock().unwrap_or_else(|e| e.into_inner());
        saved.recents.retain(|k| *k != key);
        saved.recents.insert(0, key.clone());
        saved.recents.truncate(5);
        app.save(&saved)?;
    }
    let url = app
        .web_url
        .parse::<tauri::Url>()
        .map_err(|e| e.to_string())?;
    // The web interface keeps its language under `zeabur.locale` and has no Japanese yet; without
    // a match it follows the system languages.
    let locale = if ["en", "zh-TW", "zh-CN"].contains(&lang.as_str()) {
        format!("localStorage.setItem('zeabur.locale', '{lang}')")
    } else {
        "localStorage.removeItem('zeabur.locale')".to_owned()
    };
    let script = format!("try {{ {locale} }} catch {{}}");
    let label = format!("cluster-{}", shots::label_hash(&key));
    if let Some(window) = handle.get_webview_window(&label) {
        let _ = window.set_focus();
        return Ok(());
    }
    let builder = WebviewWindowBuilder::new(&handle, label, WebviewUrl::External(url))
        .title(format!("{name} - Zeabur"))
        .inner_size(1280.0, 800.0)
        .position(120.0, 80.0)
        .initialization_script(&script);
    let window = shots::apply_theme(builder)
        .build()
        .map_err(|e| e.to_string())?;
    if shots::wants("cluster") {
        shots::capture(window);
    }
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|tauri_app| {
            // `ZEABUR_DESKTOP_STATE` puts everything in one folder, for the fixtures and
            // screenshots.
            let (config_dir, kubeconfigs_dir) = match std::env::var_os("ZEABUR_DESKTOP_STATE") {
                Some(dir) => (PathBuf::from(&dir), PathBuf::from(dir).join("kubeconfigs")),
                None => {
                    let paths = tauri_app.path();
                    (
                        paths.app_config_dir()?,
                        paths.app_local_data_dir()?.join("kubeconfigs"),
                    )
                }
            };
            let saved = std::fs::read(config_dir.join("settings.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default();
            tauri_app.manage(App {
                config_dir,
                kubeconfigs_dir,
                saved: Mutex::new(saved),
                env: RwLock::new(ChildEnv::from_process()),
                shell: Mutex::new(ShellRead::Pending),
                web_url: std::env::var("ZEABUR_DESKTOP_WEB")
                    .unwrap_or_else(|_| "http://localhost:5173/".into()),
            });

            // A login shell can take seconds; the window opens without waiting and reloads on the
            // event.
            let handle = tauri_app.handle().clone();
            std::thread::spawn(move || {
                let (merged, read) = env::load(ChildEnv::from_process());
                let app = handle.state::<App>();
                *app.env.write().unwrap_or_else(|e| e.into_inner()) = merged;
                *app.shell.lock().unwrap_or_else(|e| e.into_inner()) = read;
                let _sent = handle.emit("shell-env", ());
            });

            let mut builder = WebviewWindowBuilder::new(
                tauri_app,
                "main",
                WebviewUrl::App("desktop.html".into()),
            )
            .title("Zeabur")
            .inner_size(1040.0, 680.0)
            .min_inner_size(720.0, 480.0);
            builder = shots::apply_theme(builder);
            let window = builder.build()?;
            if shots::wants("main") {
                shots::capture(window);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            add_paths,
            add_pasted,
            remove_source,
            toggle_favourite,
            probe,
            open_cluster,
            scenario
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|err| {
            eprintln!("zeabur-desktop: {err}");
            std::process::exit(1);
        });
}
