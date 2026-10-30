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

/// Every route once: its method, its path and its handler. The server's [`Router`] registers them
/// with axum, and [`openapi`] builds the document from the same handlers; a test checks that the
/// two agree on method and path.
macro_rules! route_table {
    ($($method:ident $path:literal => $($handler:ident)::+,)+) => {
        fn routes() -> Router<AppState> {
            Router::new()$(.route($path, axum::routing::$method($($handler)::+)))+
        }

        /// The OpenAPI document. `crates/zeabur-api/openapi.yaml` and `openapi.json` are this,
        /// written by a test and committed; the server serves the JSON without building it.
        #[must_use]
        pub fn openapi() -> utoipa::openapi::OpenApi {
            OpenApiRouter::<AppState>::with_openapi(ApiDoc::openapi())
                $(.routes(utoipa_axum::routes!($($handler)::+)))+
                .split_for_parts()
                .1
        }

        #[cfg(test)]
        const ROUTES: &[(&str, &str)] = &[$((stringify!($method), $path)),+];
    };
}

route_table! {
    get "/api/v1/whoami" => identity::whoami,
    get "/api/v1/session" => identity::get,
    get "/api/v1/cluster" => cluster::get,
    get "/api/v1/cluster/capabilities" => cluster::capabilities,
    get "/api/v1/cluster/backups" => cluster::backups::get,
    post "/api/v1/cluster/backups/plan" => cluster::backups::plan,
    post "/api/v1/cluster/backups/apply" => cluster::backups::apply,
    get "/api/v1/cluster/backups/{backup}/download" => cluster::backup_download::download,
    get "/api/v1/cluster/backups/{backup}/deleted-projects" => cluster::restore_copy::deleted_projects,
    get "/api/v1/cluster/issuers" => cluster::issuers::get,
    get "/api/v1/cluster/s3-destinations" => cluster::s3_destinations::get,
    post "/api/v1/cluster/s3-destinations/plan" => cluster::s3_destinations::plan,
    post "/api/v1/cluster/s3-destinations/apply" => cluster::s3_destinations::apply,
    get "/api/v1/cluster/github-app" => cluster::github_app::get,
    post "/api/v1/cluster/github-app/manifest" => cluster::github_app::manifest,
    get "/api/v1/cluster/github-app/form" => cluster::github_app::form,
    post "/api/v1/cluster/github-app/conversion" => cluster::github_app::conversion,
    post "/api/v1/cluster/github-app/plan" => cluster::github_app::plan,
    post "/api/v1/cluster/github-app/apply" => cluster::github_app::apply,
    get "/api/v1/cluster/notifications" => notifications::cluster_get,
    post "/api/v1/cluster/notifications/plan" => notifications::cluster_plan,
    post "/api/v1/cluster/notifications/apply" => notifications::cluster_apply,
    get "/api/v1/cluster/config-reload" => cluster::config_reload::get,
    get "/api/v1/cluster/templates" => cluster::templates::list,
    get "/api/v1/cluster/templates/{template}" => cluster::templates::get,
    get "/api/v1/cluster/node-shells" => cluster::node_shells::list,
    post "/api/v1/cluster/node-shells/plan" => cluster::node_shells::plan,
    post "/api/v1/cluster/node-shells/apply" => cluster::node_shells::apply,
    get "/api/v1/cluster/node-shells/{pod}/terminal" => cluster::node_shells::terminal,
    get "/api/v1/cluster/tools" => cluster::tools::status,
    post "/api/v1/cluster/tools/plan" => cluster::tools::plan,
    post "/api/v1/cluster/tools/apply" => cluster::tools::apply,
    delete "/api/v1/cluster/tools/{name}" => cluster::tools::delete,
    get "/api/v1/cluster/tools/{name}/terminal" => cluster::tools::terminal,
    post "/api/v1/cluster/nodes/{node}/maintenance/plan" => cluster::maintenance::plan,
    post "/api/v1/cluster/nodes/{node}/maintenance/apply" => cluster::maintenance::apply,
    get "/api/v1/cluster/nodes" => cluster::nodes,
    get "/api/v1/cluster/usage" => cluster::usage::get,
    get "/api/v1/cluster/health" => cluster::health,
    get "/api/v1/cluster/upgrades" => cluster::upgrades,
    get "/api/v1/cluster/events" => cluster::events,
    get "/api/v1/cluster/alerts" => alerts::cluster,
    get "/api/v1/cluster/gaps" => gaps::cluster,
    get "/api/v1/cluster/incidents" => incidents::cluster,
    get "/api/v1/projects" => projects::list,
    get "/api/v1/projects/{project}" => projects::get,
    get "/api/v1/projects/{project}/namespace" => projects::namespace::get,
    get "/api/v1/projects/{project}/gateways" => projects::gateways::list,
    get "/api/v1/projects/{project}/services/{kind}/{name}" => services::service,
    get "/api/v1/projects/{project}/gaps" => gaps::project,
    get "/api/v1/projects/{project}/incidents" => incidents::project,
    get "/api/v1/projects/{project}/objects" => resources::objects::list,
    get "/api/v1/projects/{project}/objects/{group}/{version}/{resource}/{name}" => resources::objects::get,
    get "/api/v1/projects/{project}/creatable" => resources::creatable::list,
    get "/api/v1/projects/{project}/configmaps" => resources::config::config_maps,
    post "/api/v1/projects/{project}/secrets/{name}/reveal" => resources::config::reveal_secret,
    get "/api/v1/projects/{project}/events" => projects::events::events,
    get "/api/v1/projects/{project}/pods/{pod}/logs" => pods::logs::logs,
    get "/api/v1/projects/{project}/pods/{pod}/logs/follow" => pods::logs::follow_logs,
    get "/api/v1/projects/{project}/pods/{pod}/exec" => pods::exec::exec,
    get "/api/v1/projects/{project}/pods/{pod}/debug" => pods::debug::get,
    get "/api/v1/projects/{project}/pods/{pod}/files" => pods::files::get,
    get "/api/v1/projects/{project}/pods/{pod}/files/entries" => pods::files::list,
    delete "/api/v1/projects/{project}/pods/{pod}/files/entries" => pods::files::delete,
    get "/api/v1/projects/{project}/pods/{pod}/files/content" => pods::files::download,
    put "/api/v1/projects/{project}/pods/{pod}/files/content" => pods::files::upload,
    get "/api/v1/projects/{project}/pods/{pod}/files/preview" => pods::files::preview,
    post "/api/v1/projects/{project}/pods/{pod}/files/folders" => pods::files::make_folder,
    post "/api/v1/projects/{project}/pods/{pod}/files/moves" => pods::files::rename,
    get "/api/v1/projects/{project}/tools" => tools::status,
    delete "/api/v1/projects/{project}/tools/{name}" => tools::delete,
    get "/api/v1/projects/{project}/tools/{name}/terminal" => tools::terminal,
    get "/api/v1/projects/{project}/services/{kind}/{name}/releases" => services::releases::releases,
    get "/api/v1/projects/{project}/services/{kind}/{name}/trace" => services::trace::trace,
    get "/api/v1/projects/{project}/services/{kind}/{name}/timeline" => timeline::service,
    get "/api/v1/projects/{project}/services/{kind}/{name}/rollout" => services::rollout::rollout,
    get "/api/v1/projects/{project}/services/{kind}/{name}/runs" => services::runs::runs,
    get "/api/v1/projects/{project}/services/{kind}/{name}/evidence" => services::evidence::evidence,
    get "/api/v1/projects/{project}/timeline" => timeline::project,
    post "/api/v1/projects/{project}/plan" => projects::writes::plan,
    post "/api/v1/projects/{project}/apply" => projects::writes::apply,
    get "/api/v1/projects/{project}/certificates/{secret}" => projects::writes::certificates::get,
    get "/api/v1/projects/{project}/backups" => projects::backups::get,
    get "/api/v1/projects/{project}/notifications" => notifications::project_get,
    post "/api/v1/projects/{project}/notifications/plan" => notifications::project_plan,
    post "/api/v1/projects/{project}/notifications/apply" => notifications::project_apply,
    get "/api/v1/projects/{project}/alerts" => alerts::project,
    get "/api/v1/projects/{project}/databases" => projects::databases::get,
    get "/api/v1/projects/{project}/object-storage" => projects::object_storage::get,
    get "/api/v1/projects/{project}/databases/{engine}/{name}/export" => projects::database_export::export,
    get "/api/v1/projects/{project}/volumes/{claim}/restores" => projects::volume_restores::get,
    get "/api/v1/projects/{project}/builds" => projects::builds::get,
    post "/api/v1/projects/{project}/builds/{build}/source" => projects::source_upload::upload,
    post "/api/v1/projects/{project}/deploy-keys" => projects::deploy_keys::create,
    get "/api/v1/projects/{project}/github-app" => cluster::github_app::project,
    post "/api/v1/projects/{project}/images/inspect" => images::inspect,
    post "/api/v1/projects/{project}/compose" => projects::compose::convert_compose,
    post "/api/v1/projects/{project}/deploy-keys/{name}/public" => projects::deploy_keys::public,
    get "/api/v1/projects/{project}/network-policy/presets" => projects::network_policy::presets,
    get "/api/v1/projects/{project}/usage" => projects::usage::get,
    get "/api/v1/projects/{project}/volumes" => projects::usage::volumes,
    get "/api/v1/projects/{project}/storage" => projects::storage::get,
    get "/api/v1/projects/{project}/services/{kind}/{name}/history" => services::history::get,
    get "/api/v1/projects/{project}/services/{kind}/{name}/logs/history" => services::logs::history,
    get "/api/v1/settings/metrics-store" => settings::store::get_setting,
    put "/api/v1/settings/metrics-store" => settings::store::set,
    get "/api/v1/settings/cluster-name" => settings::cluster_name::get_setting,
    put "/api/v1/settings/cluster-name" => settings::cluster_name::set,
    get "/api/v1/settings/domains" => settings::domains::get_setting,
    put "/api/v1/settings/domains" => settings::domains::set,
    get "/api/v1/settings/sign-in" => settings::sign_in::get_setting,
    put "/api/v1/settings/sign-in" => settings::sign_in::set,
    get "/api/v1/account" => account::get,
    get "/api/v1/members" => members::handlers::list,
    post "/api/v1/members/plan" => members::plan::plan,
    post "/api/v1/members/apply" => members::plan::apply,
    delete "/api/v1/members/invitations/{name}" => members::handlers::revoke_invitation,
    post "/api/v1/members/{username}/setup-links" => members::handlers::setup_link,
    delete "/api/v1/members/{username}/credentials/{id}" => members::handlers::remove_passkey,
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
/// `zeabur_proxy` reaches Zeabur Proxy for log and usage history, when it is installed.
pub fn router_with_key(
    access: Arc<dyn ClusterAccess>,
    plan_key: PlanKey,
    settings: Settings,
    sign_in: Arc<dyn account::SignInPage>,
    zeabur_proxy: Option<Arc<dyn services::proxy::ZeaburProxy>>,
) -> Router {
    let images = inspector(images::https::Reach::Public, &settings);
    let terminals = tools::connect(&settings.terminal);
    build_with(
        access,
        zeabur_kube::k8s_openapi::jiff::Timestamp::now,
        Some(Arc::new(plan_key)),
        pods::exec::IDLE,
        settings,
        Arc::new(resolve::System),
        sign_in,
        zeabur_proxy,
        images,
        zeabur_catalog::template_idle,
        images::https::Reach::Public,
        terminals,
    )
}

/// [`router`], reading images' configurations with `images`; for tests.
pub fn router_with_images(access: Arc<dyn ClusterAccess>, images: images::Inspector) -> Router {
    build_with(
        access,
        zeabur_kube::k8s_openapi::jiff::Timestamp::now,
        PlanKey::random().ok().map(Arc::new),
        pods::exec::IDLE,
        Settings::default(),
        Arc::new(resolve::System),
        Arc::new(account::Fixed(None)),
        None,
        Arc::new(images),
        zeabur_catalog::template_idle,
        images::https::Reach::Any,
        None,
    )
}

fn inspector(reach: images::https::Reach, settings: &Settings) -> Arc<images::Inspector> {
    Arc::new(images::Inspector::connected(
        reach,
        settings.trusted_ca.as_deref(),
    ))
}

/// [`router_at`], reaching Zeabur Proxy through `zeabur_proxy`; for tests.
pub fn router_with_zeabur_proxy(
    access: Arc<dyn ClusterAccess>,
    clock: fn() -> zeabur_kube::k8s_openapi::jiff::Timestamp,
    zeabur_proxy: Arc<dyn services::proxy::ZeaburProxy>,
) -> Router {
    build_with(
        access,
        clock,
        PlanKey::random().ok().map(Arc::new),
        pods::exec::IDLE,
        Settings::default(),
        Arc::new(resolve::System),
        Arc::new(account::Fixed(None)),
        Some(zeabur_proxy),
        inspector(images::https::Reach::Any, &Settings::default()),
        zeabur_catalog::template_idle,
        images::https::Reach::Any,
        None,
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

/// [`router_with_settings`], with `terminals` as Zeabur's own service account in the terminal
/// namespace (see [`tools`]); for tests.
pub fn router_with_terminals(
    access: Arc<dyn ClusterAccess>,
    settings: Settings,
    terminals: Arc<dyn zeabur_kube::Cluster>,
) -> Router {
    let images = inspector(images::https::Reach::Any, &settings);
    build_with(
        access,
        zeabur_kube::k8s_openapi::jiff::Timestamp::now,
        PlanKey::random().ok().map(Arc::new),
        pods::exec::IDLE,
        settings,
        Arc::new(resolve::System),
        Arc::new(account::Fixed(None)),
        None,
        images,
        zeabur_catalog::template_idle,
        images::https::Reach::Any,
        Some(terminals),
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

/// [`router_with_settings`], looking domains up with `resolver`; for tests.
pub fn router_with_settings_and_resolver(
    access: Arc<dyn ClusterAccess>,
    settings: Settings,
    resolver: Arc<dyn resolve::Resolve>,
) -> Router {
    let images = inspector(images::https::Reach::Any, &settings);
    build_with(
        access,
        zeabur_kube::k8s_openapi::jiff::Timestamp::now,
        PlanKey::random().ok().map(Arc::new),
        pods::exec::IDLE,
        settings,
        resolver,
        Arc::new(account::Fixed(None)),
        None,
        images,
        zeabur_catalog::template_idle,
        images::https::Reach::Any,
        None,
    )
}

/// [`router_with_settings_and_resolver`], reading images' configurations with `images` and
/// templates' idle use from `template_idle`; for the demo, whose `clock` may be fixed.
pub fn router_for_demo(
    access: Arc<dyn ClusterAccess>,
    clock: fn() -> zeabur_kube::k8s_openapi::jiff::Timestamp,
    settings: Settings,
    resolver: Arc<dyn resolve::Resolve>,
    images: Arc<images::Inspector>,
    template_idle: fn(&str) -> Option<zeabur_catalog::IdleUse>,
    terminals: Option<Arc<dyn zeabur_kube::Cluster>>,
) -> Router {
    build_with(
        access,
        clock,
        PlanKey::random().ok().map(Arc::new),
        pods::exec::IDLE,
        settings,
        resolver,
        Arc::new(account::Fixed(None)),
        None,
        images,
        template_idle,
        images::https::Reach::Any,
        terminals,
    )
}

fn build(
    access: Arc<dyn ClusterAccess>,
    clock: fn() -> zeabur_kube::k8s_openapi::jiff::Timestamp,
    plan_key: Option<Arc<PlanKey>>,
    terminal_idle: std::time::Duration,
    settings: Settings,
) -> Router {
    let images = inspector(images::https::Reach::Any, &settings);
    build_with(
        access,
        clock,
        plan_key,
        terminal_idle,
        settings,
        Arc::new(resolve::System),
        Arc::new(account::Fixed(None)),
        None,
        images,
        zeabur_catalog::template_idle,
        images::https::Reach::Any,
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
    zeabur_proxy: Option<Arc<dyn services::proxy::ZeaburProxy>>,
    images: Arc<images::Inspector>,
    template_idle: fn(&str) -> Option<zeabur_catalog::IdleUse>,
    reach: images::https::Reach,
    terminals: Option<Arc<dyn zeabur_kube::Cluster>>,
) -> Router {
    let catalogue = Arc::new(zeabur_catalog::catalogue().unwrap_or_default());
    let trusted_ca = settings.trusted_ca.clone();
    let github = images::https::Https::new(reach, trusted_ca.as_deref()).ok();
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
        pusher: Arc::new(projects::oci_push::Pusher::new(
            reach,
            trusted_ca.as_deref(),
        )),
        store: Arc::new(cluster::object_store::Fetcher::new(
            images.reach(),
            trusted_ca.as_deref(),
        )),
        images,
        github,
        terminals,
    };
    routes()
        .route(
            "/api/openapi.json",
            get(|| async {
                (
                    [(axum::http::header::CONTENT_TYPE, "application/json")],
                    bytes::Bytes::from_static(OPENAPI_JSON),
                )
            }),
        )
        .route("/api/{*rest}", axum::routing::any(handlers::not_found))
        .layer(axum::middleware::from_fn(cache::no_store))
        .with_state(state)
}

/// The committed `openapi.json`, which the server serves at `/api/openapi.json`.
const OPENAPI_JSON: &[u8] = include_bytes!("../openapi.json");

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn the_routes_are_the_ones_the_document_describes() {
        let document = serde_json::to_value(openapi()).unwrap();
        let described: BTreeSet<(String, String)> = document["paths"]
            .as_object()
            .unwrap()
            .iter()
            .flat_map(|(path, item)| {
                item.as_object()
                    .unwrap()
                    .keys()
                    .map(|method| (method.clone(), path.clone()))
            })
            .collect();
        let served: BTreeSet<(String, String)> = ROUTES
            .iter()
            .map(|(method, path)| ((*method).to_owned(), (*path).to_owned()))
            .collect();
        assert_eq!(served, described);
    }
}
