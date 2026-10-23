#![doc = include_str!("../README.md")]
//! # Paths
//!
//! Paths follow the resource hierarchy [aip-121], with lower-case kinds (any case is accepted)
//! and two segments for kind and name, which keep names with dots unambiguous:
//!
//! | Method and path | Returns |
//! |---|---|
//! | `GET /api/v1/whoami` | who the cluster says you are |
//! | `GET /api/v1/session` | how you are signed in to Zeabur: signing out, admin rights in use |
//! | `GET /api/v1/cluster` | the cluster: its Kubernetes version and distribution |
//! | `GET /api/v1/cluster/upgrades` | end of life, version skew and newer versions from Plans |
//! | `GET /api/v1/cluster/nodes` | nodes, with requests against allocatable |
//! | `GET /api/v1/cluster/events` | events of every namespace, newest first, `limit` of them |
//! | `GET /api/v1/cluster/gaps` | what the cluster itself is missing, with fixes |
//! | `GET /api/v1/cluster/incidents` | components down and nodes not ready, with the domains and projects they reach |
//! | `GET /api/v1/cluster/capabilities` | what is installed (with versions), and what to install |
//! | `GET /api/v1/cluster/components/{capability}/{component}/install` | how to install it yourself, in four forms |
//! | `GET /api/v1/settings/metrics-store` | the metrics store history is read from, whether it answers, who may change it |
//! | `PUT /api/v1/settings/metrics-store` | changes it, or goes back to the default |
//! | `GET /api/v1/projects` | the projects, with the health of adopted ones |
//! | `GET /api/v1/projects/{p}` | the project document |
//! | `GET /api/v1/projects/{p}/namespace` | its Namespace: adopted or not, its details, whether you may change them |
//! | `GET /api/v1/projects/{p}/services/{kind}/{name}` | one service, cut from the document |
//! | `GET /api/v1/projects/{p}/services/{kind}/{name}/releases` | its releases, newest first |
//! | `GET /api/v1/projects/{p}/services/{kind}/{name}/trace` | every way to it, hop by hop from DNS to a ready pod, and where each stops |
//! | `GET /api/v1/projects/{p}/services/{kind}/{name}/rollout` | how far its rollout has come: new and old pods, done, progressing or stuck and why, with the plan that rolls back |
//! | `GET /api/v1/projects/{p}/services/{kind}/{name}/evidence` | its problems, trace, events, previous logs and objects as Markdown to copy; never a Secret or a variable's value |
//! | `GET /api/v1/projects/{p}/services/{kind}/{name}/timeline` | what changed in it and what went wrong, newest first, with the plan that reverts its running release |
//! | `GET /api/v1/projects/{p}/services/{kind}/{name}/history` | its pods' CPU, memory and restarts over a range, from the metrics store |
//! | `GET /api/v1/projects/{p}/usage` | what each pod uses now, from metrics-server |
//! | `GET /api/v1/projects/{p}/volumes` | how full each claim's volume is, from the kubelets |
//! | `GET /api/v1/projects/{p}/builds` | its Zeabur Builder builds and their runs, with each run's pod |
//! | `GET /api/v1/projects/{p}/gaps` | what the project is missing, with citations and fixes |
//! | `GET /api/v1/projects/{p}/timeline` | the same for every service of the project |
//! | `GET /api/v1/projects/{p}/incidents` | the cluster incidents that reach the project, with the services they break |
//! | `GET /api/v1/projects/{p}/objects` | every object of every namespaced kind, by kind |
//! | `GET /api/v1/projects/{p}/objects/{group}/{version}/{resource}/{name}` | one object as YAML (`core` for the empty group) |
//! | `GET /api/v1/projects/{p}/configmaps` | the ConfigMaps, in full |
//! | `POST /api/v1/projects/{p}/secrets/{name}/reveal` | a Secret's values |
//! | `GET /api/v1/projects/{p}/events` | events, newest first, `limit` of them |
//! | `GET /api/v1/projects/{p}/pods/{pod}/logs` | a log snapshot |
//! | `GET /api/v1/projects/{p}/pods/{pod}/logs/follow` | a log as Server-Sent Events |
//! | `GET /api/v1/projects/{p}/pods/{pod}/exec` | a terminal in a container, over a WebSocket (see the `pods::exec` module) |
//! | `GET /api/v1/projects/{p}/pods/{pod}/files` | a container's mounts, and whether its files are read in it or through a debug container (see the `pods::files` module) |
//! | `GET`, `DELETE /api/v1/projects/{p}/pods/{pod}/files/entries` | a folder's first 1,000 entries; deletes a file or folder |
//! | `GET`, `PUT /api/v1/projects/{p}/pods/{pod}/files/content` | downloads or uploads a file, up to 100 MB; never a Secret's |
//! | `GET /api/v1/projects/{p}/pods/{pod}/files/preview` | a file as text, up to 1 MB |
//! | `POST /api/v1/projects/{p}/pods/{pod}/files/folders` | makes a folder |
//! | `POST /api/v1/projects/{p}/pods/{pod}/files/moves` | renames a file or folder |
//! | `GET /api/v1/projects/{p}/pods/{pod}/debug` | its debug container for a container (the `debugContainer` change adds one) and why it does not start |
//! | `GET /api/v1/projects/{p}/tools` | your tools pods (the `clusterTools` change starts one); deletes those that have stopped (see the `tools` module) |
//! | `GET /api/v1/projects/{p}/tools/{name}/terminal` | a terminal with `kubectl` and `helm` acting as you, over a WebSocket |
//! | `DELETE /api/v1/projects/{p}/tools/{name}` | deletes your tools pod |
//! | `GET /api/v1/account` | who you are, your role in each project, and where to manage how you sign in |
//! | `GET /api/v1/members` | everyone Zeabur Identity signs in, and pending setup links |
//! | `POST /api/v1/members/plan` | the objects inviting someone, changing a member's access or removing a member writes, and a token; writes nothing |
//! | `POST /api/v1/members/apply` | carries out a member plan by its token; an invitation's link is made then, shown once |
//! | `DELETE /api/v1/members/invitations/{name}` | revokes a setup link |
//! | `POST /api/v1/members/{username}/setup-links` | a new setup link for a member, optionally removing their passkeys first |
//! | `DELETE /api/v1/members/{username}/credentials/{id}` | removes a member's passkey or linked account |
//! | `POST /api/v1/projects/{p}/plan` | the writes a change needs, what they do (pods replaced, fit, data and fields removed), and a token; writes nothing |
//! | `POST /api/v1/projects/{p}/apply` | carries out a plan by its token |
//!
//! There are no `/connections`, `/relations`, `/pods` or `/containers` collections: they only
//! mean something inside the project graph, which the document holds whole.
//!
//! Every write to a project goes through `plan` and `apply` (see the `projects::writes` module):
//! the plan token is the precondition of an apply, so writes need no `If-Match`, `412` or `428`. A
//! rollback is a plan too (the `rollback` change), as are storage, probes, compute and YAML
//! edits, adopting a namespace, letting it go and a project's details, so there is one write
//! path. Zeabur's own settings are not a project's: a `PUT` writes their ConfigMap directly, as
//! the person (see the `settings` module).
//!
//! # Compatibility
//!
//! The web interface is embedded in the same binary as the API, so the two always ship together.
//! Once the API is promised to other clients, it changes only by addition [aip-180]
//! [k8s-api-changes]; until then a change that breaks a client is marked `BREAKING CHANGE:` in
//! its commit.
//!
//! # References
//!
//! - [aip-121] Google, "AIP-121: Resource-oriented design." <https://google.aip.dev/121>
//! - [aip-180] Google, "AIP-180: Backwards compatibility." <https://google.aip.dev/180>
//! - [k8s-api-changes] Kubernetes SIG Architecture, "Changing the API," kubernetes/community. <https://github.com/kubernetes/community/blob/master/contributors/devel/sig-architecture/api_changes.md>
//!
//! [aip-121]: https://google.aip.dev/121
//! [aip-180]: https://google.aip.dev/180
//! [k8s-api-changes]: https://github.com/kubernetes/community/blob/master/contributors/devel/sig-architecture/api_changes.md

