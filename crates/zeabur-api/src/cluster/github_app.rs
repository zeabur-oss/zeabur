//! The cluster's GitHub App, which Zeabur Builder builds private GitHub repositories with: made
//! from the browser, installed on GitHub, and granted to projects.
//!
//! | Route | What it does |
//! |---|---|
//! | `GET /api/v1/cluster/github-app` | the App, its installations and their projects, and whether the person may change them |
//! | `POST /api/v1/cluster/github-app/manifest` | a sealed `state` for GitHub's manifest flow, bound to the person for an hour |
//! | `GET /api/v1/cluster/github-app/form` | the page that sends the manifest to GitHub |
//! | `POST /api/v1/cluster/github-app/conversion` | turns the code GitHub sent back into the App, and plans storing it; writes nothing |
//! | `POST /api/v1/cluster/github-app/plan` | what granting an installation to projects writes, and a token; writes nothing |
//! | `POST /api/v1/cluster/github-app/apply` | carries out a plan by its token |
//! | `GET /api/v1/projects/{p}/github-app` | the installations granted to the project, and the repositories each can build |
//!
//! Zeabur Builder reads the App from the Secret [`SECRET`] in [`NAMESPACE`] (`app-id`,
//! `client-id`, `private-key`, `webhook-secret`, and `api-url` off github.com), and which
//! namespaces may use an installation from the ConfigMap [`GRANTS`] there: key the installation
//! ID, value the namespaces, `,`-separated. Zeabur writes both, as the person, through a plan, so
//! RBAC in `builder-system` decides who may: in practice a cluster administrator, with admin
//! rights. Zeabur grants itself nothing.
//!
//! GitHub's manifest flow [gh-manifest] makes the App. The browser posts the manifest to GitHub
//! from a page of this server whose `form-action` allows GitHub only; the web interface's own
//! policy stays `'self'`. GitHub sends the browser back with a code and the `state`; the
//! conversion opens the `state`, checks it is the person's and fresh, and exchanges the code
//! once. The private key and the webhook secret travel only in the sealed plan token, and the
//! plan shows the Secret with its values hidden.
//!
//! Project members cannot read `builder-system`, so a grant also writes what the project may
//! build into the project: the ConfigMap [`PROJECT_CONFIG_MAP`], one key per installation, with
//! the account and the repositories it could read when the grant was planned. Builder never
//! reads it; it only fills the repository picker. Granting again refreshes it.
//!
//! [gh-manifest]: https://docs.github.com/en/apps/sharing-github-apps/registering-a-github-app-from-a-manifest

use std::collections::{BTreeMap, BTreeSet};

use axum::Json;
use axum::extract::{Path, RawQuery, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use utoipa::ToSchema;
use zeabur_core::message::Message;
use zeabur_core::problem::{Denied, Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{Cluster, ClusterExt as _, ObjectTarget, ResourceRef, WriteBody, WriteRequest};

use crate::error::{ApiError, get_read, read_problem};
use crate::github::{self, Conversion, GitHub};
use crate::handlers::{AppState, Person};
use crate::page::{self, Lang};
use crate::plan_token::{self as token, LIFETIME_SECONDS, TokenError};
use crate::projects::namespace::Permission;
use crate::projects::writes::token_problem;
use crate::views::UnreadableView;

/// Zeabur Builder's namespace.
pub const NAMESPACE: &str = "builder-system";
/// The App's Secret, as Builder's chart names it (`githubApp.secretName`).
pub const SECRET: &str = "github-app";
/// Which namespaces may use which installation (`githubApp.grantsName`).
pub const GRANTS: &str = "github-app-installations";
/// The ConfigMap in a project with the installations granted to it.
pub const PROJECT_CONFIG_MAP: &str = "zeabur-github-app";
/// The label of [`PROJECT_CONFIG_MAP`].
pub const LABEL: &str = "zeabur.com/github-app";
/// The annotation Builder's own setup puts on the Secret: the App's page.
pub const PAGE_ANNOTATION: &str = "builder.zeabur.com/github-app";
const PLAN_PREFIX: &str = "gha1.";
const STATE_PREFIX: &str = "ghs1.";
/// How long the manifest flow may take, as long as GitHub keeps the code.
const STATE_SECONDS: i64 = 60 * 60;
const HIDDEN: &str = "(hidden)";

/// The cluster's GitHub App.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GitHubApp {
    /// Zeabur Builder's namespace, where the App is kept.
    pub namespace: String,
    /// The App; absent when there is none, or when the person may not read its Secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<GitHubAppView>,
    /// Whether the person may make the App and grant its installations.
    pub can_change: Permission,
    /// The App's installations; absent when they could not be read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installations: Option<Vec<GitHubInstallation>>,
    /// Why the installations could not be read, in GitHub's or Zeabur's words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installations_problem: Option<String>,
    /// The Secret's `api-url` is not an https URL, which is why the installations were not read.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub api_url_refused: bool,
    /// Where people open GitHub, `https://github.com`.
    pub web_url: String,
    /// What could not be read.
    pub unreadable: Vec<UnreadableView>,
}

/// The App, without its keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GitHubAppView {
    /// Its page on GitHub, `https://github.com/apps/<slug>`; empty when not known.
    pub html_url: String,
    /// Where to install it on an account; empty when not known.
    pub install_url: String,
    /// Whether it receives push events; without, Builder asks for new commits every minute.
    pub webhook: bool,
}

