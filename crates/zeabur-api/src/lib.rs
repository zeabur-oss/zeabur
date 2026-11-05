#![doc = include_str!("../README.md")]
//! # Paths
//!
//! Paths follow the resource hierarchy [aip-121], with lower-case kinds (any case is accepted)
//! and two segments for kind and name, which keep names with dots unambiguous:
//!
//! | Method and path | Returns |
//! |---|---|
//! | `GET /api/v1/whoami` | who the cluster says you are |
//! | `GET /api/v1/session` | how you are signed in to Zeabur, and whether you can sign out |
//! | `GET /api/v1/cluster` | the cluster: its Kubernetes version and distribution |
//! | `GET /api/v1/cluster/upgrades` | end of life, version skew and newer versions from Plans |
//! | `GET /api/v1/cluster/nodes` | nodes, with requests against allocatable |
//! | `GET /api/v1/cluster/usage` | what each namespace's pods reserve, may use at most and use now |
//! | `GET /api/v1/cluster/events` | events of every namespace, newest first, `limit` of them |
//! | `GET /api/v1/cluster/gaps` | what the cluster itself is missing, with fixes |
//! | `GET /api/v1/cluster/incidents` | components down and nodes not ready, with the domains and projects they reach |
//! | `GET /api/v1/cluster/node-shells` | the node shell pods (cluster administrators only) |
//! | `GET /api/v1/cluster/node-shells/{pod}/terminal` | a root shell on the node, over a WebSocket, for whoever opened it |
//! | `GET /api/v1/cluster/tools` | your cluster tools pods, for administrators; deletes those that have stopped (see `cluster::tools`) |
//! | `GET /api/v1/cluster/tools/{name}/terminal` | a terminal in it, `kubectl` and `helm` acting as you, over a WebSocket |
//! | `GET /api/v1/cluster/backups` | Velero's backup locations (and whether Velero reaches them), schedules and newest backups, or that Velero is not installed |
//! | `GET /api/v1/cluster/capabilities` | what is installed (with versions), and what to install |
//! | `GET /api/v1/cluster/templates` | the catalogue's templates: what each asks for and requests, and whether its components are there |
//! | `GET /api/v1/cluster/templates/{template}` | one of them, with what its pods use when idle |
//! | `GET /api/v1/settings/metrics-store` | the metrics store history is read from, whether it answers, who may change it |
//! | `PUT /api/v1/settings/metrics-store` | changes it, or goes back to the default |
//! | `GET /api/v1/settings/sign-in` | how admin accounts sign in (Zeabur Identity's teams and AAL3 settings), who may change it |
//! | `PUT /api/v1/settings/sign-in` | changes it |
//! | `GET /api/v1/projects` | the projects, with the health of adopted ones |
//! | `GET /api/v1/projects/{p}` | the project document |
//! | `GET /api/v1/projects/{p}/namespace` | its Namespace: adopted or not, its details, whether you may change them |
//! | `GET /api/v1/projects/{p}/services/{kind}/{name}` | one service, cut from the document |
//! | `GET /api/v1/projects/{p}/services/{kind}/{name}/releases` | its releases, newest first |
//! | `GET /api/v1/projects/{p}/services/{kind}/{name}/trace` | every way to it, hop by hop from DNS to a ready pod, and where each stops |
//! | `GET /api/v1/projects/{p}/services/{kind}/{name}/rollout` | how far its rollout has come: new and old pods, done, progressing or stuck and why, with the plan that rolls back |
//! | `GET /api/v1/projects/{p}/services/cronjob/{name}/runs` | a CronJob's runs, newest first, with their pods |
//! | `GET /api/v1/projects/{p}/services/{kind}/{name}/evidence` | its problems, trace, events, previous logs and objects as Markdown to copy; never a Secret or a variable's value |
//! | `GET /api/v1/projects/{p}/services/{kind}/{name}/timeline` | what changed in it and what went wrong, newest first, with the plan that reverts its running release |
//! | `GET /api/v1/projects/{p}/services/{kind}/{name}/history` | its pods' CPU, memory and restarts over a range, from the metrics store |
//! | `GET /api/v1/projects/{p}/usage` | what each pod uses now, from metrics-server |
//! | `GET /api/v1/projects/{p}/volumes` | how full each claim's volume is, from the kubelets |
//! | `GET /api/v1/projects/{p}/builds` | its Zeabur Builder builds and their runs, with each run's pod |
//! | `POST /api/v1/projects/{p}/builds/{b}/source` | pushes an uploaded folder (a gzip tar, up to 256 MiB) to the registry for a Build that builds one, and answers the digest to name in it; the plan (`uploadToken`, then `archiveDigest`) is the write |
//! | `GET /api/v1/projects/{p}/notifications` | where the project's alerts go: its channels and the events each receives, or that alerting is not installed |
//! | `POST /api/v1/projects/{p}/notifications/plan` | what adding, changing, testing or removing a channel writes, and a token; writes nothing and never shows a secret |
//! | `POST /api/v1/projects/{p}/notifications/apply` | carries out that plan by its token |
//! | `GET /api/v1/projects/{p}/gaps` | what the project is missing, with citations and fixes |
//! | `GET /api/v1/projects/{p}/timeline` | the same for every service of the project |
//! | `GET /api/v1/projects/{p}/incidents` | the cluster incidents that reach the project, with the services they break |
//! | `GET /api/v1/projects/{p}/objects` | every object of every namespaced kind, by kind |
//! | `GET /api/v1/projects/{p}/objects/{group}/{version}/{resource}/{name}` | one object as YAML (`core` for the empty group) |
//! | `GET /api/v1/projects/{p}/configmaps` | the ConfigMaps, in full |
//! | `POST /api/v1/projects/{p}/secrets/{name}/reveal` | a Secret's values |
//! | `GET /api/v1/projects/{p}/databases/{engine}/{name}/export` | a database's dump, streamed from its own dump tool through your `pods/exec` |
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
//! | `POST /api/v1/cluster/backups/plan` | what setting the backup location or schedule, pausing it or backing up now writes, and a token; writes nothing and never shows the keys |
//! | `POST /api/v1/cluster/backups/apply` | carries out that plan by its token |
//! | `GET /api/v1/cluster/notifications` | the cluster's notification channels and whether the alert rules are installed, or that alerting is not installed |
//! | `POST /api/v1/cluster/notifications/plan` | what adding, changing, testing or removing a cluster channel writes, and a token; writes nothing and never shows a secret |
//! | `POST /api/v1/cluster/notifications/apply` | carries out that plan by its token |
//! | `POST /api/v1/cluster/node-shells/plan` | what opening a shell on a node, or removing a shell pod, writes, and a token; writes nothing |
//! | `POST /api/v1/cluster/node-shells/apply` | carries out that plan by its token |
//! | `POST /api/v1/cluster/tools/plan` | what starting a kubectl terminal for the whole cluster writes, and a token; writes nothing |
//! | `POST /api/v1/cluster/tools/apply` | carries out that plan by its token |
//! | `POST /api/v1/cluster/nodes/{node}/maintenance/plan` | what stopping or resuming scheduling does to a node, and a token; writes nothing |
//! | `POST /api/v1/cluster/nodes/{node}/maintenance/apply` | carries out that plan by its token: patches the node |
//! | `DELETE /api/v1/cluster/tools/{name}` | deletes your cluster tools pod |
//! | `DELETE /api/v1/members/invitations/{name}` | revokes a setup link |
//! | `POST /api/v1/members/{username}/setup-links` | a new setup link for a member, optionally removing their passkeys first |
//! | `DELETE /api/v1/members/{username}/credentials/{id}` | removes a member's passkey or linked account |
//! | `POST /api/v1/projects/{p}/images/inspect` | an image's declared ports and user, read from its registry anonymously or with the project's pull Secret, read as you; `unreadable` when the registry cannot be read |
//! | `POST /api/v1/projects/{p}/compose` | the services a Docker Compose file makes, with the fields left out and why; reads and writes nothing in the cluster |
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
mod github;
mod handlers;
pub mod identity;
pub mod images;
pub mod incidents;
pub mod local;
pub mod members;
pub mod notifications;
pub mod page;
pub mod plan_token;
pub mod pods;
pub mod projects;
mod query;
pub mod releases;
pub mod resolve;
pub mod resources;
pub mod services;
pub mod settings;
pub mod timeline;
pub mod tools;
pub mod used;
pub mod views;