pub mod access;
pub mod account;
pub mod alerts;
mod cache;
pub mod cluster;
mod error;
pub mod gaps;
mod handlers;
pub mod identity;
pub mod incidents;
pub mod local;
pub mod members;
pub mod plan_token;
pub mod pods;
pub mod projects;
mod query;
pub mod resolve;
pub mod resources;
pub mod services;
pub mod settings;
pub mod timeline;
pub mod tools;
pub mod views;

use std::sync::Arc;

use axum::Router;
use axum::routing::get;
pub use error::ApiError;
pub use handlers::AppState;
pub use settings::Settings;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::access::ClusterAccess;
use crate::plan_token::PlanKey;

/// The description of the API that is not attached to a route.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Zeabur API",
        description = "Kubernetes made simple and secure by default. Every request acts as the \
person making it; errors are RFC 9457 problems that keep kube-apiserver's own answer.

**Shapes.** A get returns the resource itself, with `unreadable` naming the kinds the person may \
not list. A list returns `{items}` with every item; events take a `limit` (100 by default, 500 \
at most) and say in `totalSize` how many matched. Every value names the Kubernetes object it came \
from (`origin`), every top-level node has an `id` in the shape of its path, and times are RFC \
3339.

**Caching.** Every answer carries `Cache-Control: no-store`.