/// One installation of the App, and the projects it is granted to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GitHubInstallation {
    /// Its ID.
    pub id: i64,
    /// The user or organization it is installed on.
    pub account: String,
    /// `User` or `Organization`.
    pub account_type: String,
    /// `all` or `selected` repositories.
    pub repository_selection: String,
    /// Its settings page on GitHub.
    pub settings_url: String,
    /// The namespaces that may use it.
    pub namespaces: Vec<String>,
    /// Every namespace may use it (`*`).
    pub every_namespace: bool,
}

/// What the manifest flow needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GitHubManifestRequest {
    /// The App's name on GitHub, unique there, at most 34 characters.
    pub name: String,
    /// Where GitHub sends the browser back, `https://<zeabur>/settings/github-app`.
    pub redirect_url: String,
    /// Builder's webhook URL, `https://<host>/hooks/github-app`; empty when its receiver is off.
    #[serde(default)]
    pub webhook_url: String,
    /// The organization to make the App in; empty for the person's own account.
    #[serde(default)]
    pub organization: String,
}

/// The state of a manifest flow.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GitHubManifest {
    /// Open this page, which sends the manifest to GitHub.
    pub form_url: String,
}

/// What GitHub sent back.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct GitHubConversionRequest {
    /// The `code` query parameter.
    pub code: String,
    /// The `state` query parameter.
    pub state: String,
}

/// An installation to grant to projects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GitHubGrantRequest {
    /// The installation.
    pub installation_id: i64,
    /// The namespaces that may use it, all of them; empty takes it from every namespace.
    pub namespaces: Vec<String>,
}

/// What a write does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum GitHubAppWriteAction {
    /// Creates the object.
    Create,
    /// Changes it.
    Patch,
}

/// One object a plan writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GitHubAppWrite {
    /// What it does.
    pub action: GitHubAppWriteAction,
    /// The object's kind.
    pub kind: String,
    /// Its name.
    pub name: String,
    /// Its namespace.
    pub namespace: String,
    /// The object as it is, YAML; absent for a creation and for a Secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_yaml: Option<String>,
    /// The object as it will be, YAML, a Secret's values hidden.
    pub after_yaml: Option<String>,
}

/// What the person should know before applying a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum GitHubAppWarning {
    /// The cluster has an App already; Builds of its installations stop working.
    ReplacesApp,
    /// The App receives no push events, so Builder asks GitHub for new commits every minute.
    NoWebhook,
    /// Every namespace could use the installation (`*`); afterwards only those chosen can.
    WasEveryNamespace,
    /// Namespaces lose the installation; their Builds of its repositories stop.
    RemovesNamespaces,
}

/// A plan, and the token that applies it.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GitHubAppPlan {
    /// Send this to `apply` to carry out exactly this plan.
    pub token: String,
    /// When the token stops working, RFC 3339.
    pub expires_at: String,
    /// The writes, in order; empty when nothing changes.
    pub writes: Vec<GitHubAppWrite>,
    /// What to know first.
    pub warnings: Vec<GitHubAppWarning>,
}

/// The token of a plan to apply.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct GitHubAppApply {
    /// The token a plan returned.
    pub token: String,
}

/// What an apply wrote.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GitHubAppApplied {
    /// The objects written, `Kind/namespace/name`.
    pub written: Vec<String>,
}

/// The installations granted to a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGitHubApp {
    /// The installations, by account.
    pub installations: Vec<ProjectInstallation>,
}

/// An installation granted to a project, as the grant left it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInstallation {
    /// Its ID.
    pub id: i64,
    /// The user or organization it is installed on.
    pub account: String,
    /// The repositories it could read.
    pub repositories: Vec<ProjectRepository>,
    /// When the list was read from GitHub, RFC 3339.
    pub refreshed_at: String,
}

/// A repository a project may build with an installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRepository {
    /// `owner/name`.
    pub full_name: String,
    /// Its default branch.
    pub default_branch: String,
    /// Whether it is private.
    pub private: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FlowState {
    person: String,
    groups: Vec<String>,
    request: GitHubManifestRequest,
    expires_at: i64,
}

/// What a plan carries out, with what it read from GitHub.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum Intent {
    Save {
        app: Conversion,
        api_url: String,
    },
    Grant {
        installation: ProjectInstallation,
        namespaces: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Claims {
    person: String,
    groups: Vec<String>,
    intent: Intent,
    versions: BTreeMap<String, String>,
    expires_at: i64,
}

fn config_maps() -> ResourceRef {
    ResourceRef::new("", "v1", "configmaps", "ConfigMap")
}

fn secrets() -> ResourceRef {
    ResourceRef::new("", "v1", "secrets", "Secret")
}

fn problem(kind: Kind, detail: impl Into<String>) -> ApiError {
    ApiError::from(Problem::new(kind, detail))
}

fn github_problem(error: &str) -> ApiError {
    let mut problem = Problem::new(Kind::Unavailable, format!("GitHub: {error}"));
    "GitHub did not answer as expected".clone_into(&mut problem.title);
    problem.summary = Message::new("problem.summary.github").with("detail", error);
    problem.fix_for_occasion(
        "Try again. If it keeps failing, check that the cluster can reach GitHub.",
        Message::new("problem.github.fix"),
    );
    ApiError::from(problem)
}

async fn permission(cluster: &dyn Cluster, plural: &str) -> Permission {
    let right = Denied {
        verb: "create".to_owned(),
        group: String::new(),
        resource: plural.to_owned(),
        name: None,
        namespace: Some(NAMESPACE.to_owned()),
    };
    match cluster.review_access(&right).await {
        Ok(true) => Permission::Allowed,
        Ok(false) => Permission::Denied,
        Err(_) => Permission::Unknown,
    }
}

async fn find(
    cluster: &dyn Cluster,
    resource: &ResourceRef,
    namespace: &str,
    name: &str,
) -> Result<Option<Value>, ApiError> {
    let namespace = (resource.kind != "Namespace").then_some(namespace);
    cluster
        .find_object(resource, namespace, name)
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("{} {name}", resource.kind),
                get_read("", &resource.resource, namespace, name),
            )
        })
        .map(|o| o.map(|o| serde_json::to_value(o).unwrap_or_default()))
}

