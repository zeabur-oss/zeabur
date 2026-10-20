//! `zeaburctl doctor`: the cluster's version, its components and the Zeabur Identity setup, as the
//! person whose kubeconfig this is may see them.
//!
//! # The API's answers, not a second opinion
//!
//! Every check reads the API in this process ([`InProcess`]) and words its answer: the severity
//! of the version comes from `GET /cluster/upgrades`, the missing components from
//! `GET /cluster/capabilities` and `GET /cluster/gaps`, the Zeabur Identity objects from the
//! project endpoints and the identity from `GET /whoami`. A read the cluster refuses is skipped
//! with the rule the API names in `denied`; it is not a failure.
//!
//! Whether kube-apiserver trusts Zeabur Identity's issuer is in its `AuthenticationConfiguration`,
//! a file on the control plane that no RBAC rule can read, so it is reported as not checked.
//!
//! # Exit codes
//!
//! The Nagios plugin convention [nagios]: 0 when nothing needs doing, 1 for a warning, 2 for
//! something critical, 3 when the cluster cannot be reached or a check could not run. Several
//! results give the most serious one, in the order critical, warning, could not run. clap's own
//! usage error is also 2.
//!
//! [nagios]: https://nagios-plugins.org/doc/guidelines.html

use std::process::ExitCode;

use serde::Serialize;
use serde_json::Value;

use crate::in_process::InProcess;
use crate::output;

/// Zeabur Identity's API group (zeabur-identity-k8s).
const ZEABUR_IDENTITY_GROUP: &str = "identity.zeabur.com";
/// The namespace Zeabur Identity installs into by default.
const ZEABUR_IDENTITY_NAMESPACE: &str = "zeabur-identity";
/// The `OAuthClient` Zeabur's in-cluster server signs in with (`--client-id`).
const ZEABUR_CLIENT: &str = "zeabur";
/// The audience kube-apiserver accepts Zeabur Identity's tokens for.
const KUBERNETES_AUDIENCE: &str = "kubernetes";
/// The prefix Zeabur Identity's `AuthenticationConfiguration` gives usernames.
const ZEABUR_IDENTITY_USER_PREFIX: &str = "zeabur:";
const ZEABUR_IDENTITY_GET_STARTED: &str =
    "https://github.com/zeabur/identity/blob/main/docs/get-started.md";
const VERSION_SKEW_POLICY: &str = "https://kubernetes.io/releases/version-skew-policy/";

/// Options of `zeaburctl doctor`.
#[derive(Debug, clap::Args)]
#[command(after_long_help = "Exit codes (the Nagios plugin convention, \
https://nagios-plugins.org/doc/guidelines.html): 0 nothing to fix, 1 a warning, 2 something \
critical, 3 the cluster cannot be reached or a check could not run. A usage error is also 2.")]
pub(crate) struct DoctorArgs {
    /// The kubeconfig context to use (default: the current one).
    #[arg(long)]
    context: Option<String>,
    /// How to print the result.
    #[arg(long, value_enum, default_value_t = Format::Text)]
    output: Format,
}

/// How the result is printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum Format {
    /// One section per check, one line per finding.
    Text,
    /// `{"checks": [{name, status, findings, fix}]}`.
    Json,
}

/// The result of one check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Status {
    /// Nothing to do.
    Ok,
    /// Plan to act.
    Warning,
    /// Act now.
    Critical,
    /// The person may not read what the check needs, or it does not apply.
    Skipped,
    /// The check could not run.
    Error,
}

impl Status {
    /// As `--output json` names it.
    const fn label(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warning => "warning",
            Self::Critical => "critical",
            Self::Skipped => "skipped",
            Self::Error => "error",
        }
    }

    /// The exit code of a run whose most serious result this is.
    const fn exit_code(self) -> u8 {
        match self {
            Self::Ok | Self::Skipped => 0,
            Self::Warning => 1,
            Self::Critical => 2,
            Self::Error => 3,
        }
    }

    /// How serious it is, next to the other results of a run.
    const fn weight(self) -> u8 {
        match self {
            Self::Ok | Self::Skipped => 0,
            Self::Error => 1,
            Self::Warning => 2,
            Self::Critical => 3,
        }
    }
}

/// One check: its stable name, result, what it found and how to fix it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Check {
    name: &'static str,
    status: Status,
    findings: Vec<String>,
    fix: Option<String>,
}

impl Check {
    const fn new(name: &'static str) -> Self {
        Self {
            name,
            status: Status::Ok,
            findings: Vec::new(),
            fix: None,
        }
    }

