//! Cluster windows. Each one starts the local service (`zeabur-local`) for its kubeconfig file and
//! context, loads the service's one-time link, and stops the service when the window closes.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use zeabur_local::{Kubeconfig, Running, Settings};

use crate::env::ChildEnv;
use crate::output;

/// The kubeconfig file and context a cluster window opens.
#[derive(Clone, Debug)]
pub(crate) struct Target {
    pub(crate) file: PathBuf,
    pub(crate) context: String,
}

/// What each cluster window opens and the service it runs, by window label.
#[derive(Debug, Default)]
pub(crate) struct Clusters {
    targets: Mutex<HashMap<String, (u64, Target)>>,
    next: AtomicU64,
    running: Mutex<HashMap<String, Running>>,
}

impl Clusters {
    pub(crate) fn set_target(&self, label: &str, target: Target) {
        self.targets
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                label.to_owned(),
                (self.next.fetch_add(1, Ordering::Relaxed), target),
            );
    }

    /// Starts the service for the window `label`, replacing one it already runs, and returns the
    /// link to load.
    pub(crate) async fn start(&self, label: &str, env: &ChildEnv) -> Result<String, String> {
        let (id, target) = lookup(
            &self.targets.lock().unwrap_or_else(PoisonError::into_inner),
            label,
        )
        .ok_or_else(|| "This window has no cluster to open.".to_owned())?;
        self.stop(label);
        let own: Vec<(OsString, OsString)> = std::env::vars_os().collect();
        let kubeconfig = read(&target, &env.changed_from(&own))?;
        let running = zeabur_local::start_from(kubeconfig, &target.context, Settings::default())
            .await
            .map_err(|e| e.to_string())?;
        let url = running.url().to_owned();
        let targets = self.targets.lock().unwrap_or_else(PoisonError::into_inner);
        if lookup(&targets, label).is_none_or(|(now, _)| now != id) {
            drop(targets);
            stop_later(running);
            return Err("This window was closed.".to_owned());
        }
        self.running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(label.to_owned(), running);
        drop(targets);
        Ok(url)
    }

    /// Forgets the window `label` and stops its service.
    pub(crate) fn closed(&self, label: &str) {
        self.targets
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(label);
        self.stop(label);
    }

    fn stop(&self, label: &str) {
        let running = self
            .running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(label);
        if let Some(running) = running {
            stop_later(running);
        }
    }
}

fn stop_later(running: Running) {
    tauri::async_runtime::spawn(async move {
        if let Err(err) = running.stop().await {
            output::warn(&format!("the local service stopped with an error: {err}"));
        }
    });
}

fn lookup(targets: &HashMap<String, (u64, Target)>, label: &str) -> Option<(u64, Target)> {
    targets.get(label).cloned()
}

/// Reads the target's file and gives every exec credential plugin in it the variables of the
/// login shell (`vars`), unless the entry sets them itself.
fn read(target: &Target, vars: &[(String, String)]) -> Result<Kubeconfig, String> {
    let mut kubeconfig = Kubeconfig::read_from(&target.file)
        .map_err(|e| format!("cannot read {}: {e}", target.file.display()))?;
    for named in &mut kubeconfig.auth_infos {
        if let Some(exec) = named.auth_info.as_mut().and_then(|a| a.exec.as_mut()) {
            let list = exec.env.get_or_insert_with(Vec::new);
            for (name, value) in vars {
                let set = list
                    .iter()
                    .any(|entry| entry.get("name").is_some_and(|n| n == name));
                if !set {
                    list.push(HashMap::from([
                        ("name".to_owned(), name.clone()),
                        ("value".to_owned(), value.clone()),
                    ]));
                }
            }
        }
    }
    Ok(kubeconfig)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_startup_belongs_to_the_window_that_began_it() {
        let target = Target {
            file: PathBuf::from("config"),
            context: "c".to_owned(),
        };
        let clusters = Clusters::default();
        let id = |c: &Clusters| lookup(&c.targets.lock().unwrap(), "cluster-a").map(|(id, _)| id);
        clusters.set_target("cluster-a", target.clone());
        let first = id(&clusters);
        clusters.closed("cluster-a");
        assert_eq!(id(&clusters), None);
        clusters.set_target("cluster-a", target);
        assert!(id(&clusters).is_some());
        assert_ne!(id(&clusters), first);
    }

    #[test]
    fn exec_plugins_get_the_shell_variables_they_do_not_set() {
        let dir = std::env::temp_dir().join(format!("zeabur-desktop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("config");
        std::fs::write(
            &file,
            "apiVersion: v1\nkind: Config\nusers:\n- name: eks\n  user:\n    exec:\n      \
             apiVersion: client.authentication.k8s.io/v1\n      command: aws\n      env:\n      \
             - name: AWS_PROFILE\n        value: prod\n- name: plain\n  user:\n    token: t\n",
        )
        .unwrap();
        let target = Target {
            file,
            context: "eks".to_owned(),
        };
        let vars = vec![
            ("PATH".to_owned(), "/opt/homebrew/bin:/usr/bin".to_owned()),
            ("AWS_PROFILE".to_owned(), "dev".to_owned()),
        ];
        let kubeconfig = read(&target, &vars).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();

        let mut env = Vec::new();
        for named in &kubeconfig.auth_infos {
            let exec = named.auth_info.as_ref().and_then(|a| a.exec.as_ref());
            for entry in exec.and_then(|e| e.env.as_ref()).into_iter().flatten() {
                env.push((
                    named.name.clone(),
                    entry["name"].clone(),
                    entry["value"].clone(),
                ));
            }
        }
        assert_eq!(
            env,
            vec![
                (
                    "eks".to_owned(),
                    "AWS_PROFILE".to_owned(),
                    "prod".to_owned()
                ),
                (
                    "eks".to_owned(),
                    "PATH".to_owned(),
                    "/opt/homebrew/bin:/usr/bin".to_owned()
                ),
            ]
        );
    }
}
