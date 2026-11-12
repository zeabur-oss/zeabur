//! Kubeconfig sources and the contexts in them. Each file is parsed on its own (no merging), so a
//! context name that appears in two files shows up twice, each with its file.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use kube::config::{AuthInfo, Kubeconfig};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    /// `~/.kube/config`, or each path in `KUBECONFIG`.
    Default,
    File,
    Folder,
    /// YAML pasted into the app, saved under the app's data folder.
    Pasted,
}

/// A source the person added. Default sources are not stored; they come from the environment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddedSource {
    pub kind: SourceKind,
    pub path: PathBuf,
}

/// The default sources: each path in `KUBECONFIG` (`:` between paths, `;` on Windows), or
/// `~/.kube/config` (`%USERPROFILE%\.kube\config` on Windows) when it is unset or empty.
pub fn default_paths(kubeconfig_env: Option<OsString>, home: Option<PathBuf>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(value) = kubeconfig_env.filter(|v| !v.is_empty()) {
        for path in std::env::split_paths(&value) {
            if !path.as_os_str().is_empty() {
                paths.push(path);
            }
        }
        return paths;
    }
    if let Some(home) = home {
        paths.push(home.join(".kube").join("config"));
    }
    paths
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceView {
    pub id: String,
    pub kind: SourceKind,
    pub path: String,
    /// Files read from this source (one, or each file of a folder).
    pub files: usize,
    pub contexts: usize,
    /// Files that could not be read or parsed, with the reason.
    pub errors: Vec<FileError>,
    pub missing: bool,
}

#[derive(Debug, Serialize)]
pub struct FileError {
    pub file: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum Auth {
    /// Nothing the app can check is wrong.
    Ok { method: String },
    /// The exec plugin is not on the app's PATH but is on the login shell's PATH.
    ShellPathOnly { command: String },
    /// The exec plugin is not found at all.
    PluginMissing {
        command: String,
        hint: Option<String>,
    },
    /// The context names a user or cluster the file does not define.
    MissingEntry { what: String, name: String },
    /// A file the user entry points to does not exist.
    FileMissing { path: String },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextView {
    /// The file path and the context name; stable across launches.
    pub key: String,
    pub source_id: String,
    pub file: String,
    pub name: String,
    pub cluster: String,
    pub server: Option<String>,
    pub user: Option<String>,
    pub namespace: Option<String>,
    pub current: bool,
    /// Other files define a context with the same name.
    pub same_name_elsewhere: usize,
    pub auth: Auth,
}

pub struct SourceRef<'a> {
    pub id: String,
    pub kind: SourceKind,
    pub path: &'a Path,
}

/// Where to look for exec plugins: the app's own PATH and, on macOS and Linux, the login
/// shell's PATH (a Finder or Dock launch gets only `/usr/bin:/bin:/usr/sbin:/sbin`).
#[derive(Debug, Default)]
pub struct SearchPaths {
    pub app: Vec<PathBuf>,
    pub shell: Vec<PathBuf>,
}

fn files_of(kind: SourceKind, path: &Path) -> std::io::Result<Vec<PathBuf>> {
    if kind != SourceKind::Folder {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut files = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') || !entry.file_type()?.is_file() {
            continue;
        }
        files.push(entry.path());
    }
    files.sort();
    Ok(files)
}

fn find_command(command: &str, dirs: &[PathBuf]) -> bool {
    let path = Path::new(command);
    if path.is_absolute() {
        return path.is_file();
    }
    for dir in dirs {
        if dir.join(command).is_file() {
            return true;
        }
        if cfg!(windows) {
            for ext in ["exe", "cmd", "bat"] {
                if dir.join(format!("{command}.{ext}")).is_file() {
                    return true;
                }
            }
        }
    }
    false
}

fn check_auth(info: &AuthInfo, search: &SearchPaths) -> Auth {
    if let Some(exec) = &info.exec {
        let command = exec.command.clone().unwrap_or_default();
        if find_command(&command, &search.app) {
            return Auth::Ok {
                method: format!("exec {command}"),
            };
        }
        if find_command(&command, &search.shell) {
            return Auth::ShellPathOnly { command };
        }
        return Auth::PluginMissing {
            command,
            hint: exec.install_hint.clone(),
        };
    }
    for path in [&info.client_certificate, &info.client_key, &info.token_file]
        .into_iter()
        .flatten()
    {
        if !Path::new(path).is_file() {
            return Auth::FileMissing { path: path.clone() };
        }
    }
    let method = if info.token.is_some() || info.token_file.is_some() {
        "token"
    } else if info.client_certificate.is_some() || info.client_certificate_data.is_some() {
        "client certificate"
    } else if let Some(provider) = &info.auth_provider {
        return Auth::Ok {
            method: format!("auth-provider {}", provider.name),
        };
    } else {
        "none"
    };
    Auth::Ok {
        method: method.to_owned(),
    }
}

/// Reads every source and returns the sources with counts and the contexts of every file.
pub fn read_all(
    sources: &[SourceRef<'_>],
    search: &SearchPaths,
) -> (Vec<SourceView>, Vec<ContextView>) {
    let mut views = Vec::new();
    let mut contexts = Vec::new();
    for source in sources {
        let mut view = SourceView {
            id: source.id.clone(),
            kind: source.kind,
            path: source.path.display().to_string(),
            files: 0,
            contexts: 0,
            errors: Vec::new(),
            missing: !source.path.exists(),
        };
        if view.missing {
            views.push(view);
            continue;
        }
        let files = match files_of(source.kind, source.path) {
            Ok(files) => files,
            Err(err) => {
                view.errors.push(FileError {
                    file: view.path.clone(),
                    reason: err.to_string(),
                });
                views.push(view);
                continue;
            }
        };
        for file in files {
            view.files += 1;
            let config = match Kubeconfig::read_from(&file) {
                Ok(config) => config,
                Err(err) => {
                    view.errors.push(FileError {
                        file: file.display().to_string(),
                        reason: err.to_string(),
                    });
                    continue;
                }
            };
            for named in &config.contexts {
                view.contexts += 1;
                contexts.push(context_view(&source.id, &file, &config, named, search));
            }
        }
        views.push(view);
    }

    let mut counts: HashMap<String, usize> = HashMap::new();
    for context in &contexts {
        *counts.entry(context.name.clone()).or_default() += 1;
    }
    for context in &mut contexts {
        context.same_name_elsewhere = counts[&context.name] - 1;
    }
    (views, contexts)
}

fn context_view(
    source_id: &str,
    file: &Path,
    config: &Kubeconfig,
    named: &kube::config::NamedContext,
    search: &SearchPaths,
) -> ContextView {
    let file_text = file.display().to_string();
    let context = named.context.clone().unwrap_or_default();
    let cluster = config.clusters.iter().find(|c| c.name == context.cluster);
    let server = cluster
        .and_then(|c| c.cluster.as_ref())
        .and_then(|c| c.server.clone());
    let auth = if cluster.is_none() {
        Auth::MissingEntry {
            what: "cluster".into(),
            name: context.cluster.clone(),
        }
    } else {
        match &context.user {
            None => Auth::Ok {
                method: "none".into(),
            },
            Some(user) => match config.auth_infos.iter().find(|a| &a.name == user) {
                None => Auth::MissingEntry {
                    what: "user".into(),
                    name: user.clone(),
                },
                Some(entry) => check_auth(&entry.auth_info.clone().unwrap_or_default(), search),
            },
        }
    };
    ContextView {
        key: format!("{file_text}#{}", named.name),
        source_id: source_id.to_owned(),
        file: file_text,
        name: named.name.clone(),
        cluster: context.cluster.clone(),
        server,
        user: context.user.clone(),
        namespace: context.namespace.clone(),
        current: config.current_context.as_deref() == Some(named.name.as_str()),
        same_name_elsewhere: 0,
        auth,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kubeconfig_env_splits_on_the_platform_separator() {
        let home = Some(PathBuf::from("/home/a"));
        #[cfg(windows)]
        let value = OsString::from(r"C:\Users\a\.kube\config;D:\kube\team.yaml;");
        #[cfg(not(windows))]
        let value = OsString::from("/home/a/.kube/config:/srv/kube/team.yaml:");
        let paths = default_paths(Some(value), home.clone());
        assert_eq!(paths.len(), 2);
        assert_eq!(
            default_paths(Some(OsString::new()), home.clone()),
            vec![PathBuf::from("/home/a/.kube/config")]
        );
        assert_eq!(default_paths(None, None), Vec::<PathBuf>::new());
    }

    #[test]
    fn same_context_names_in_two_files_stay_apart() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let home = fixtures.join("home/.kube/config");
        let team = fixtures.join("team");
        let sources = [
            SourceRef {
                id: "default".into(),
                kind: SourceKind::Default,
                path: &home,
            },
            SourceRef {
                id: "team".into(),
                kind: SourceKind::Folder,
                path: &team,
            },
        ];
        let (views, contexts) = read_all(&sources, &SearchPaths::default());
        assert_eq!(views[1].files, 3);
        let staging: Vec<_> = contexts.iter().filter(|c| c.name == "staging").collect();
        assert_eq!(staging.len(), 2);
        assert_ne!(staging[0].file, staging[1].file);
        assert!(staging.iter().all(|c| c.same_name_elsewhere == 1));
    }
}