    /// A check that ends at a refused or failed read: skipped with the rule the API names in
    /// `denied`, an error otherwise.
    fn from_problem(name: &'static str, problem: &Value) -> Self {
        let mut check = Self::new(name);
        if let Some(denied) = problem.get("denied") {
            check.status = Status::Skipped;
            check
                .findings
                .push(format!("skipped: needs {}", rule(denied)));
        } else {
            check.status = Status::Error;
            check.findings.push(detail(problem));
        }
        check.add_fix(text(problem, "fix"));
        check
    }

    /// Raises the result to `status` unless it is already as serious.
    const fn raise(&mut self, status: Status) {
        if status.weight() > self.status.weight() {
            self.status = status;
        }
    }

    fn add_fix(&mut self, fix: &str) {
        if fix.is_empty() {
            return;
        }
        match &mut self.fix {
            Some(text) => {
                text.push_str("; ");
                text.push_str(fix);
            }
            None => self.fix = Some(fix.to_owned()),
        }
    }
}

/// Every check of one run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Report {
    checks: Vec<Check>,
}

impl Report {
    /// The most serious result's exit code.
    fn exit_code(&self) -> u8 {
        self.checks
            .iter()
            .map(|c| c.status)
            .max_by_key(|s| s.weight())
            .map_or(0, Status::exit_code)
    }

    /// One section per check: its result and name, then a line per finding and the fix.
    fn text(&self) -> String {
        let mut lines = Vec::new();
        for check in &self.checks {
            lines.push(format!("{:<9}{}", check.status.label(), check.name));
            lines.extend(check.findings.iter().map(|f| format!("         {f}")));
            lines.extend(check.fix.iter().map(|f| format!("         fix: {f}")));
        }
        lines.push(String::new());
        lines.join("\n")
    }
}

pub(crate) fn run(args: &DoctorArgs) -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            output::error(&format!("cannot start: {error}"));
            return ExitCode::from(3);
        }
    };
    let report = runtime.block_on(async {
        let api =
            InProcess::from_kubeconfig(args.context.as_deref(), zeabur_api::Settings::default())
                .await?;
        examine(&api).await.map_err(|problem| detail(&problem))
    });
    let report = match report {
        Ok(report) => report,
        Err(message) => {
            output::error(&format!("cannot reach the cluster: {message}"));
            return ExitCode::from(3);
        }
    };
    match args.output {
        Format::Text => output::report(&report.text()),
        Format::Json => match serde_json::to_string_pretty(&report) {
            Ok(json) => output::report(&format!("{json}\n")),
            Err(error) => {
                output::error(&error.to_string());
                return ExitCode::from(3);
            }
        },
    }
    ExitCode::from(report.exit_code())
}

/// Every check; `Err` with the API's problem when the cluster does not even say who the person
/// is.
async fn examine(api: &InProcess) -> Result<Report, Value> {
    let whoami = api.get("/api/v1/whoami").await?;
    let zeabur_identity_kinds = format!(
        "/api/v1/projects/{ZEABUR_IDENTITY_NAMESPACE}/objects?kinds={ZEABUR_IDENTITY_GROUP}/oauthclients"
    );
    let zeabur_identity_namespace =
        format!("/api/v1/projects/{ZEABUR_IDENTITY_NAMESPACE}/namespace");
    let (upgrades, capabilities, gaps, kinds, namespace) = tokio::join!(
        api.get("/api/v1/cluster/upgrades"),
        api.get("/api/v1/cluster/capabilities"),
        api.get("/api/v1/cluster/gaps"),
        api.get(&zeabur_identity_kinds),
        api.get(&zeabur_identity_namespace),
    );
    let client_kind = kinds.as_ref().ok().and_then(|list| {
        list.get("items")?
            .as_array()?
            .iter()
            .find(|k| {
                text(k, "group") == ZEABUR_IDENTITY_GROUP && text(k, "resource") == "oauthclients"
            })
            .cloned()
    });
    let client = match &client_kind {
        Some(kind) => Some(
            api.get(&format!(
                "/api/v1/projects/{ZEABUR_IDENTITY_NAMESPACE}/objects/{ZEABUR_IDENTITY_GROUP}/{}/oauthclients/{ZEABUR_CLIENT}",
                text(kind, "version")
            ))
            .await,
        ),
        None => None,
    };
    let installed = zeabur_identity_installed(&kinds, &namespace);
    let identity = zeabur_identity_user(&whoami, installed.status == Status::Ok);
    Ok(Report {
        checks: vec![
            cluster_version(&upgrades),
            components(&capabilities),
            cluster_gaps(&gaps),
            installed,
            zeabur_identity_client(client.as_ref()),
            identity,
            zeabur_identity_issuer_trust(),
        ],
    })
}