fn text<'a>(object: &'a Value, pointer: &str) -> &'a str {
    object
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// The namespaces of a grant: `(namespaces, every)`.
fn grant_of(value: &str) -> (Vec<String>, bool) {
    let mut every = false;
    let mut namespaces = BTreeSet::new();
    for n in value
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|n| !n.is_empty())
    {
        if n == "*" {
            every = true;
        } else {
            namespaces.insert(n.to_owned());
        }
    }
    (namespaces.into_iter().collect(), every)
}

fn dns_label(name: &str) -> bool {
    (1..=63).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
}

/// Builder's message for an `api-url` that is not https.
const API_URL_NOT_HTTPS: &str = "`api-url` must be an https URL";

/// Why the Secret's `api-url` is refused, in Builder's words. The address of Zeabur's own
/// settings is the administrator's and is not checked.
fn api_url_problem(api_url: &str, configured: &str) -> Option<&'static str> {
    if api_url == configured {
        return None;
    }
    match api_url.strip_prefix("https://") {
        None => Some(API_URL_NOT_HTTPS),
        Some(rest) if rest.split('/').next().is_none_or(str::is_empty) => {
            Some("`api-url` has no host")
        }
        Some(_) => None,
    }
}

/// The App's credentials from its Secret, read as the person: `(issuer, private key, API)`.
async fn credentials(
    cluster: &dyn Cluster,
    default_api: &str,
) -> Result<(String, String, String), ApiError> {
    let values = cluster
        .reveal_secret(NAMESPACE, SECRET)
        .await
        .map_err(|e| {
            read_problem(
                e,
                "the GitHub App's private key",
                get_read("", "secrets", Some(NAMESPACE), SECRET),
            )
        })?;
    let value = |key: &str| {
        values
            .get(key)
            .and_then(|v| std::str::from_utf8(v).ok())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
    };
    let issuer = value("client-id").or_else(|| value("app-id"));
    let (Some(issuer), Some(key)) = (issuer, value("private-key")) else {
        return Err(problem(
            Kind::Invalid,
            format!("The Secret {SECRET} in {NAMESPACE} has no app-id or private-key."),
        ));
    };
    let api = value("api-url").map(|v| v.trim_end_matches('/').to_owned());
    if let Some(why) = api
        .as_deref()
        .and_then(|api| api_url_problem(api, default_api.trim_end_matches('/')))
    {
        let mut refused = Problem::new(Kind::Invalid, why);
        refused.summary = Message::new("problem.summary.githubApiUrl");
        refused.fix_for_occasion(
            "Set `api-url` in the Secret to the https address of GitHub's API.",
            Message::new("problem.githubApiUrl.fix"),
        );
        return Err(ApiError::from(refused));
    }
    Ok((issuer, key, api.unwrap_or_else(|| default_api.to_owned())))
}

fn client(state: &AppState) -> Result<&crate::images::https::Https, ApiError> {
    state.github.as_ref().ok_or_else(|| {
        problem(
            Kind::Internal,
            "Zeabur could not set up TLS when it started, so it cannot reach GitHub.",
        )
    })
}

