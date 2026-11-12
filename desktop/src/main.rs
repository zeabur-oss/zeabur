//! Proof of concept: the Zeabur desktop app. It lists every context of every kubeconfig source and
//! opens Zeabur's web interface for the ones the person picks.

mod probe;
mod shots;
mod sources;

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder};

use sources::{AddedSource, ContextView, SearchPaths, SourceKind, SourceRef, SourceView};

/// What the app remembers between launches. It never writes to a kubeconfig file.
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
    state_dir: PathBuf,
    saved: Mutex<Saved>,
    search: SearchPaths,
    web_url: String,
}

impl App {
    fn save(&self, saved: &Saved) -> Result<(), String> {
        std::fs::create_dir_all(&self.state_dir).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(saved).map_err(|e| e.to_string())?;
        std::fs::write(self.state_dir.join("state.json"), text).map_err(|e| e.to_string())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    sources: Vec<SourceView>,
    contexts: Vec<ContextView>,
    favourites: Vec<String>,
    recents: Vec<String>,
    /// PATH entries the app found only through the login shell; shown in the exec plugin note.
    shell_path_used: bool,
}

fn source_list(saved: &Saved) -> Vec<(String, SourceKind, PathBuf)> {
    let mut list = Vec::new();
    let defaults = sources::default_paths(std::env::var_os("KUBECONFIG"), std::env::home_dir());
    for path in defaults {
        if !saved.hidden_defaults.contains(&path) && path.exists() {
            list.push((format!("default:{}", path.display()), SourceKind::Default, path));
        }
    }
    for added in &saved.added {
        list.push((format!("added:{}", added.path.display()), added.kind, added.path.clone()));
    }
    list
}

#[tauri::command]
fn snapshot(app: State<'_, App>) -> Snapshot {
    let saved = app.saved.lock().unwrap_or_else(|e| e.into_inner());
    let list = source_list(&saved);
    let mut refs = Vec::new();
    for (id, kind, path) in &list {
        refs.push(SourceRef { id: id.clone(), kind: *kind, path });
    }
    let (sources, contexts) = sources::read_all(&refs, &app.search);
    Snapshot {
        sources,
        contexts,
        favourites: saved.favourites.clone(),
        recents: saved.recents.clone(),
        shell_path_used: !app.search.shell.is_empty(),
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
        let kind = if path.is_dir() { SourceKind::Folder } else { SourceKind::File };
        saved.hidden_defaults.retain(|p| p != &path);
        if !saved.added.iter().any(|a| a.path == path) {
            saved.added.push(AddedSource { kind, path });
        }
    }
    app.save(&saved)
}

/// Checks pasted YAML, then keeps it as a file in the app's folder.
#[tauri::command]
fn add_pasted(app: State<'_, App>, name: String, yaml: String) -> Result<(), String> {
    let config = kube::config::Kubeconfig::from_yaml(&yaml).map_err(|e| e.to_string())?;
    if config.contexts.is_empty() {
        return Err("no-contexts".into());
    }
    let mut slug = String::new();
    for c in name.trim().chars() {
        slug.push(if c.is_alphanumeric() || c == '-' { c } else { '-' });
    }
    if slug.is_empty() {
        slug.push_str("pasted");
    }
    let dir = app.state_dir.join("pasted");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = write_new(&dir, &slug, yaml.as_bytes()).map_err(|e| e.to_string())?;
    let mut saved = app.saved.lock().unwrap_or_else(|e| e.into_inner());
    saved.added.push(AddedSource { kind: SourceKind::Pasted, path });
    app.save(&saved)
}

/// Writes `bytes` to a new file `dir/{slug}.yaml`, or `{slug}-2.yaml` and so on when that name is
/// taken. It never replaces a file.
fn write_new(dir: &std::path::Path, slug: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    use std::io::Write;
    let mut n = 1;
    loop {
        let name = if n == 1 { format!("{slug}.yaml") } else { format!("{slug}-{n}.yaml") };
        let path = dir.join(name);
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(bytes)?;
                return Ok(path);
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => n += 1,
            Err(err) => return Err(err),
        }
    }
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
    let url = app.web_url.parse::<tauri::Url>().map_err(|e| e.to_string())?;
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
    let window = shots::apply_theme(builder).build().map_err(|e| e.to_string())?;
    if shots::wants("cluster") {
        shots::capture(window);
    }
    Ok(())
}

/// PATH from the login shell, so exec plugins installed by Homebrew, asdf or mise are found when
/// the app starts from Finder or the Dock.
fn login_shell_path() -> Vec<PathBuf> {
    if cfg!(windows) {
        return Vec::new();
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let output = std::process::Command::new(shell)
        .args(["-l", "-c", "printf %s \"$PATH\""])
        .stdin(std::process::Stdio::null())
        .output();
    match output {
        Ok(out) if out.status.success() => {
            std::env::split_paths(&String::from_utf8_lossy(&out.stdout).to_string()).collect()
        }
        _ => Vec::new(),
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|tauri_app| {
            let state_dir = match std::env::var_os("ZEABUR_DESKTOP_STATE") {
                Some(dir) => PathBuf::from(dir),
                None => tauri_app.path().app_config_dir()?,
            };
            let saved = std::fs::read(state_dir.join("state.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default();
            let app_path: Vec<PathBuf> = std::env::var_os("PATH")
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default();
            let mut shell = Vec::new();
            if std::env::var_os("ZEABUR_DESKTOP_NO_SHELL_PATH").is_none() {
                for dir in login_shell_path() {
                    if !app_path.contains(&dir) {
                        shell.push(dir);
                    }
                }
            }
            tauri_app.manage(App {
                state_dir,
                saved: Mutex::new(saved),
                search: SearchPaths { app: app_path, shell },
                web_url: std::env::var("ZEABUR_DESKTOP_WEB").unwrap_or_else(|_| "http://localhost:5173/".into()),
            });

            let mut builder = WebviewWindowBuilder::new(tauri_app, "main", WebviewUrl::App("desktop.html".into()))
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

#[cfg(test)]
mod tests {
    use super::write_new;

    #[test]
    fn pasting_the_same_name_twice_keeps_both_files() {
        let dir = std::env::temp_dir().join(format!("zeabur-desktop-pasted-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = write_new(&dir, "pasted", b"first").unwrap();
        let second = write_new(&dir, "pasted", b"second").unwrap();
        assert_ne!(first, second);
        assert_eq!(std::fs::read(&first).unwrap(), b"first");
        assert_eq!(std::fs::read(&second).unwrap(), b"second");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