/// The version's severity and states, from `GET /cluster/upgrades`.
fn cluster_version(answer: &Result<Value, Value>) -> Check {
    const NAME: &str = "cluster-version";
    let upgrades = match answer {
        Ok(upgrades) => upgrades,
        Err(problem) => return Check::from_problem(NAME, problem),
    };
    let mut check = Check::new(NAME);
    let minor = text(upgrades, "minor");
    let end_of_life = upgrades
        .get("release")
        .map_or("", |release| text(release, "endOfLife"));
    let version = text(upgrades, "kubernetesVersion");
    check.findings.push(if end_of_life.is_empty() {
        format!("kube-apiserver {version}")
    } else {
        format!("kube-apiserver {version} ({minor}, end of life {end_of_life})")
    });
    check.status = match text(upgrades, "severity") {
        "danger" => Status::Critical,
        "warning" => Status::Warning,
        "info" | "none" => Status::Ok,
        other => {
            check.findings.push(format!("unknown severity {other:?}"));
            Status::Error
        }
    };
    let skew = list(upgrades, "skew");
    let newer = Some(text(upgrades, "nextMinor"))
        .filter(|v| !v.is_empty())
        .or_else(|| Some(text(upgrades, "patch")).filter(|v| !v.is_empty()));
    for state in list(upgrades, "states") {
        match state.as_str().unwrap_or_default() {
            "end-of-life" => {
                check
                    .findings
                    .push(format!("{minor} gets no fixes any more"));
                check.add_fix(&newer.map_or_else(
                    || "upgrade one minor at a time (https://kubernetes.io/releases/)".to_owned(),
                    |v| format!("upgrade to {v}, one minor at a time"),
                ));
            }
            "maintenance" => {
                check.findings.push(format!(
                    "{minor} gets critical fixes only, until {end_of_life}"
                ));
                check.add_fix(&newer.map_or_else(
                    || format!("upgrade before {end_of_life}, one minor at a time"),
                    |v| format!("upgrade to {v} before {end_of_life}"),
                ));
            }
            "skew-unsupported" => {
                for node in skew
                    .iter()
                    .filter(|n| n.get("unsupported") == Some(&Value::Bool(true)))
                {
                    check.findings.push(format!(
                        "node {}: kubelet {} is outside the supported version skew",
                        text(node, "node"),
                        text(node, "kubeletVersion")
                    ));
                }
                check.add_fix(&format!(
                    "keep every kubelet at kube-apiserver's minor or up to three older \
                     ({VERSION_SKEW_POLICY})"
                ));
            }
            "skew-mixed" => {
                let nodes: Vec<String> = skew
                    .iter()
                    .map(|n| format!("{} ({})", text(n, "node"), text(n, "kubeletVersion")))
                    .collect();
                check
                    .findings
                    .push(format!("nodes run other minors: {}", nodes.join(", ")));
            }
            "upgrading" => {
                let nodes: Vec<&str> = list(upgrades, "upgrading")
                    .iter()
                    .filter_map(Value::as_str)
                    .collect();
                check
                    .findings
                    .push(format!("upgrading nodes: {}", nodes.join(", ")));
            }
            "patch-available" => check
                .findings
                .push(format!("{} is available", text(upgrades, "patch"))),
            "minor-available" => check
                .findings
                .push(format!("{} is available", text(upgrades, "nextMinor"))),
            "newer-than-data" => check.findings.push(format!(
                "{minor} is newer than Zeabur's release table ({})",
                text(upgrades, "dataAsOf")
            )),
            other => check.findings.push(other.to_owned()),
        }
    }
    unreadable(&mut check, upgrades);
    check
}

/// Missing recommended components and installed ones behind the tested version, from
/// `GET /cluster/capabilities`.
fn components(answer: &Result<Value, Value>) -> Check {
    const NAME: &str = "components";
    let capabilities = match answer {
        Ok(capabilities) => capabilities,
        Err(problem) => return Check::from_problem(NAME, problem),
    };
    let mut check = Check::new(NAME);
    for capability in list(capabilities, "items") {
        let name = text(capability, "name");
        if capability.get("existing") == Some(&Value::Bool(false))
            && text(capability, "suggestion") == "recommended"
        {
            let choices: Vec<&str> = list(capability, "components")
                .iter()
                .map(|c| text(c, "displayName"))
                .collect();
            check.raise(Status::Warning);
            check.findings.push(format!(
                "{name} is missing (recommended: {})",
                choices.join(" or ")
            ));
        }
        for installation in list(capability, "installations") {
            if text(installation, "upgrade").starts_with("behind-") {
                check.findings.push(format!(
                    "{} {} is behind the tested {}",
                    text(installation, "displayName"),
                    text(installation, "version"),
                    text(installation, "tested")
                ));
            }
        }
    }
    if check.status == Status::Warning {
        check.add_fix("`zeaburctl ui` shows how to install each: Cluster, Components");
    }
    check
}