/// The App, its installations and their projects. The installations are read from GitHub with
/// the App's private key, read as the person; a person who may not read it sees the App without
/// them.
#[utoipa::path(get, path = "/api/v1/cluster/github-app", tag = "cluster",
    operation_id = "getGitHubApp",
    responses((status = 200, body = GitHubApp),
              (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    State(state): State<AppState>,
    Person(cluster): Person,
) -> Result<Json<GitHubApp>, ApiError> {
    let cluster = cluster.as_ref();
    let mut unreadable = Vec::new();
    let mut read = async |resource: ResourceRef, name: &str| match cluster
        .find_object(&resource, Some(NAMESPACE), name)
        .await
    {
        Ok(object) => Ok(object.map(|o| serde_json::to_value(o).unwrap_or_default())),
        Err(error) => {
            let refusal = error
                .into_unreadable(&resource.resource)
                .map_err(|e| ApiError::from(e.into_problem("the GitHub App")))?;
            unreadable.push(UnreadableView::from(refusal));
            Ok::<_, ApiError>(None)
        }
    };
    let secret = read(secrets(), SECRET).await?;
    let grants = read(config_maps(), GRANTS).await?;
    let app = secret.as_ref().map(|s| {
        let html_url = text(
            s,
            &format!(
                "/metadata/annotations/{}",
                PAGE_ANNOTATION.replace('/', "~1")
            ),
        )
        .trim_end_matches('/')
        .to_owned();
        GitHubAppView {
            install_url: if html_url.is_empty() {
                String::new()
            } else {
                format!("{html_url}/installations/new")
            },
            html_url,
            webhook: s.pointer("/data/webhook-secret").is_some(),
        }
    });
    let can_change = permission(cluster, "secrets")
        .await
        .and(permission(cluster, "configmaps").await);
    let (installations, installations_problem) = if app.is_some() {
        match listed(&state, cluster, grants.as_ref()).await {
            Ok(list) => (Some(list), None),
            Err(why) => (None, Some(why)),
        }
    } else {
        (None, None)
    };
    let api_url_refused = matches!(
        installations_problem.as_deref(),
        Some(API_URL_NOT_HTTPS | "`api-url` has no host")
    );
    Ok(Json(GitHubApp {
        namespace: NAMESPACE.to_owned(),
        app,
        can_change,
        installations,
        installations_problem,
        api_url_refused,
        web_url: state.settings.github.web_url.clone(),
        unreadable,
    }))
}

async fn listed(
    state: &AppState,
    cluster: &dyn Cluster,
    grants: Option<&Value>,
) -> Result<Vec<GitHubInstallation>, String> {
    let (issuer, key, api) = credentials(cluster, &state.settings.github.api_url)
        .await
        .map_err(|e| e.0.detail.clone())?;
    let http = client(state).map_err(|e| e.0.detail.clone())?;
    let jwt = github::jwt(&issuer, &key, (state.clock)().as_second())?;
    let found = GitHub { http, api: &api }.installations(&jwt).await?;
    Ok(found
        .into_iter()
        .map(|i| {
            let grant = grants.map_or_default(|g| text(g, &format!("/data/{}", i.id)));
            let (namespaces, every_namespace) = grant_of(grant);
            GitHubInstallation {
                id: i.id,
                account: i.account.login,
                account_type: i.account.kind,
                repository_selection: i.repository_selection,
                settings_url: i.html_url,
                namespaces,
                every_namespace,
            }
        })
        .collect())
}

/// The `state` GitHub sent back is expired, altered, or from another server.
fn flow_problem() -> ApiError {
    problem(
        Kind::BadRequest,
        "The link from GitHub expired, or it is not from this server. Start again from the \
         cluster page.",
    )
}

fn person_mismatch() -> ApiError {
    problem(
        Kind::Forbidden,
        "Someone else started this, or you started it with other group memberships.",
    )
}

fn sealing(state: &AppState) -> Result<std::sync::Arc<token::PlanKey>, ApiError> {
    state.plan_key.clone().ok_or_else(|| {
        problem(
            Kind::Internal,
            "Zeabur could not make a token key when it started, so it cannot plan changes.",
        )
    })
}

fn sealed<T: Serialize>(state: &AppState, prefix: &str, claims: &T) -> Result<String, ApiError> {
    token::sign_as(&*sealing(state)?, prefix, claims).map_err(|_random| {
        problem(
            Kind::Internal,
            "The system random number generator failed, so Zeabur could not seal the token.",
        )
    })
}

/// What is wrong with a manifest request, in words for the person.
fn manifest_problem(
    request: &GitHubManifestRequest,
    public_host: Option<&str>,
) -> Option<&'static str> {
    let name = request.name.trim();
    let url = |u: &str| u.starts_with("https://") || u.starts_with("http://");
    let redirect_host = request
        .redirect_url
        .split_once("://")
        .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or_default());
    if name.is_empty() || name.chars().count() > 34 {
        Some("A GitHub App's name has 1 to 34 characters.")
    } else if !url(&request.redirect_url) || public_host.is_some_and(|h| redirect_host != Some(h)) {
        Some("GitHub sends you back to Zeabur's own address only.")
    } else if !request.webhook_url.is_empty() && !request.webhook_url.starts_with("https://") {
        Some("The webhook URL starts with https://.")
    } else if !request.organization.is_empty()
        && !request
            .organization
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        Some("An organization's name has letters, digits and -.")
    } else {
        None
    }
}

/// The manifest of the App [gh-manifest-params]: read the repositories, set commit statuses,
/// and receive pushes when Builder's receiver has a URL.
///
/// [gh-manifest-params]: https://docs.github.com/en/apps/sharing-github-apps/registering-a-github-app-from-a-manifest#github-app-manifest-parameters
fn manifest_of(request: &GitHubManifestRequest) -> Value {
    let mut manifest = json!({
        "name": request.name.trim(),
        "url": "https://github.com/zeabur-oss/builder",
        "description": "Builds container images of pushed commits with Zeabur Builder.",
        "redirect_url": request.redirect_url,
        "public": false,
        "default_permissions": {"contents": "read", "metadata": "read", "statuses": "write"},
    });
    if !request.webhook_url.is_empty()
        && let Some(fields) = manifest.as_object_mut()
    {
        fields.insert(
            "hook_attributes".to_owned(),
            json!({"url": request.webhook_url, "active": true}),
        );
        fields.insert("default_events".to_owned(), json!(["push"]));
    }
    manifest
}

