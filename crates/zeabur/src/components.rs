//! `zeabur components install`: what **Cluster → Components** does, from the command line, as
//! the pod's service account. The chart's install-time Job runs it to install cert-manager
//! before anyone signs in.
//!
//! The API runs in this process (a service call on `zeabur_api::router`, no listener). The
//! command asks for the plan and applies it, one step at a time, until every step and health
//! check is ready, as the page does.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, header};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::time::Instant;
use tower::ServiceExt as _;
use zeabur_api::access::SingleCluster;
use zeabur_api::cluster::components::{
    ComponentAction, ComponentApplied, ComponentPlan, ComponentRequest, StepState,
};
use zeabur_api::views::CapabilityList;
use zeabur_catalog::InstallSource;
use zeabur_core::problem::Problem;
use zeabur_kube::k8s_openapi::jiff::SignedDuration;
use zeabur_kube::{
    Cluster as _, ClusterEndpoint, ObjectTarget, Preconditions, UserClient, WriteBody, WriteRequest,
};

/// The service account token of this pod, which kubelet renews.
pub(crate) const TOKEN_FILE: &str = "/var/run/secrets/kubernetes.io/serviceaccount/token";
/// How long to wait before asking for the plan again.
const POLL: Duration = Duration::from_secs(3);
/// The largest answer read.
const MAX_BODY: usize = 16 * 1024 * 1024;

/// Zeabur Catalog's components.
#[derive(Debug, clap::Parser)]
#[command(version)]
pub(crate) struct ComponentsArgs {
    #[command(subcommand)]
    command: ComponentsCommand,
}

#[derive(Debug, clap::Subcommand)]
enum ComponentsCommand {
    /// Install components as **Cluster → Components** does, acting as this pod's service
    /// account, which must be a cluster administrator. A component that is already installed,
    /// by anyone but Zeabur, is left as it is.
    Install(InstallArgs),
}

#[derive(Debug, clap::Args)]
struct InstallArgs {
    /// The components, by id, such as `cert-manager`, installed one after another.
    #[arg(required = true)]
    components: Vec<String>,
    /// How long all of them may take, such as `90s`, `10m` or `1h`.
    #[arg(long, default_value = "10m", value_parser = duration)]
    timeout: Duration,
    /// Where to read Zeabur Catalog's API: an `https://` URL, such as a mirror.
    #[arg(long, env = "ZEABUR_CATALOG_URL", default_value = zeabur_api::CATALOG_URL)]
    catalog_url: String,
    /// A PEM file of certificate authorities to trust, on top of the platform's roots, for
    /// reading the catalog.
    #[arg(long, env = "ZEABUR_EXTRA_CA_FILE")]
    extra_ca_file: Option<PathBuf>,
    /// The `ClusterRoleBinding` that gives this pod its rights, deleted once every component is
    /// ready.
    #[arg(long, value_name = "NAME")]
    revoke: Option<String>,
}

/// A time limit such as `90s`, `10m` or `1h30m`, in jiff's friendly format.
pub(crate) fn duration(text: &str) -> Result<Duration, String> {
    text.parse::<SignedDuration>()
        .ok()
        .and_then(|limit| Duration::try_from(limit).ok())
        .ok_or_else(|| format!("`{text}` is not a time limit, such as 90s, 10m or 1h"))
}

pub(crate) async fn run(args: &ComponentsArgs) -> Result<(), String> {
    let ComponentsCommand::Install(args) = &args.command;
    let settings = zeabur_api::Settings {
        catalog_url: crate::config::catalog_url(&args.catalog_url)?,
        trusted_ca: crate::config::bundle(&[(&args.extra_ca_file, "--extra-ca-file")])?,
        template_fetch: false,
        ..zeabur_api::Settings::default()
    };
    let endpoint = ClusterEndpoint::in_cluster().map_err(|e| e.to_string())?;
    let cluster =
        UserClient::with_token_file(&endpoint, Path::new(TOKEN_FILE)).map_err(|e| e.to_string())?;
    let api = Api::new(cluster.clone(), settings);
    let deadline = Instant::now() + args.timeout;
    for id in &args.components {
        tokio::time::timeout_at(deadline, install(&api, id))
            .await
            .map_err(|_elapsed| format!("{id} is not ready after {}s", args.timeout.as_secs()))??;
    }
    if let Some(binding) = &args.revoke {
        revoke(&cluster, binding).await;
    }
    Ok(())
}