/// What the cluster itself is missing, from `GET /cluster/gaps`.
fn cluster_gaps(answer: &Result<Value, Value>) -> Check {
    const NAME: &str = "cluster-gaps";
    let gaps = match answer {
        Ok(gaps) => gaps,
        Err(problem) => return Check::from_problem(NAME, problem),
    };
    let mut check = Check::new(NAME);
    let items = list(gaps, "items");
    for gap in items {
        if text(gap, "severity") == "warning" {
            check.raise(Status::Warning);
        }
        let subject = gap.get("subject").unwrap_or(&Value::Null);
        let params: Vec<String> = gap
            .get("message")
            .and_then(|m| m.get("params"))
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .map(|(k, v)| format!("{k} {}", v.as_str().unwrap_or_default()))
            .collect();
        let finding = format!(
            "{}: {} {}",
            text(gap, "rule"),
            text(subject, "kind"),
            text(subject, "name")
        );
        check.findings.push(if params.is_empty() {
            finding
        } else {
            format!("{finding} ({})", params.join(", "))
        });
        let fix = gap.get("fix").unwrap_or(&Value::Null);
        let how = match text(fix, "kind") {
            "install" => format!("install {}", text(fix, "id")),
            "plan" => format!("apply the {} plan in `zeaburctl ui`", text(fix, "id")),
            other => format!("{other} {} in `zeaburctl ui`", text(fix, "id")),
        };
        match list(gap, "citations").first() {
            Some(citation) => check.add_fix(&format!("{how} ({})", text(citation, "url"))),
            None => check.add_fix(&how),
        }
    }
    if items.is_empty() && !list(gaps, "unreadable").is_empty() {
        check.status = Status::Skipped;
    }
    unreadable(&mut check, gaps);
    check
}

/// Zeabur Identity's `OAuthClient` kind is served and its namespace exists.
fn zeabur_identity_installed(
    kinds: &Result<Value, Value>,
    namespace: &Result<Value, Value>,
) -> Check {
    const NAME: &str = "zeabur-identity-installed";
    let kinds = match kinds {
        Ok(kinds) => kinds,
        Err(problem) => return Check::from_problem(NAME, problem),
    };
    let mut check = Check::new(NAME);
    let mut served: Vec<&str> = list(kinds, "items")
        .iter()
        .filter(|k| text(k, "group") == ZEABUR_IDENTITY_GROUP)
        .map(|k| text(k, "kind"))
        .collect();
    served.sort_unstable();
    let has_client = served.contains(&"OAuthClient");
    let has_namespace = match namespace {
        Ok(_) => Some(true),
        Err(problem) if text(problem, "kind") == "not-found" => Some(false),
        Err(problem) => {
            // The namespace part only: the kinds still say whether Zeabur Identity is installed.
            let refused = Check::from_problem(NAME, problem);
            check.findings.extend(refused.findings);
            if refused.status == Status::Error {
                check.raise(Status::Error);
            }
            None
        }
    };
    if served.is_empty() {
        check
            .findings
            .push(format!("no {ZEABUR_IDENTITY_GROUP} kinds are served"));
    } else {
        check.findings.push(format!(
            "{ZEABUR_IDENTITY_GROUP} serves {}",
            served.join(", ")
        ));
    }
    match has_namespace {
        Some(true) => check
            .findings
            .push(format!("namespace {ZEABUR_IDENTITY_NAMESPACE} exists")),
        Some(false) => check.findings.push(format!(
            "namespace {ZEABUR_IDENTITY_NAMESPACE} does not exist"
        )),
        None => {}
    }
    match (has_client, has_namespace) {
        (true, Some(true) | None) => {}
        (true, Some(false)) => {
            check.raise(Status::Warning);
            check.findings.push(format!(
                "Zeabur Identity's kinds are served, but this check looks for Zeabur Identity only in \
                 {ZEABUR_IDENTITY_NAMESPACE}"
            ));
        }
        (false, Some(true)) => {
            check.raise(Status::Critical);
            check.add_fix(&format!(
                "{ZEABUR_IDENTITY_NAMESPACE} exists without Zeabur Identity's OAuthClient kind: install Zeabur Identity's \
                 CustomResourceDefinitions ({ZEABUR_IDENTITY_GET_STARTED})"
            ));
        }
        (false, Some(false) | None) => {
            check.raise(Status::Warning);
            check.findings.push(
                "Zeabur Identity is not installed; Zeabur's in-cluster server signs people in through it"
                    .to_owned(),
            );
            check.add_fix(&format!(
                "install Zeabur Identity: {ZEABUR_IDENTITY_GET_STARTED}"
            ));
        }
    }
    check
}