/// Starts GitHub's manifest flow: a `state` sealed for the person, valid for an hour, and the
/// page that sends the manifest to GitHub. Writes nothing. A person who may not write Builder's
/// Secret is refused here, before GitHub makes an App that Zeabur could not store.
#[utoipa::path(post, path = "/api/v1/cluster/github-app/manifest", tag = "cluster",
    operation_id = "startGitHubAppManifest", request_body = GitHubManifestRequest,
    responses((status = 200, body = GitHubManifest),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn manifest(
    State(state): State<AppState>,
    Person(cluster): Person,
    Json(request): Json<GitHubManifestRequest>,
) -> Result<Json<GitHubManifest>, ApiError> {
    if let Some(why) = manifest_problem(&request, state.settings.public_host.as_deref()) {
        return Err(problem(Kind::Invalid, why));
    }
    let can_change = permission(cluster.as_ref(), "secrets")
        .await
        .and(permission(cluster.as_ref(), "configmaps").await);
    if matches!(can_change, Permission::Denied) {
        return Err(problem(
            Kind::Forbidden,
            format!(
                "Only a cluster admin can set up the GitHub App: the cluster does not let you \
                 create Secrets and ConfigMaps in {NAMESPACE}."
            ),
        ));
    }
    let identity = cluster
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let claims = FlowState {
        person: identity.username,
        groups: identity.groups,
        request,
        expires_at: (state.clock)().as_second().saturating_add(STATE_SECONDS),
    };
    let sealed = sealed(&state, STATE_PREFIX, &claims)?;
    let query: String = form_urlencoded::Serializer::new(String::new())
        .append_pair("state", &sealed)
        .finish();
    Ok(Json(GitHubManifest {
        form_url: format!("/api/v1/cluster/github-app/form?{query}"),
    }))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The form page's only style, allowed by its hash.
const FORM_STYLE: &str = "body{margin:0;min-height:100vh;display:grid;place-items:center;\
font:15px/1.6 system-ui,sans-serif;color:#18181b;background:#fafafa}\
form{max-width:26rem;margin:16px;padding:24px;border:1px solid #e4e4e7;border-radius:12px;\
background:#fff}p{margin:0 0 16px}button{font:inherit;font-weight:500;padding:6px 14px;\
border:0;border-radius:8px;color:#fff;background:#18181b;cursor:pointer}\
@media(prefers-color-scheme:dark){body{color:#fafafa;background:#09090b}\
form{border-color:#27272a;background:#18181b}button{color:#18181b;background:#fafafa}}";

/// The page that sends the manifest to GitHub: a form with one button, no script, whose policy
/// lets forms go to GitHub only.
#[utoipa::path(get, path = "/api/v1/cluster/github-app/form", tag = "cluster",
    operation_id = "gitHubAppForm",
    params(("state" = String, Query, description = "The state `manifest` answered")),
    responses((status = 200, content_type = "text/html", description = "The page"),
              (status = 400, content_type = "text/html", description = "The state is not valid")))]
pub(crate) async fn form(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let lang = Lang::of(&headers);
    let sealed = form_urlencoded::parse(query.unwrap_or_default().as_bytes())
        .find(|(k, _)| k == "state")
        .map_or_default(|(_, v)| v.into_owned());
    let opened = sealing(&state)
        .ok()
        .and_then(|key| token::open_as::<FlowState>(&key, STATE_PREFIX, &sealed).ok())
        .filter(|c| (state.clock)().as_second() < c.expires_at);
    let Some(claims) = opened else {
        let text = match lang {
            Lang::En => "This link has expired. Go back to Zeabur and start again.",
            Lang::ZhTw => "這個連結已經失效，請回到 Zeabur 重新開始。",
            Lang::ZhCn => "这个链接已经失效，请回到 Zeabur 重新开始。",
        };
        return page::html(
            StatusCode::BAD_REQUEST,
            lang,
            "",
            None,
            &format!("<p>{text}</p>"),
        );
    };
    let web = state.settings.github.web_url.trim_end_matches('/');
    let organization = claims.request.organization.as_str();
    let path = if organization.is_empty() {
        "/settings/apps/new".to_owned()
    } else {
        format!("/organizations/{organization}/settings/apps/new")
    };
    let query: String = form_urlencoded::Serializer::new(String::new())
        .append_pair("state", &sealed)
        .finish();
    let action = format!("{web}{path}?{query}");
    let (lead, button) = match lang {
        Lang::En => (
            "GitHub asks you to confirm the new App. Zeabur brings you back when it is made.",
            "Continue on GitHub",
        ),
        Lang::ZhTw => (
            "接下來會到 GitHub 確認建立 App，建好後會自動回到 Zeabur。",
            "前往 GitHub",
        ),
        Lang::ZhCn => (
            "接下来会到 GitHub 确认创建 App，创建后会自动回到 Zeabur。",
            "前往 GitHub",
        ),
    };
    let body = format!(
        "<style>{FORM_STYLE}</style><form method=\"post\" action=\"{}\"><p>{lead}</p>\
<input type=\"hidden\" name=\"manifest\" value=\"{}\"><button type=\"submit\">{button}</button></form>",
        escape(&action),
        escape(&manifest_of(&claims.request).to_string()),
    );
    let mut response = page::html(StatusCode::OK, lang, "", None, &body);
    let style = STANDARD.encode(Sha256::digest(FORM_STYLE.as_bytes()));
    let policy = format!(
        "default-src 'none'; style-src 'sha256-{style}'; base-uri 'none'; form-action {web}; \
frame-ancestors 'none'"
    );
    if let Ok(value) = HeaderValue::from_str(&policy) {
        response
            .headers_mut()
            .insert(header::CONTENT_SECURITY_POLICY, value);
    }
    response
}