use std::sync::Arc;

use axum::Router;
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
kube-apiserver's `Retry-After` when it gave one.

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

// The server registers each handler from the path and methods of its `#[utoipa::path]` and nothing
// else. utoipa-axum's `routes!` also builds every operation and schema, so with it the server built
// the whole document at start and kept all the schema code in the binary (#76). Only `openapi()`
// builds the document now, for the test that writes `openapi.yaml`.
//
// `described!` names the struct `#[utoipa::path]` generates for a handler, `__path_<handler>`, the
// way utoipa-axum's own `routes!` does; utoipa does not document that name. If a utoipa upgrade
// changes it, `routes()` stops compiling: see how the new `utoipa_axum::routes!` reaches the
// `utoipa::Path` impl and do the same in `described!`. Until then, `openapi_router()
// .split_for_parts().0` is a working `routes()`, at the cost of the size and memory above.
macro_rules! handlers {
    ($then:ident) => {
        $then! {
            identity::whoami,
            identity::get,
            cluster::get,
            cluster::zeabur::get,
            cluster::update::get,
            cluster::capabilities,
            cluster::backups::get,
            cluster::backups::plan,
            cluster::backups::apply,
            cluster::backup_download::download,
            cluster::restore_copy::deleted_projects,
            cluster::issuers::get,
            cluster::s3_destinations::get,
            cluster::s3_destinations::plan,
            cluster::s3_destinations::apply,
            cluster::github_app::get,
            cluster::github_app::manifest,
            cluster::github_app::form,
            cluster::github_app::conversion,
            cluster::github_app::plan,
            cluster::github_app::apply,
            notifications::cluster_get,
            notifications::cluster_plan,
            notifications::cluster_apply,
            cluster::config_reload::get,
            cluster::templates::list,
            cluster::templates::get,
            cluster::node_shells::list,
            cluster::node_shells::plan,
            cluster::node_shells::apply,
            cluster::node_shells::terminal,
            cluster::tools::status,
            cluster::tools::plan,
            cluster::tools::apply,
            cluster::tools::delete,
            cluster::tools::terminal,
            cluster::maintenance::plan,
            cluster::maintenance::apply,
            cluster::nodes,
            cluster::usage::get,
            cluster::health,
            cluster::upgrades,
            cluster::events,
            alerts::cluster,
            gaps::cluster,
            incidents::cluster,
            projects::list,
            projects::get,
            projects::namespace::get,
            projects::gateways::list,
            services::service,
            gaps::project,
            incidents::project,
            resources::objects::list,
            resources::objects::get,
            resources::creatable::list,
            resources::config::config_maps,
            resources::config::reveal_secret,
            projects::events::events,
            pods::logs::logs,
            pods::logs::follow_logs,
            pods::exec::exec,
            pods::debug::get,
            pods::files::get,
            pods::files::list,
            pods::files::delete,
            pods::files::download,
            pods::files::upload,
            pods::files::preview,
            pods::files::make_folder,
            pods::files::rename,
            tools::status,
            tools::delete,
            tools::terminal,
            services::releases::releases,
            services::trace::trace,
            timeline::service,
            services::rollout::rollout,
            services::runs::runs,
            services::evidence::evidence,
            timeline::project,
            projects::writes::plan,
            projects::writes::apply,
            projects::writes::certificates::get,
            projects::backups::get,
            notifications::project_get,
            notifications::project_plan,
            notifications::project_apply,
            alerts::project,
            projects::databases::get,
            projects::object_storage::get,
            projects::database_export::export,
            projects::volume_restores::get,
            projects::builds::get,
            projects::source_upload::upload,
            projects::deploy_keys::create,
            cluster::github_app::project,
            images::inspect,
            projects::compose::convert_compose,
            projects::deploy_keys::public,
            projects::network_policy::presets,
            projects::usage::get,
            projects::usage::volumes,
            projects::storage::get,
            services::history::get,
            services::logs::history,
            settings::store::get_setting,
            settings::store::set,
            settings::cluster_name::get_setting,
            settings::cluster_name::set,
            settings::domains::get_setting,
            settings::domains::set,
            settings::sign_in::get_setting,
            settings::sign_in::set,
            account::get,
            members::handlers::list,
            members::plan::plan,
            members::plan::apply,
            members::handlers::revoke_invitation,
            members::handlers::setup_link,
            members::handlers::remove_passkey,
        }
    };
}