/// The `OAuthClient` `zeabur` exists, may ask for elevation and gets tokens for kube-apiserver.
fn zeabur_identity_client(answer: Option<&Result<Value, Value>>) -> Check {
    const NAME: &str = "zeabur-identity-client";
    let mut check = Check::new(NAME);
    let document = match answer {
        None => {
            check.status = Status::Skipped;
            check
                .findings
                .push("skipped: Zeabur Identity's OAuthClient kind is not served".to_owned());
            return check;
        }
        Some(Err(problem)) if text(problem, "kind") == "not-found" => {
            check.status = Status::Warning;
            check.findings.push(format!(
                "no OAuthClient {ZEABUR_CLIENT} in {ZEABUR_IDENTITY_NAMESPACE}"
            ));
            check.add_fix(&format!(
                "zeabur-identityctl client create {ZEABUR_CLIENT} --type confidential --allow-elevation \
                 --audience {KUBERNETES_AUDIENCE} --redirect-uri https://<Zeabur's host>/auth/callback \
                 | kubectl apply -f -"
            ));
            return check;
        }
        Some(Err(problem)) => return Check::from_problem(NAME, problem),
        Some(Ok(document)) => document,
    };
    let object: Value = match serde_saphyr::from_str(text(document, "yaml")) {
        Ok(object) => object,
        Err(error) => {
            check.status = Status::Error;
            check
                .findings
                .push(format!("cannot read OAuthClient {ZEABUR_CLIENT}: {error}"));
            return check;
        }
    };
    let spec = object.get("spec").unwrap_or(&Value::Null);
    let where_ = format!("OAuthClient {ZEABUR_IDENTITY_NAMESPACE}/{ZEABUR_CLIENT}");
    if list(spec, "audiences")
        .iter()
        .any(|a| a.as_str() == Some(KUBERNETES_AUDIENCE))
    {
        check
            .findings
            .push(format!("{where_} gets tokens for {KUBERNETES_AUDIENCE}"));
    } else {
        check.raise(Status::Critical);
        check.findings.push(format!(
            "{where_} does not get tokens for {KUBERNETES_AUDIENCE}: kube-apiserver refuses them"
        ));
        check.add_fix(&format!(
            "add {KUBERNETES_AUDIENCE} to spec.audiences of {where_}"
        ));
    }
    if spec.get("allowElevation") == Some(&Value::Bool(true)) {
        check
            .findings
            .push(format!("{where_} may ask for elevation"));
    } else {
        check.raise(Status::Warning);
        check
            .findings
            .push(format!("{where_} may not ask for elevation"));
        check.add_fix(&format!(
            "set spec.allowElevation: true on {where_}, or install Zeabur with \
             elevation.enabled=false"
        ));
    }
    check
}

/// Signed in with a Zeabur Identity token: the cluster names the person with Zeabur Identity's
/// prefix. With Zeabur Identity installed, another identity is a warning: Zeabur's in-cluster
/// server signs people in only through Zeabur Identity, so what this kubeconfig may do is not what
/// the browser gets. Without Zeabur Identity, `zeabur-identity-installed` already warns and this
/// check is skipped.
fn zeabur_identity_user(whoami: &Value, zeabur_identity_installed: bool) -> Check {
    let mut check = Check::new("zeabur-identity-user");
    let username = text(whoami, "username");
    if username.starts_with(ZEABUR_IDENTITY_USER_PREFIX) {
        check
            .findings
            .push(format!("signed in through Zeabur Identity as {username}"));
    } else if zeabur_identity_installed {
        check.status = Status::Warning;
        check.findings.push(format!(
            "you are \"{username}\", not a Zeabur Identity user"
        ));
        check.add_fix(
            "switch to a context that signs in through Zeabur Identity, then run doctor again",
        );
    } else {
        check.status = Status::Skipped;
        check.findings.push(format!(
            "skipped: you are {username}, not signed in with a Zeabur Identity token"
        ));
    }
    check
}

/// Not checked: kube-apiserver's `AuthenticationConfiguration` is not readable through RBAC.
fn zeabur_identity_issuer_trust() -> Check {
    let mut check = Check::new("zeabur-identity-issuer-trust");
    check.status = Status::Skipped;
    check.findings.push(
        "skipped: whether kube-apiserver trusts Zeabur Identity's issuer is in a file on the control \
         plane that no RBAC rule can read"
            .to_owned(),
    );
    check.add_fix("sign in with a Zeabur Identity context and run `kubectl auth whoami`");
    check
}

/// The reads a response names in `unreadable`, as findings.
fn unreadable(check: &mut Check, answer: &Value) {
    for refused in list(answer, "unreadable") {
        check.findings.push(format!(
            "partly checked: needs list {}",
            text(refused, "resource")
        ));
    }
}