**Consistency.** A project is read with one consistent LIST per kind, issued concurrently. Kinds \
are not read at one shared resourceVersion, which Kubernetes does not offer across kinds, so a \
response can briefly show, for example, a pod that a just-deleted workload owned; the next read \
corrects it. Zeabur promises freshness (nothing older than the request), not a single-revision \
snapshot.

**Errors.** The problem `kind` and `reason` are open enumerations: expect new values, treat an \
unknown `kind` by its HTTP status and ignore an unknown `reason`. `429` and `503` carry \
kube-apiserver's `Retry-After` when it gave one. Where admin rights are offered and not in use \
(`/api/v1/session`), a `403` from kube-apiserver comes as `elevation-required`, with the same \
`denied` and `kubernetes`; while they are in use, every request goes with them.

**Compatibility.** The embedded web interface ships with the API; other clients are not yet \
promised a stable `/api/v1`.",
        license(name = "Apache-2.0", identifier = "Apache-2.0")
    ),
    tags(
        (name = "identity", description = "Who you are and what you may do"),
        (name = "members", description = "Everyone Zeabur Identity signs in: passkeys, setup links, removals"),
        (name = "cluster", description = "The cluster and what it has installed"),
        (name = "projects", description = "Projects (namespaces), their events, and changes: plan, then apply"),
        (name = "services", description = "A project's services (workloads): releases and usage history"),
        (name = "pods", description = "A pod's containers: logs, a terminal over a WebSocket, and their files"),
        (name = "resources", description = "A project's resources and objects of any kind, as YAML"),
        (name = "settings", description = "Zeabur's own settings"),
    ),
    components(schemas(pods::exec::ClientFrame, pods::exec::ServerFrame))
)]
struct ApiDoc;

fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(identity::whoami))
        .routes(routes!(identity::get))
        .routes(routes!(cluster::get))
        .routes(routes!(cluster::capabilities))
        .routes(routes!(cluster::config_reload::get))
        .routes(routes!(cluster::install::guide_for))
        .routes(routes!(cluster::nodes))
        .routes(routes!(cluster::health))
        .routes(routes!(cluster::upgrades))
        .routes(routes!(cluster::events))
        .routes(routes!(alerts::cluster))
        .routes(routes!(gaps::cluster))
        .routes(routes!(incidents::cluster))
        .routes(routes!(projects::list))
        .routes(routes!(projects::get))
        .routes(routes!(projects::namespace::get))
        .routes(routes!(services::service))
        .routes(routes!(gaps::project))
        .routes(routes!(incidents::project))
        .routes(routes!(resources::objects::list))
        .routes(routes!(resources::objects::get))
        .routes(routes!(resources::creatable::list))
        .routes(routes!(resources::config::config_maps))
        .routes(routes!(resources::config::reveal_secret))
        .routes(routes!(projects::events::events))
        .routes(routes!(pods::logs::logs))
        .routes(routes!(pods::logs::follow_logs))
        .routes(routes!(pods::exec::exec))
        .routes(routes!(pods::debug::get))
        .routes(routes!(pods::files::get))
        .routes(routes!(pods::files::list, pods::files::delete))
        .routes(routes!(pods::files::download, pods::files::upload))
        .routes(routes!(pods::files::preview))
        .routes(routes!(pods::files::make_folder))
        .routes(routes!(pods::files::rename))
        .routes(routes!(tools::status))
        .routes(routes!(tools::delete))
        .routes(routes!(tools::terminal))
        .routes(routes!(services::releases::releases))
        .routes(routes!(services::trace::trace))
        .routes(routes!(timeline::service))
        .routes(routes!(services::rollout::rollout))
        .routes(routes!(services::evidence::evidence))
        .routes(routes!(timeline::project))
        .routes(routes!(projects::writes::plan))
        .routes(routes!(projects::writes::apply))
        .routes(routes!(projects::backups::get))
        .routes(routes!(alerts::project))
        .routes(routes!(projects::databases::get))
        .routes(routes!(projects::builds::get))
        .routes(routes!(projects::deploy_keys::create))
        .routes(routes!(projects::network_policy::presets))
        .routes(routes!(projects::usage::get))
        .routes(routes!(projects::usage::volumes))
        .routes(routes!(services::history::get))
        .routes(routes!(services::logs::history))
        .routes(routes!(settings::store::get_setting, settings::store::set))
        .routes(routes!(
            settings::cluster_name::get_setting,
            settings::cluster_name::set
        ))
        .routes(routes!(account::get))
        .routes(routes!(members::handlers::list))
        .routes(routes!(members::plan::plan))
        .routes(routes!(members::plan::apply))
        .routes(routes!(members::handlers::revoke_invitation))
        .routes(routes!(members::handlers::setup_link))
        .routes(routes!(members::handlers::remove_passkey))
}

/// The API, for `access`; mount it next to the web interface.
///
/// Every answer carries `Cache-Control: no-store` (see the `cache` module).
pub fn router(access: Arc<dyn ClusterAccess>) -> Router {
    router_at(access, zeabur_kube::k8s_openapi::jiff::Timestamp::now)
}

/// [`router`] for the in-cluster server, with its `settings`.
///
/// Plan tokens are sealed with `plan_key` instead of a key made at start, so that they outlive a
/// restart, `sign_in` finds the issuer's page where people manage how they sign in, and
/// `log_proxy` reaches Zeabur Proxy for log history, when it is installed.
pub fn router_with_key(
    access: Arc<dyn ClusterAccess>,
    plan_key: PlanKey,
    settings: Settings,
    sign_in: Arc<dyn account::SignInPage>,
    log_proxy: Option<Arc<dyn services::logs::LogProxy>>,
) -> Router {
    build_with(
        access,
        zeabur_kube::k8s_openapi::jiff::Timestamp::now,
        Some(Arc::new(plan_key)),
        pods::exec::IDLE,
        settings,
        Arc::new(resolve::System),
        sign_in,
        log_proxy,
    )
}

/// [`router_at`], reaching Zeabur Proxy through `log_proxy`; for tests.
pub fn router_with_log_proxy(
    access: Arc<dyn ClusterAccess>,
    clock: fn() -> zeabur_kube::k8s_openapi::jiff::Timestamp,
    log_proxy: Arc<dyn services::logs::LogProxy>,
) -> Router {
    build_with(
        access,
        clock,
        PlanKey::random().ok().map(Arc::new),
        pods::exec::IDLE,
        Settings::default(),
        Arc::new(resolve::System),
        Arc::new(account::Fixed(None)),
        Some(log_proxy),
    )
}

/// [`router`] with `settings` other than the defaults.
pub fn router_with_settings(access: Arc<dyn ClusterAccess>, settings: Settings) -> Router {
    build(
        access,
        zeabur_kube::k8s_openapi::jiff::Timestamp::now,
        PlanKey::random().ok().map(Arc::new),
        pods::exec::IDLE,
        settings,
    )
}

