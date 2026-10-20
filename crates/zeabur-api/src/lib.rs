#![doc = include_str!("../README.md")]
//! # Paths
//!
//! Paths follow the resource hierarchy [aip-121], with lower-case kinds (any case is accepted)
//! and two segments for kind and name, which keep names with dots unambiguous:
//!
//! | Method and path | Returns |
//! |---|---|
//! | `GET /api/v1/whoami` | who the cluster says you are |
//! | `GET /api/v1/session` | how you are signed in to zeabur: signing out, admin rights in use |
//! | `GET /api/v1/cluster` | the cluster: its Kubernetes version and distribution |
//! | `GET /api/v1/cluster/upgrades` | end of life, version skew and newer versions from Plans |
//! | `GET /api/v1/cluster/nodes` | nodes, with requests against allocatable |
//! | `GET /api/v1/cluster/events` | events of every namespace, newest first, `limit` of them |
//! | `GET /api/v1/cluster/gaps` | what the cluster itself is missing, with fixes |
//! | `GET /api/v1/cluster/capabilities` | what is installed (with versions), and what to install |
//! | `GET /api/v1/cluster/components/{capability}/{component}/install` | how to install it yourself, in four forms |
//! | `GET /api/v1/settings/metrics-store` | the metrics store history is read from, whether it answers, who may change it |
//! | `PUT /api/v1/settings/metrics-store` | changes it, or goes back to the default |
//! | `GET /api/v1/projects` | the projects, with the health of adopted ones |
//! | `GET /api/v1/projects/{p}` | the project document |
//! | `GET /api/v1/projects/{p}/namespace` | its Namespace: adopted or not, its details, whether you may change them |
//! | `GET /api/v1/projects/{p}/services/{kind}/{name}` | one service, cut from the document |
//! | `GET /api/v1/projects/{p}/services/{kind}/{name}/releases` | its releases, newest first |
//! | `GET /api/v1/projects/{p}/services/{kind}/{name}/history` | its pods' CPU, memory and restarts over a range, from the metrics store |
//! | `GET /api/v1/projects/{p}/usage` | what each pod uses now, from metrics-server |
//! | `GET /api/v1/projects/{p}/gaps` | what the project is missing, with citations and fixes |
//! | `GET /api/v1/projects/{p}/objects` | every object of every namespaced kind, by kind |
//! | `GET /api/v1/projects/{p}/objects/{group}/{version}/{resource}/{name}` | one object as YAML (`core` for the empty group) |
//! | `GET /api/v1/projects/{p}/configmaps` | the ConfigMaps, in full |
//! | `POST /api/v1/projects/{p}/secrets/{name}/reveal` | a Secret's values |
//! | `GET /api/v1/projects/{p}/events` | events, newest first, `limit` of them |
//! | `GET /api/v1/projects/{p}/pods/{pod}/logs` | a log snapshot |
//! | `GET /api/v1/projects/{p}/pods/{pod}/logs/follow` | a log as Server-Sent Events |
//! | `GET /api/v1/projects/{p}/pods/{pod}/exec` | a terminal in a container, over a WebSocket (see the `pods::exec` module) |
//! | `POST /api/v1/projects/{p}/plan` | the writes a change needs, and a token; writes nothing |
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
mod cache;
pub mod cluster;
mod error;
pub mod gaps;
mod handlers;
pub mod identity;
pub mod local;
pub mod pods;
pub mod projects;
mod query;
pub mod resources;
pub mod services;
pub mod settings;
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
use zeabur_plan::token::PlanKey;

use crate::access::ClusterAccess;

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
        (name = "identity", description = "Who you are"),
        (name = "cluster", description = "The cluster and what it has installed"),
        (name = "projects", description = "Projects (namespaces), their events, and changes: plan, then apply"),
        (name = "services", description = "A project's services (workloads): releases and usage history"),
        (name = "pods", description = "A pod's containers: logs, and a terminal over a WebSocket"),
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
        .routes(routes!(cluster::install::guide_for))
        .routes(routes!(cluster::nodes))
        .routes(routes!(cluster::upgrades))
        .routes(routes!(cluster::events))
        .routes(routes!(gaps::cluster))
        .routes(routes!(projects::list))
        .routes(routes!(projects::get))
        .routes(routes!(projects::namespace::get))
        .routes(routes!(services::service))
        .routes(routes!(gaps::project))
        .routes(routes!(resources::objects::list))
        .routes(routes!(resources::objects::get))
        .routes(routes!(resources::config::config_maps))
        .routes(routes!(resources::config::reveal_secret))
        .routes(routes!(projects::events::events))
        .routes(routes!(pods::logs::logs))
        .routes(routes!(pods::logs::follow_logs))
        .routes(routes!(pods::exec::exec))
        .routes(routes!(services::releases::releases))
        .routes(routes!(projects::writes::plan))
        .routes(routes!(projects::writes::apply))
        .routes(routes!(projects::backups::get))
        .routes(routes!(projects::network_policy::presets))
        .routes(routes!(projects::usage::get))
        .routes(routes!(services::history::get))
        .routes(routes!(settings::store::get_setting, settings::store::set))
}

/// The API, for `access`; mount it next to the web interface.
///
/// Every answer carries `Cache-Control: no-store` (see the `cache` module).
pub fn router(access: Arc<dyn ClusterAccess>) -> Router {
    router_at(access, zeabur_kube::k8s_openapi::jiff::Timestamp::now)
}

/// [`router`], sealing plan tokens with `plan_key` instead of a key made at start, so that
/// tokens outlive a restart of the in-cluster server, and with the server's `settings`.
pub fn router_with_key(
    access: Arc<dyn ClusterAccess>,
    plan_key: PlanKey,
    settings: Settings,
) -> Router {
    build(
        access,
        zeabur_kube::k8s_openapi::jiff::Timestamp::now,
        Some(Arc::new(plan_key)),
        pods::exec::IDLE,
        settings,
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

fn build(
    access: Arc<dyn ClusterAccess>,
    clock: fn() -> zeabur_kube::k8s_openapi::jiff::Timestamp,
    plan_key: Option<Arc<PlanKey>>,
    terminal_idle: std::time::Duration,
    settings: Settings,
) -> Router {
    let catalogue = Arc::new(zeabur_catalog::catalogue().unwrap_or_default());
    let state = AppState {
        access,
        catalogue,
        plan_key,
        clock,
        terminal_idle,
        settings: Arc::new(settings),
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