/// Turns the code GitHub sent back into the App, and plans storing it in Builder's Secret;
/// writes nothing. The private key and the webhook secret travel only sealed in the token.
#[utoipa::path(post, path = "/api/v1/cluster/github-app/conversion", tag = "cluster",
    operation_id = "convertGitHubApp", request_body = GitHubConversionRequest,
    responses((status = 200, body = GitHubAppPlan),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 503, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn conversion(
    State(state): State<AppState>,
    Person(cluster): Person,
    Json(request): Json<GitHubConversionRequest>,
) -> Result<Json<GitHubAppPlan>, ApiError> {
    let opened: FlowState = token::open_as(&*sealing(&state)?, STATE_PREFIX, &request.state)
        .map_err(|_error| flow_problem())?;
    if (state.clock)().as_second() >= opened.expires_at {
        return Err(flow_problem());
    }
    let identity = cluster
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    if (identity.username.as_str(), identity.groups.as_slice())
        != (opened.person.as_str(), opened.groups.as_slice())
    {
        return Err(person_mismatch());
    }
    let api = state.settings.github.api_url.clone();
    let app = GitHub {
        http: client(&state)?,
        api: &api,
    }
    .convert(&request.code)
    .await
    .map_err(|e| github_problem(&e))?;
    let intent = Intent::Save { app, api_url: api };
    planned(&state, cluster.as_ref(), identity, intent).await
}

/// Plans granting an installation to projects: Builder's grant, and the repositories it can read
/// in each project; writes nothing. The repositories are read from GitHub now.
#[utoipa::path(post, path = "/api/v1/cluster/github-app/plan", tag = "cluster",
    operation_id = "planGitHubApp", request_body = GitHubGrantRequest,
    responses((status = 200, body = GitHubAppPlan),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM),
              (status = 503, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn plan(
    State(state): State<AppState>,
    Person(cluster): Person,
    Json(request): Json<GitHubGrantRequest>,
) -> Result<Json<GitHubAppPlan>, ApiError> {
    let namespaces: BTreeSet<String> = request
        .namespaces
        .iter()
        .map(|n| n.trim().to_owned())
        .collect();
    if request.installation_id <= 0
        || namespaces.len() > 500
        || !namespaces.iter().all(|n| dns_label(n))
    {
        return Err(problem(
            Kind::Invalid,
            "Choose an installation and projects by their names.",
        ));
    }
    let identity = cluster
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let (issuer, key, api) = credentials(cluster.as_ref(), &state.settings.github.api_url).await?;
    let now = (state.clock)();
    let jwt = github::jwt(&issuer, &key, now.as_second()).map_err(|e| problem(Kind::Invalid, e))?;
    let github = GitHub {
        http: client(&state)?,
        api: &api,
    };
    let found = github
        .installation(&jwt, request.installation_id)
        .await
        .map_err(|e| github_problem(&e))?;
    let repositories = github
        .repositories(&jwt, request.installation_id)
        .await
        .map_err(|e| github_problem(&e))?;
    let installation = ProjectInstallation {
        id: found.id,
        account: found.account.login,
        repositories: repositories
            .into_iter()
            .map(|r| ProjectRepository {
                full_name: r.full_name,
                default_branch: r.default_branch,
                private: r.private,
            })
            .collect(),
        refreshed_at: now.to_string(),
    };
    let intent = Intent::Grant {
        installation,
        namespaces: namespaces.into_iter().collect(),
    };
    planned(&state, cluster.as_ref(), identity, intent).await
}

async fn planned(
    state: &AppState,
    cluster: &dyn Cluster,
    identity: zeabur_kube::Identity,
    intent: Intent,
) -> Result<Json<GitHubAppPlan>, ApiError> {
    let Computed {
        versions,
        writes,
        warnings,
    } = compute(cluster, &intent).await?;
    let expires_at = (state.clock)().as_second().saturating_add(LIFETIME_SECONDS);
    let claims = Claims {
        person: identity.username,
        groups: identity.groups,
        intent,
        versions,
        expires_at,
    };
    Ok(Json(GitHubAppPlan {
        token: sealed(state, PLAN_PREFIX, &claims)?,
        expires_at: Timestamp::from_second(expires_at)
            .unwrap_or(Timestamp::UNIX_EPOCH)
            .to_string(),
        writes: writes
            .iter()
            .map(|w| GitHubAppWrite {
                action: w.action,
                kind: w.target.kind.clone(),
                name: w.target.name.clone(),
                namespace: w.target.namespace.clone(),
                before_yaml: w.before.as_ref().and_then(yaml),
                after_yaml: yaml(&w.shown),
            })
            .collect(),
        warnings,
    }))
}

struct Planned {
    target: ObjectTarget,
    body: WriteBody,
    action: GitHubAppWriteAction,
    before: Option<Value>,
    shown: Value,
}

struct Computed {
    versions: BTreeMap<String, String>,
    writes: Vec<Planned>,
    warnings: Vec<GitHubAppWarning>,
}

fn target(kind: &str, namespace: &str, name: &str) -> ObjectTarget {
    ObjectTarget {
        api_version: "v1".to_owned(),
        kind: kind.to_owned(),
        plural: if kind == "Secret" {
            "secrets"
        } else {
            "configmaps"
        }
        .to_owned(),
        namespace: namespace.to_owned(),
        name: name.to_owned(),
    }
}

fn version(object: Option<&Value>) -> String {
    object.map_or_default(|o| text(o, "/metadata/resourceVersion").to_owned())
}

/// The write that makes `after` of `kind` exist, replacing `/data` of `before`; `shown` is what
/// the plan shows.
fn put(
    kind: &str,
    before: Option<&Value>,
    after: Value,
    shown: Value,
    extra: Vec<Value>,
) -> Planned {
    let namespace = text(&after, "/metadata/namespace").to_owned();
    let name = text(&after, "/metadata/name").to_owned();
    let target = target(kind, &namespace, &name);
    match before {
        None => Planned {
            target,
            body: WriteBody::Create(after),
            action: GitHubAppWriteAction::Create,
            before: None,
            shown,
        },
        Some(before) => {
            let mut ops = vec![
                json!({"op": "test", "path": "/metadata/resourceVersion", "value": version(Some(before))}),
                json!({"op": "add", "path": "/data", "value": after.get("data").cloned().unwrap_or_else(|| json!({}))}),
            ];
            ops.extend(extra);
            Planned {
                target,
                body: WriteBody::JsonPatch(Value::Array(ops)),
                action: GitHubAppWriteAction::Patch,
                before: (kind != "Secret").then(|| before.clone()),
                shown,
            }
        }
    }
}

fn data_of(object: Option<&Value>) -> serde_json::Map<String, Value> {
    object
        .and_then(|o| o.get("data"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

async fn compute(cluster: &dyn Cluster, intent: &Intent) -> Result<Computed, ApiError> {
    match intent {
        Intent::Save { app, api_url } => save(cluster, app, api_url).await,
        Intent::Grant {
            installation,
            namespaces,
        } => grant(cluster, installation, namespaces).await,
    }
}

async fn save(
    cluster: &dyn Cluster,
    app: &Conversion,
    api_url: &str,
) -> Result<Computed, ApiError> {
    let namespace = ResourceRef::new("", "v1", "namespaces", "Namespace");
    if find(cluster, &namespace, "", NAMESPACE).await?.is_none() {
        return Err(problem(
            Kind::Invalid,
            format!("Zeabur Builder is not installed: the cluster has no namespace {NAMESPACE}."),
        ));
    }
    let before = find(cluster, &secrets(), NAMESPACE, SECRET).await?;
    let mut values = BTreeMap::from([
        ("app-id", app.id.to_string()),
        ("client-id", app.client_id.clone()),
        ("private-key", app.pem.clone()),
    ]);
    if let Some(secret) = app.webhook_secret.as_ref().filter(|s| !s.is_empty()) {
        values.insert("webhook-secret", secret.clone());
    }
    if api_url.trim_end_matches('/') != crate::settings::GITHUB_API {
        values.insert("api-url", api_url.to_owned());
    }
    let metadata = |annotations: Value| json!({"name": SECRET, "namespace": NAMESPACE, "annotations": annotations});
    let annotations = json!({PAGE_ANNOTATION: app.html_url});
    let after = if before.is_none() {
        json!({
            "apiVersion": "v1", "kind": "Secret", "metadata": metadata(annotations.clone()),
            "type": "Opaque", "stringData": values,
        })
    } else {
        let data: serde_json::Map<String, Value> = values
            .iter()
            .map(|(k, v)| ((*k).to_owned(), json!(STANDARD.encode(v))))
            .collect();
        json!({"metadata": metadata(annotations.clone()), "data": data})
    };
    let shown = json!({
        "apiVersion": "v1", "kind": "Secret", "metadata": metadata(annotations),
        "type": "Opaque",
        "data": values.keys().map(|k| ((*k).to_owned(), json!(HIDDEN))).collect::<serde_json::Map<_, _>>(),
    });
    let mut kept = before
        .as_ref()
        .and_then(|b| b.pointer("/metadata/annotations"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    kept.insert(PAGE_ANNOTATION.to_owned(), json!(app.html_url));
    let extra = vec![json!({"op": "add", "path": "/metadata/annotations", "value": kept})];
    let mut warnings = Vec::new();
    if before.is_some() {
        warnings.push(GitHubAppWarning::ReplacesApp);
    }
    if !values.contains_key("webhook-secret") {
        warnings.push(GitHubAppWarning::NoWebhook);
    }
    Ok(Computed {
        versions: BTreeMap::from([("Secret".to_owned(), version(before.as_ref()))]),
        writes: vec![put("Secret", before.as_ref(), after, shown, extra)],
        warnings,
    })
}

async fn grant(
    cluster: &dyn Cluster,
    installation: &ProjectInstallation,
    namespaces: &[String],
) -> Result<Computed, ApiError> {
    let key = installation.id.to_string();
    let grants = find(cluster, &config_maps(), NAMESPACE, GRANTS).await?;
    let mut data = data_of(grants.as_ref());
    let (old, every) = grant_of(data.get(&key).and_then(Value::as_str).unwrap_or_default());
    let mut warnings = Vec::new();
    if every {
        warnings.push(GitHubAppWarning::WasEveryNamespace);
    }
    let chosen: BTreeSet<&str> = namespaces.iter().map(String::as_str).collect();
    let removed: Vec<&str> = old
        .iter()
        .map(String::as_str)
        .filter(|n| !chosen.contains(n))
        .collect();
    if !removed.is_empty() {
        warnings.push(GitHubAppWarning::RemovesNamespaces);
    }
    if namespaces.is_empty() {
        data.remove(&key);
    } else {
        data.insert(key.clone(), json!(namespaces.join(", ")));
    }
    let mut versions = BTreeMap::from([("ConfigMap".to_owned(), version(grants.as_ref()))]);
    let mut writes = Vec::new();
    let after = json!({"apiVersion": "v1", "kind": "ConfigMap",
        "metadata": {"name": GRANTS, "namespace": NAMESPACE}, "data": data});
    if data_of(grants.as_ref()) != data || grants.is_none() && !namespaces.is_empty() {
        writes.push(put(
            "ConfigMap",
            grants.as_ref(),
            after.clone(),
            after,
            Vec::new(),
        ));
    }
    let snapshot = serde_json::to_string(installation).unwrap_or_default();
    for namespace in namespaces
        .iter()
        .map(String::as_str)
        .chain(removed.iter().copied())
    {
        let before = find(cluster, &config_maps(), namespace, PROJECT_CONFIG_MAP).await?;
        versions.insert(format!("ConfigMap/{namespace}"), version(before.as_ref()));
        let mut data = data_of(before.as_ref());
        if chosen.contains(namespace) {
            data.insert(key.clone(), json!(snapshot));
        } else if data.remove(&key).is_none() {
            continue;
        }
        let after = json!({"apiVersion": "v1", "kind": "ConfigMap",
            "metadata": {"name": PROJECT_CONFIG_MAP, "namespace": namespace, "labels": {LABEL: "true"}},
            "data": data});
        writes.push(put(
            "ConfigMap",
            before.as_ref(),
            after.clone(),
            after,
            Vec::new(),
        ));
    }
    Ok(Computed {
        versions,
        writes,
        warnings,
    })
}

fn yaml(object: &Value) -> Option<String> {
    zeabur_graph::manifest::to_yaml(&zeabur_graph::manifest::clean(object)).ok()
}

async fn send(cluster: &dyn Cluster, write: &Planned, dry_run: bool) -> Result<(), ApiError> {
    let request = WriteRequest {
        target: write.target.clone(),
        body: write.body.clone(),
        dry_run,
    };
    let target = &write.target;
    let create = write.action == GitHubAppWriteAction::Create;
    let denied = Denied {
        verb: if create { "create" } else { "patch" }.to_owned(),
        group: String::new(),
        resource: target.plural.clone(),
        name: (!create).then(|| target.name.clone()),
        namespace: Some(target.namespace.clone()),
    };
    cluster.write(&request).await.map(|_| ()).map_err(|error| {
        read_problem(
            error,
            &format!("{} {} in {}", target.kind, target.name, target.namespace),
            denied,
        )
    })
}

/// Carries out a GitHub App plan by its token: checks it, dry-runs every write, then writes.
#[utoipa::path(post, path = "/api/v1/cluster/github-app/apply", tag = "cluster",
    operation_id = "applyGitHubApp", request_body = GitHubAppApply,
    responses((status = 200, body = GitHubAppApplied),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn apply(
    State(state): State<AppState>,
    Person(cluster): Person,
    Json(request): Json<GitHubAppApply>,
) -> Result<Json<GitHubAppApplied>, ApiError> {
    let claims: Claims =
        token::open_as(&*sealing(&state)?, PLAN_PREFIX, &request.token).map_err(token_problem)?;
    if (state.clock)().as_second() >= claims.expires_at {
        return Err(token_problem(TokenError::Expired));
    }
    let identity = cluster
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    if (identity.username.as_str(), identity.groups.as_slice())
        != (claims.person.as_str(), claims.groups.as_slice())
    {
        return Err(person_mismatch());
    }
    let Computed {
        versions, writes, ..
    } = compute(cluster.as_ref(), &claims.intent).await?;
    if versions != claims.versions {
        return Err(problem(
            Kind::Conflict,
            "The GitHub App or a project's grant changed after the plan was made.",
        ));
    }
    for write in &writes {
        send(cluster.as_ref(), write, true).await?;
    }
    for write in &writes {
        send(cluster.as_ref(), write, false).await?;
    }
    Ok(Json(GitHubAppApplied {
        written: writes
            .iter()
            .map(|w| format!("{}/{}/{}", w.target.kind, w.target.namespace, w.target.name))
            .collect(),
    }))
}

/// The installations granted to the project, and the repositories each could read when it was
/// granted; read as the person from the project's ConfigMap [`PROJECT_CONFIG_MAP`].
#[utoipa::path(get, path = "/api/v1/projects/{project}/github-app", tag = "projects",
    operation_id = "getProjectGitHubApp",
    params(("project" = String, Path, description = "The project's namespace")),
    responses((status = 200, body = ProjectGitHubApp),
              (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn project(
    Person(cluster): Person,
    Path(project): Path<String>,
) -> Result<Json<ProjectGitHubApp>, ApiError> {
    let object = find(
        cluster.as_ref(),
        &config_maps(),
        &project,
        PROJECT_CONFIG_MAP,
    )
    .await?;
    let mut installations: Vec<ProjectInstallation> = data_of(object.as_ref())
        .values()
        .filter_map(Value::as_str)
        .filter_map(|v| serde_json::from_str(v).ok())
        .collect();
    installations.sort_by(|a, b| a.account.cmp(&b.account).then(a.id.cmp(&b.id)));
    Ok(Json(ProjectGitHubApp { installations }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(webhook: &str) -> GitHubManifestRequest {
        GitHubManifestRequest {
            name: "Zeabur shop-prod".to_owned(),
            redirect_url: "https://zeabur.example.com/settings/github-app".to_owned(),
            webhook_url: webhook.to_owned(),
            organization: String::new(),
        }
    }

    #[test]
    fn the_manifest_asks_for_pushes_only_with_a_webhook() {
        let without = manifest_of(&request(""));
        assert_eq!(without["default_permissions"]["contents"], "read");
        assert!(without.get("hook_attributes").is_none());
        assert!(without.get("default_events").is_none());
        let with = manifest_of(&request("https://hooks.example.com/hooks/github-app"));
        assert_eq!(with["default_events"], json!(["push"]));
        assert_eq!(
            with["hook_attributes"]["url"],
            "https://hooks.example.com/hooks/github-app"
        );
    }

    #[test]
    fn github_sends_people_back_to_zeabur_only() {
        let host = Some("zeabur.example.com");
        assert_eq!(manifest_problem(&request(""), host), None);
        let mut elsewhere = request("");
        elsewhere.redirect_url = "https://evil.example.com/settings/github-app".to_owned();
        assert!(manifest_problem(&elsewhere, host).is_some());
        assert!(manifest_problem(&request("http://hooks.example.com"), host).is_some());
    }

    #[test]
    fn the_secrets_api_url_must_be_https_as_in_builder() {
        let own = "http://127.0.0.1:9";
        assert_eq!(
            api_url_problem("http://ghe.example.com/api/v3", "https://api.github.com"),
            Some("`api-url` must be an https URL")
        );
        assert_eq!(
            api_url_problem("https:///api", "https://api.github.com"),
            Some("`api-url` has no host")
        );
        assert_eq!(
            api_url_problem("https://ghe.example.com/api/v3", "https://api.github.com"),
            None
        );
        assert_eq!(api_url_problem(own, own), None);
    }

    #[test]
    fn a_grant_lists_namespaces_and_every() {
        assert_eq!(
            grant_of("shop, blog"),
            (vec!["blog".to_owned(), "shop".to_owned()], false)
        );
        assert_eq!(grant_of("*"), (Vec::new(), true));
    }
}