/// [`router`], reading the time from `clock` instead of the system clock; for tests.
pub fn router_at(
    access: Arc<dyn ClusterAccess>,
    clock: fn() -> zeabur_kube::k8s_openapi::jiff::Timestamp,
) -> Router {
    build(
        access,
        clock,
        PlanKey::random().ok().map(Arc::new),
        pods::exec::IDLE,
        Settings::default(),
    )
}

/// [`router_at`], closing a terminal after `terminal_idle` without a byte instead of after
/// [`pods::exec::IDLE`]; for tests.
pub fn router_with_terminal_idle(
    access: Arc<dyn ClusterAccess>,
    clock: fn() -> zeabur_kube::k8s_openapi::jiff::Timestamp,
    terminal_idle: std::time::Duration,
) -> Router {
    build(
        access,
        clock,
        PlanKey::random().ok().map(Arc::new),
        terminal_idle,
        Settings::default(),
    )
}

/// [`router`], looking domains up with `resolver` instead of the system's resolver; for tests.
pub fn router_with_resolver(
    access: Arc<dyn ClusterAccess>,
    resolver: Arc<dyn resolve::Resolve>,
) -> Router {
    router_with_settings_and_resolver(access, Settings::default(), resolver)
}

/// [`router_with_settings`], looking domains up with `resolver`; for tests and the demo.
pub fn router_with_settings_and_resolver(
    access: Arc<dyn ClusterAccess>,
    settings: Settings,
    resolver: Arc<dyn resolve::Resolve>,
) -> Router {
    build_with(
        access,
        zeabur_kube::k8s_openapi::jiff::Timestamp::now,
        PlanKey::random().ok().map(Arc::new),
        pods::exec::IDLE,
        settings,
        resolver,
        Arc::new(account::Fixed(None)),
        None,
    )
}

fn build(
    access: Arc<dyn ClusterAccess>,
    clock: fn() -> zeabur_kube::k8s_openapi::jiff::Timestamp,
    plan_key: Option<Arc<PlanKey>>,
    terminal_idle: std::time::Duration,
    settings: Settings,
) -> Router {
    build_with(
        access,
        clock,
        plan_key,
        terminal_idle,
        settings,
        Arc::new(resolve::System),
        Arc::new(account::Fixed(None)),
        None,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "every router above passes all of AppState's parts"
)]
fn build_with(
    access: Arc<dyn ClusterAccess>,
    clock: fn() -> zeabur_kube::k8s_openapi::jiff::Timestamp,
    plan_key: Option<Arc<PlanKey>>,
    terminal_idle: std::time::Duration,
    settings: Settings,
    resolver: Arc<dyn resolve::Resolve>,
    sign_in: Arc<dyn account::SignInPage>,
    log_proxy: Option<Arc<dyn services::logs::LogProxy>>,
) -> Router {
    let catalogue = Arc::new(zeabur_catalog::catalogue().unwrap_or_default());
    let state = AppState {
        access,
        catalogue,
        plan_key,
        clock,
        terminal_idle,
        settings: Arc::new(settings),
        resolver,
        tools: Arc::new(tools::Sessions::default()),
        sign_in,
        log_proxy,
    };
    let (router, api) = routes().split_for_parts();
    let spec = Arc::new(serde_json::to_string(&api).unwrap_or_default());
    router
        .route(
            "/api/openapi.json",
            get(move || {
                let spec = Arc::clone(&spec);
                async move {
                    (
                        [(axum::http::header::CONTENT_TYPE, "application/json")],
                        spec.as_str().to_owned(),
                    )
                }
            }),
        )
        .route("/api/{*rest}", axum::routing::any(handlers::not_found))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            error::offer_elevation_on_refusal,
        ))
        .layer(axum::middleware::from_fn(cache::no_store))
        .with_state(state)
}

/// The OpenAPI document. `crates/zeabur-api/openapi.yaml` is this, written as YAML by a test and
/// committed; the server itself serves it as JSON at `/api/openapi.json`.
#[must_use]
pub fn openapi() -> utoipa::openapi::OpenApi {
    routes().split_for_parts().1
}