macro_rules! described {
    (@ [$($module:ident)*] $segment:ident :: $($rest:tt)+) => {
        described!(@ [$($module)* $segment] $($rest)+)
    };
    (@ [$($module:ident)*] $handler:ident) => {
        pastey::paste!($($module::)* [<__path_ $handler>])
    };
    ($($path:tt)+) => {
        described!(@ [] $($path)+)
    };
}

fn routes() -> Router<AppState> {
    macro_rules! serve {
        ($($($segment:ident)::+),+ $(,)?) => {{
            let router = Router::new();
            $(let router = route::<described!($($segment)::+), _, _>(router, $($segment)::+);)+
            router
        }};
    }
    handlers!(serve)
}

fn route<P, H, T>(router: Router<AppState>, handler: H) -> Router<AppState>
where
    P: utoipa::Path,
    H: axum::handler::Handler<T, AppState>,
    T: 'static,
{
    use utoipa_axum::PathItemExt;
    let filter = P::methods()
        .iter()
        .map(PathItemExt::to_method_filter)
        .reduce(axum::routing::MethodFilter::or);
    match filter {
        Some(filter) => router.route(&P::path(), axum::routing::on(filter, handler)),
        None => router,
    }
}

fn openapi_router() -> OpenApiRouter<AppState> {
    macro_rules! describe {
        ($($($segment:ident)::+),+ $(,)?) => {{
            let router = OpenApiRouter::with_openapi(ApiDoc::openapi());
            // One closure per handler keeps each description's stack frame apart.
            $(let router = (|router: OpenApiRouter<AppState>| router.routes(routes!($($segment)::+)))(router);)+
            router
        }};
    }
    handlers!(describe)
}