/// Deletes `binding`. The components are ready, so a failure only leaves the binding to the next
/// `helm upgrade`, which removes it.
async fn revoke(cluster: &UserClient, binding: &str) {
    let request = WriteRequest {
        target: ObjectTarget {
            api_version: "rbac.authorization.k8s.io/v1".to_owned(),
            kind: "ClusterRoleBinding".to_owned(),
            plural: "clusterrolebindings".to_owned(),
            namespace: String::new(),
            name: binding.to_owned(),
        },
        body: WriteBody::Delete(Preconditions::default()),
        dry_run: false,
    };
    match cluster.write(&request).await {
        Ok(_written) => tracing::info!(event = "components.revoke", binding, "deleted"),
        Err(error) => {
            tracing::warn!(event = "components.revoke", binding, %error, "not deleted");
        }
    }
}

async fn install(api: &Api, id: &str) -> Result<(), String> {
    let capabilities: CapabilityList = api
        .get("/api/v1/cluster/capabilities")
        .await
        .map_err(describe)?;
    if installed_by_other(&capabilities, id) {
        tracing::info!(
            event = "components.install",
            component = id,
            "already installed"
        );
        return Ok(());
    }
    let plan_path = format!("/api/v1/cluster/components/{id}/plan");
    let apply_path = format!("/api/v1/cluster/components/{id}/apply");
    let mut waiting = String::new();
    loop {
        let request = ComponentRequest {
            action: ComponentAction::Install,
            catalog_version: None,
            values: BTreeMap::new(),
        };
        let plan: ComponentPlan = api.post(&plan_path, &request).await.map_err(describe)?;
        match progress(&plan) {
            Progress::Ready => {
                tracing::info!(event = "components.install", component = id, "ready");
                return Ok(());
            }
            Progress::Failed(why) => return Err(format!("{id}: {why}")),
            Progress::Waiting(what) if what != waiting => {
                tracing::info!(event = "components.install", component = id, waiting = %what, "waiting");
                waiting = what;
            }
            Progress::Waiting(_) => {}
        }
        let request = ComponentRequest {
            catalog_version: Some(plan.catalog_version),
            ..request
        };
        api.post::<_, ComponentApplied>(&apply_path, &request)
            .await
            .map_err(describe)?;
        tokio::time::sleep(POLL).await;
    }
}

/// Something other than Zeabur installed `id`, and it runs: Zeabur leaves it as it is.
fn installed_by_other(capabilities: &CapabilityList, id: &str) -> bool {
    capabilities
        .items
        .iter()
        .flat_map(|c| &c.installations)
        .any(|i| i.component == id && i.running && i.source != InstallSource::Zeabur)
}

#[derive(Debug, PartialEq, Eq)]
enum Progress {
    Ready,
    /// The first step or check that will not get ready without a change, and why.
    Failed(String),
    /// The first step or check that is not ready.
    Waiting(String),
}

fn progress(plan: &ComponentPlan) -> Progress {
    if plan.done {
        return Progress::Ready;
    }
    let steps = plan
        .steps
        .iter()
        .map(|s| (&s.release, s.state, s.message.as_deref()));
    let checks = plan
        .checks
        .iter()
        .map(|c| (&c.name, c.state, c.message.as_deref().or(Some(&c.advice))));
    let mut waiting = None;
    for (name, state, message) in steps.chain(checks) {
        match state {
            StepState::Ready => {}
            StepState::Failed => {
                let message = message.map_or_else(String::new, |m| format!(": {m}"));
                return Progress::Failed(format!("{name} failed{message}"));
            }
            StepState::Pending | StepState::Waiting => {
                waiting.get_or_insert_with(|| name.clone());
            }
        }
    }
    Progress::Waiting(waiting.unwrap_or_default())
}

fn describe(problem: Box<Problem>) -> String {
    problem.detail
}

/// The API, called in this process as the service account.
struct Api {
    router: Router,
}

impl Api {
    fn new(cluster: UserClient, settings: zeabur_api::Settings) -> Self {
        Self {
            router: zeabur_api::router_with(
                Arc::new(SingleCluster(Arc::new(cluster))),
                zeabur_api::Options {
                    settings,
                    ..zeabur_api::Options::default()
                },
            ),
        }
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, Box<Problem>> {
        self.send(Request::builder().uri(path), Body::empty()).await
    }

    async fn post<B: Serialize + Sync, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, Box<Problem>> {
        let bytes = serde_json::to_vec(body).map_err(internal)?;
        let request = Request::builder()
            .method(Method::POST)
            .uri(path)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::CONTENT_LENGTH, bytes.len());
        self.send(request, Body::from(bytes)).await
    }

    async fn send<T: DeserializeOwned>(
        &self,
        request: axum::http::request::Builder,
        body: Body,
    ) -> Result<T, Box<Problem>> {
        let request = request.body(body).map_err(internal)?;
        let Ok(response) = self.router.clone().oneshot(request).await;
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), MAX_BODY)
            .await
            .map_err(internal)?;
        if status.is_success() {
            serde_json::from_slice(&body).map_err(internal)
        } else {
            Err(serde_json::from_slice(&body).map_or_else(
                |_e| internal(format!("the API answered {status} without a problem")),
                Box::new,
            ))
        }
    }
}