/// A refused request as the rule that allows it: `get oauthclients.identity.zeabur.com/zeabur
/// in zeabur-identity`.
fn rule(denied: &Value) -> String {
    let part = |key: &str, before: &str| {
        let value = text(denied, key);
        if value.is_empty() {
            String::new()
        } else {
            format!("{before}{value}")
        }
    };
    format!(
        "{} {}{}{}{}",
        text(denied, "verb"),
        text(denied, "resource"),
        part("group", "."),
        part("name", "/"),
        part("namespace", " in ")
    )
}

/// A problem's `detail`, or its title when it has none.
fn detail(problem: &Value) -> String {
    let detail = text(problem, "detail");
    if detail.is_empty() {
        text(problem, "title").to_owned()
    } else {
        detail.to_owned()
    }
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn list<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use axum::Router;
    use serde_json::json;
    use zeabur_api::access::SingleCluster;
    use zeabur_core::problem::KubernetesStatus;
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::k8s_openapi::jiff::Timestamp;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{DynamicObject, Identity, ServedResource, Unreadable};

    use super::*;

    fn today() -> Timestamp {
        "2026-10-16T00:00:00Z".parse().unwrap()
    }

    fn api(cluster: &MemoryCluster) -> InProcess {
        InProcess::new(zeabur_api::router_at(
            Arc::new(SingleCluster(Arc::new(cluster.clone()))),
            today,
        ))
    }

    fn object(value: Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    fn zeabur_client(spec: &Value) -> DynamicObject {
        object(json!({
            "apiVersion": "identity.zeabur.com/v1alpha1", "kind": "OAuthClient",
            "metadata": {"name": "zeabur", "namespace": "zeabur-identity"},
            "spec": spec,
        }))
    }

    /// A cluster at `version`, with Zeabur Identity installed and Zeabur registered at it, as
    /// `zeabur:alice`.
    fn cluster(version: &str) -> MemoryCluster {
        let namespace: Namespace =
            serde_json::from_value(json!({"metadata": {"name": "zeabur-identity"}})).unwrap();
        let identity_resource = |resource: &str, kind: &str| {
            ServedResource::readable(ZEABUR_IDENTITY_GROUP, "v1alpha1", resource, kind, true)
        };
        MemoryCluster::new(State {
            identity: Identity {
                username: "zeabur:alice".to_owned(),
                groups: vec!["zeabur:admins".to_owned()],
            },
            namespaces: vec![namespace],
            served: vec![
                identity_resource("oauthclients", "OAuthClient"),
                identity_resource("users", "User"),
            ],
            dynamic: vec![zeabur_client(&json!({
                "type": "Confidential",
                "redirectUris": ["https://zeabur.example.com/auth/callback"],
                "audiences": ["kubernetes"],
                "secretHash": format!("sha256:{}", "0".repeat(64)),
                "allowElevation": true,
            }))],
            facts: zeabur_kube::ClusterFacts {
                version: version.to_owned(),
                ..zeabur_kube::ClusterFacts::default()
            },
            ..State::default()
        })
    }

    async fn report(cluster: &MemoryCluster) -> Report {
        examine(&api(cluster)).await.unwrap()
    }

    fn check<'a>(report: &'a Report, name: &str) -> &'a Check {
        report.checks.iter().find(|c| c.name == name).unwrap()
    }

    fn status(report: &Report, name: &str) -> Status {
        check(report, name).status
    }

    fn refuse_capabilities(cluster: &MemoryCluster) {
        cluster.update(|s| {
            s.facts.unreadable.push(Unreadable {
                resource: "customresourcedefinitions.apiextensions.k8s.io".to_owned(),
                status: KubernetesStatus::new(403, "Forbidden", "forbidden"),
            });
        });
    }

    #[tokio::test]
    async fn a_supported_version_and_zeabur_registered_at_zeabur_identity_are_ok() {
        let report = report(&cluster("v1.36.5+k3s1")).await;
        for name in [
            "cluster-version",
            "cluster-gaps",
            "zeabur-identity-installed",
            "zeabur-identity-client",
            "zeabur-identity-user",
        ] {
            assert_eq!(status(&report, name), Status::Ok, "{name}: {report:#?}");
        }
        assert_eq!(
            check(&report, "cluster-version").findings,
            ["kube-apiserver v1.36.5+k3s1 (1.36, end of life 2027-06-28)"]
        );
        assert_eq!(
            check(&report, "zeabur-identity-user").findings,
            ["signed in through Zeabur Identity as zeabur:alice"]
        );
        // Not readable through RBAC: said, never guessed.
        assert_eq!(
            status(&report, "zeabur-identity-issuer-trust"),
            Status::Skipped
        );
    }

    #[tokio::test]
    async fn the_versions_severity_is_the_apis() {
        let report = report(&cluster("v1.34.3+k3s1")).await;
        let version = check(&report, "cluster-version");
        assert_eq!(version.status, Status::Warning);
        assert_eq!(
            version.findings.get(1).map(String::as_str),
            Some("1.34 gets critical fixes only, until 2026-10-27")
        );
        assert!(version.fix.is_some());

        let cluster = cluster("v1.33.9+k3s1");
        refuse_capabilities(&cluster);
        let report = examine(&api(&cluster)).await.unwrap();
        assert_eq!(status(&report, "cluster-version"), Status::Critical);
        assert_eq!(report.exit_code(), 2);
    }

    #[tokio::test]
    async fn missing_recommended_components_are_a_warning_with_where_to_install_them() {
        let report = report(&cluster("v1.36.5+k3s1")).await;
        let components = check(&report, "components");
        assert_eq!(components.status, Status::Warning);
        assert!(
            components
                .findings
                .iter()
                .all(|f| f.contains(" is missing (recommended: ")),
            "{components:#?}"
        );
        assert_eq!(report.exit_code(), 1);
    }

    #[tokio::test]
    async fn a_retired_ingress_controller_is_a_cluster_gap_with_its_fix() {
        let cluster = cluster("v1.36.5+k3s1");
        cluster.update(|s| {
            s.served.push(ServedResource::readable(
                "networking.k8s.io",
                "v1",
                "ingressclasses",
                "IngressClass",
                false,
            ));
            s.dynamic.push(object(json!({
                "apiVersion": "networking.k8s.io/v1", "kind": "IngressClass",
                "metadata": {"name": "nginx"},
                "spec": {"controller": "k8s.io/ingress-nginx"},
            })));
        });
        let report = report(&cluster).await;
        let gaps = check(&report, "cluster-gaps");
        assert_eq!(gaps.status, Status::Warning);
        assert_eq!(
            gaps.findings,
            [
                "retired-ingress-controller: IngressClass nginx (class nginx, controller \
                 k8s.io/ingress-nginx)"
            ]
        );
        assert!(
            gaps.fix
                .as_deref()
                .is_some_and(|f| f.starts_with("install ingress/traefik (https://")),
            "{gaps:#?}"
        );
    }

    #[tokio::test]
    async fn refused_reads_are_skipped_with_the_rule_the_api_names() {
        let cluster = cluster("v1.36.5+k3s1");
        refuse_capabilities(&cluster);
        cluster.update(|s| {
            s.forbidden_in_namespaces
                .insert("oauthclients.identity.zeabur.com".to_owned());
            s.forbidden_resources.insert("namespaces".to_owned());
        });
        let report = report(&cluster).await;
        let components = check(&report, "components");
        assert_eq!(components.status, Status::Skipped);
        assert_eq!(
            components.findings,
            ["skipped: needs list customresourcedefinitions.apiextensions.k8s.io"]
        );
        let client = check(&report, "zeabur-identity-client");
        assert_eq!(client.status, Status::Skipped);
        assert_eq!(
            client.findings,
            ["skipped: needs get oauthclients.identity.zeabur.com/zeabur in zeabur-identity"]
        );
        assert!(client.fix.is_some(), "{client:#?}");
        let installed = check(&report, "zeabur-identity-installed");
        assert_eq!(installed.status, Status::Ok);
        assert!(
            installed
                .findings
                .contains(&"skipped: needs get namespaces/zeabur-identity".to_owned()),
            "{installed:#?}"
        );
        // Nothing left to fix: refusals are not failures.
        assert_eq!(report.exit_code(), 0);
    }

    #[tokio::test]
    async fn a_missing_or_misconfigured_zeabur_identity_is_named() {
        let absent = cluster("v1.36.5+k3s1");
        absent.update(|s| {
            s.served.clear();
            s.dynamic.clear();
            s.namespaces.clear();
        });
        let without = report(&absent).await;
        assert_eq!(
            status(&without, "zeabur-identity-installed"),
            Status::Warning
        );
        assert_eq!(status(&without, "zeabur-identity-client"), Status::Skipped);

        // The namespace without the kinds: half installed.
        let half = cluster("v1.36.5+k3s1");
        half.update(|s| {
            s.served.clear();
            s.dynamic.clear();
        });
        assert_eq!(
            status(&report(&half).await, "zeabur-identity-installed"),
            Status::Critical
        );

        let unregistered = cluster("v1.36.5+k3s1");
        unregistered.update(|s| s.dynamic.clear());
        let found = report(&unregistered).await;
        let client = check(&found, "zeabur-identity-client");
        assert_eq!(client.status, Status::Warning);
        assert!(
            client
                .fix
                .as_deref()
                .is_some_and(|f| f.starts_with("zeabur-identityctl client create zeabur ")),
            "{client:#?}"
        );

        let misconfigured = cluster("v1.36.5+k3s1");
        misconfigured.update(|s| {
            s.dynamic = vec![zeabur_client(&json!({
                "type": "Public",
                "redirectUris": ["https://zeabur.example.com/auth/callback"],
                "audiences": ["zeabur"],
            }))];
        });
        let found = report(&misconfigured).await;
        let client = check(&found, "zeabur-identity-client");
        assert_eq!(client.status, Status::Critical);
        assert_eq!(
            client.findings,
            [
                "OAuthClient zeabur-identity/zeabur does not get tokens for kubernetes: \
                 kube-apiserver refuses them",
                "OAuthClient zeabur-identity/zeabur may not ask for elevation",
            ]
        );
    }

    #[tokio::test]
    async fn an_identity_without_zeabur_identity_prefix_is_a_warning_when_zeabur_identity_is_installed()
     {
        let cluster = cluster("v1.36.5+k3s1");
        cluster.update(|s| s.identity.username = "kubernetes-admin".to_owned());
        let report = report(&cluster).await;
        assert_eq!(status(&report, "zeabur-identity-installed"), Status::Ok);
        let identity = check(&report, "zeabur-identity-user");
        assert_eq!(identity.status, Status::Warning);
        assert_eq!(
            identity.findings,
            [r#"you are "kubernetes-admin", not a Zeabur Identity user"#]
        );
        assert_eq!(
            identity.fix.as_deref(),
            Some(
                "switch to a context that signs in through Zeabur Identity, then run doctor again"
            )
        );
        assert_eq!(report.exit_code(), 1);
    }

    /// Without Zeabur Identity, `zeabur-identity-installed` already warns: the identity is not a
    /// second warning.
    #[tokio::test]
    async fn an_identity_without_zeabur_identity_prefix_is_skipped_without_zeabur_identity() {
        let cluster = cluster("v1.36.5+k3s1");
        cluster.update(|s| {
            s.identity.username = "kubernetes-admin".to_owned();
            s.served.clear();
            s.dynamic.clear();
            s.namespaces.clear();
        });
        let report = report(&cluster).await;
        assert_eq!(
            status(&report, "zeabur-identity-installed"),
            Status::Warning
        );
        let identity = check(&report, "zeabur-identity-user");
        assert_eq!(identity.status, Status::Skipped);
        assert_eq!(
            identity.findings,
            ["skipped: you are kubernetes-admin, not signed in with a Zeabur Identity token"]
        );
        assert_eq!(identity.fix, None);
    }

    #[tokio::test]
    async fn checks_that_cannot_run_are_errors_and_exit_3() {
        let cluster = cluster("v1.36.5+k3s1");
        cluster.update(|s| s.busy = Some(1));
        let report = report(&cluster).await;
        assert_eq!(status(&report, "cluster-version"), Status::Error);
        assert_eq!(status(&report, "components"), Status::Error);
        assert_eq!(report.exit_code(), 3);

        // No answer to who the person is: the cluster cannot be reached.
        assert!(examine(&InProcess::new(Router::new())).await.is_err());
    }

    #[test]
    fn the_most_serious_result_sets_the_exit_code() {
        let run = |statuses: &[Status]| {
            Report {
                checks: statuses
                    .iter()
                    .map(|&status| Check {
                        status,
                        ..Check::new("check")
                    })
                    .collect(),
            }
            .exit_code()
        };
        assert_eq!(run(&[]), 0);
        assert_eq!(run(&[Status::Ok, Status::Skipped]), 0);
        assert_eq!(run(&[Status::Ok, Status::Warning]), 1);
        assert_eq!(run(&[Status::Error, Status::Warning]), 1);
        assert_eq!(run(&[Status::Warning, Status::Critical, Status::Error]), 2);
        assert_eq!(run(&[Status::Ok, Status::Error]), 3);
    }

    /// The JSON and the text of one run, against `tests/golden/` (regenerate with
    /// `ZEABUR_UPDATE_GOLDEN=1`). Components are refused so the run does not follow the
    /// catalogue.
    #[tokio::test]
    async fn the_output_matches_its_golden_files() {
        let cluster = cluster("v1.34.3+k3s1");
        refuse_capabilities(&cluster);
        let report = report(&cluster).await;
        let outputs = [
            (
                "doctor.json",
                format!("{}\n", serde_json::to_string_pretty(&report).unwrap()),
            ),
            ("doctor.txt", report.text()),
        ];
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
        let update = std::env::var_os("ZEABUR_UPDATE_GOLDEN").is_some();
        let mut stale = Vec::new();
        for (file, text) in outputs {
            let path = dir.join(file);
            if update {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(&path, &text).unwrap();
            } else if std::fs::read_to_string(&path).ok().as_deref() != Some(&text) {
                stale.push(file);
            }
        }
        assert!(
            stale.is_empty(),
            "stale golden files (regenerate with ZEABUR_UPDATE_GOLDEN=1): {stale:?}"
        );
    }
}