/// What [`router_with`] builds the API with.
///
/// [`Options::default`] is what [`router`] uses: the system clock and resolver, a plan key made at
/// start, no Zeabur Proxy and no terminal namespace, with registries read at any address.
pub struct Options {
    /// Zeabur's settings (see [`settings`]).
    pub settings: Settings,
    /// The time now.
    pub clock: fn() -> zeabur_kube::k8s_openapi::jiff::Timestamp,
    /// The key plan tokens are sealed with; a key read from a file outlives a restart.
    pub plan_key: Option<Arc<PlanKey>>,
    /// How long a terminal may idle before the server closes it.
    pub terminal_idle: std::time::Duration,
    /// How domains are looked up for a route's trace.
    pub resolver: Arc<dyn resolve::Resolve>,
    /// Where the issuer lets people manage how they sign in.
    pub sign_in: Arc<dyn account::SignInPage>,
    /// Zeabur Proxy, for log and usage history, when it is installed.
    pub zeabur_proxy: Option<Arc<dyn services::proxy::ZeaburProxy>>,
    /// Reads images' configurations; `None` connects to registries at `reach`, trusting
    /// `settings.trusted_ca`.
    pub images: Option<Arc<images::Inspector>>,
    /// What a template uses when idle.
    pub template_idle: fn(&str) -> Option<zeabur_catalog::IdleUse>,
    /// Which addresses registries, buckets and GitHub may be reached at.
    pub reach: images::https::Reach,
    /// Zeabur's own service account in the terminal namespace (see [`tools`]).
    pub terminals: Option<Arc<dyn zeabur_kube::Cluster>>,
    /// Receives each feature a successful write used, for the experience program.
    pub used: used::Used,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            clock: zeabur_kube::k8s_openapi::jiff::Timestamp::now,
            plan_key: PlanKey::random().ok().map(Arc::new),
            terminal_idle: pods::exec::IDLE,
            resolver: Arc::new(resolve::System),
            sign_in: Arc::new(account::Fixed(None)),
            zeabur_proxy: None,
            images: None,
            template_idle: zeabur_catalog::template_idle,
            reach: images::https::Reach::Any,
            terminals: None,
            used: Arc::new(|_| {}),
        }
    }
}

impl std::fmt::Debug for Options {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Options").finish_non_exhaustive()
    }
}

/// The API, for `access`; mount it next to the web interface.
///
/// Every answer carries `Cache-Control: no-store` (see the `cache` module).
pub fn router(access: Arc<dyn ClusterAccess>) -> Router {
    router_with(access, Options::default())
}

/// [`router`] with `options`.
pub fn router_with(access: Arc<dyn ClusterAccess>, options: Options) -> Router {
    let Options {
        settings,
        clock,
        plan_key,
        terminal_idle,
        resolver,
        sign_in,
        zeabur_proxy,
        images,
        template_idle,
        reach,
        terminals,
        used,
    } = options;
    let catalogue = Arc::new(zeabur_catalog::catalogue().unwrap_or_default());
    let tls = images::https::tls(settings.trusted_ca.as_deref());
    let images = images.unwrap_or_else(|| Arc::new(images::Inspector::connected(reach, &tls)));
    let github = tls
        .as_ref()
        .ok()
        .map(|tls| images::https::Https::new(reach, tls));
    let state = AppState {
        access,
        catalogue,
        template_idle,
        plan_key,
        clock,
        terminal_idle,
        settings: Arc::new(settings),
        resolver,
        tools: Arc::new(tools::Sessions::default()),
        sign_in,
        zeabur_proxy,
        pusher: Arc::new(projects::oci_push::Pusher::new(reach, &tls)),
        store: Arc::new(cluster::object_store::Fetcher::new(images.reach(), &tls)),
        images,
        github,
        releases: Arc::new(cluster::update::Releases::default()),
        terminals,
        used,
    };
    routes()
        .route("/api/{*rest}", axum::routing::any(handlers::not_found))
        .layer(axum::middleware::from_fn(cache::no_store))
        .with_state(state)
}

/// The OpenAPI document. A test writes it to `crates/zeabur-api/openapi.yaml`, which is committed;
/// the server does not serve it.
#[must_use]
pub fn openapi() -> utoipa::openapi::OpenApi {
    openapi_router().split_for_parts().1
}