fn internal(error: impl std::fmt::Display) -> Box<Problem> {
    Box::new(Problem::new(
        zeabur_core::problem::Kind::Internal,
        error.to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use clap::Parser as _;
    use serde_json::json;

    use super::*;

    fn plan(done: bool, steps: &[(&str, &str)], checks: &[(&str, &str)]) -> ComponentPlan {
        let steps: Vec<_> = steps
            .iter()
            .map(|(release, state)| {
                json!({"component": "cert-manager", "release": release, "state": state,
                       "message": "image pull failed", "writes": []})
            })
            .collect();
        let checks: Vec<_> = checks
            .iter()
            .map(|(name, state)| {
                json!({"component": "cert-manager", "name": name, "state": state,
                       "advice": "look at the pods", "timeoutSeconds": 300})
            })
            .collect();
        serde_json::from_value(json!({
            "action": "install", "catalogVersion": "0.6.1", "components": ["cert-manager"],
            "steps": steps, "checks": checks, "warnings": [], "inputs": [], "leftovers": [], "done": done,
        }))
        .unwrap()
    }

    fn capabilities(source: &str, running: bool) -> CapabilityList {
        serde_json::from_value(json!({
            "catalogueCommit": "abc", "installed": 1, "recommended": 0,
            "items": [{
                "name": "certificates", "existing": true, "evidence": [], "components": [],
                "installations": [{
                    "component": "cert-manager", "displayName": "cert-manager",
                    "source": source, "running": running,
                }],
            }],
        }))
        .unwrap()
    }

    #[test]
    fn a_plan_is_ready_only_when_it_is_done() {
        let done = plan(true, &[("cert-manager", "ready")], &[("pods", "ready")]);
        assert_eq!(progress(&done), Progress::Ready);
    }

    #[test]
    fn the_first_step_or_check_that_is_not_ready_is_what_it_waits_for() {
        let waiting = plan(
            false,
            &[("crds", "ready"), ("cert-manager", "waiting")],
            &[("pods", "pending")],
        );
        assert_eq!(progress(&waiting), Progress::Waiting("cert-manager".into()));
        let checking = plan(false, &[("crds", "ready")], &[("pods", "waiting")]);
        assert_eq!(progress(&checking), Progress::Waiting("pods".into()));
    }

    #[test]
    fn a_failed_step_or_check_stops_the_wait_and_says_why() {
        let step = plan(false, &[("cert-manager", "failed")], &[]);
        assert_eq!(
            progress(&step),
            Progress::Failed("cert-manager failed: image pull failed".into())
        );
        let check = plan(false, &[("cert-manager", "ready")], &[("pods", "failed")]);
        assert!(
            matches!(progress(&check), Progress::Failed(why) if why.starts_with("pods failed"))
        );
    }

    #[test]
    fn a_component_someone_else_runs_is_left_alone_and_zeaburs_is_continued() {
        for source in ["k3s", "helm-chart", "helm", "other"] {
            assert!(installed_by_other(
                &capabilities(source, true),
                "cert-manager"
            ));
        }
        assert!(!installed_by_other(
            &capabilities("zeabur", true),
            "cert-manager"
        ));
        assert!(!installed_by_other(
            &capabilities("helm", false),
            "cert-manager"
        ));
        assert!(!installed_by_other(&capabilities("helm", true), "traefik"));
    }

    #[test]
    fn install_takes_components_and_a_time_limit_and_none_of_the_servers_flags() {
        let args = ComponentsArgs::try_parse_from([
            "zeabur components",
            "install",
            "--timeout",
            "2m",
            "cert-manager",
        ])
        .unwrap();
        let ComponentsCommand::Install(install) = args.command;
        assert_eq!(install.components, ["cert-manager"]);
        assert_eq!(install.timeout, Duration::from_secs(120));
        assert!(ComponentsArgs::try_parse_from(["zeabur components", "install"]).is_err());
    }

    #[test]
    fn a_time_limit_is_a_duration() {
        for (text, want) in [
            ("90s", Duration::from_secs(90)),
            ("10m", Duration::from_secs(600)),
            ("1h", Duration::from_secs(3600)),
            ("0s", Duration::ZERO),
        ] {
            assert_eq!(duration(text), Ok(want), "{text}");
        }
        for bad in [
            "",
            "10",
            "m",
            "ten m",
            "-5m",
            "1d",
            "1x",
            "99999999999999999999h",
        ] {
            assert!(duration(bad).is_err(), "{bad}");
        }
    }
}
