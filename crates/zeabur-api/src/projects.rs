//! Projects (namespaces) and what is in them: the projection, read fresh as the person.
//!
//! # The project document is the unit
//!
//! One read of a namespace (one LIST per kind, twelve in all) gives the whole projection, and
//! `get` returns it as one document. The service and resource endpoints are cuts of that same
//! read, for deep links, scripts and the MCP server; the web interface derives its service,
//! variables and objects views from the cached document instead of reading the namespace again
//! for each, so the parts of one screen cannot contradict each other.
//!
//! # Consistency
//!
//! A project is read with one consistent LIST per kind, issued concurrently while the request
//! is handled. The kinds are not read at one shared `resourceVersion`, which Kubernetes does not
//! offer across kinds: resource versions "can be compared" only within one resource type
//! [k8s-concepts]. A response can therefore briefly show, for example, a pod that a
//! just-deleted workload owned; the next read corrects it. Zeabur promises freshness (nothing
//! older than the request), not a single-revision snapshot.
//!
//! # Diagnoses
//!
//! The document also reads the namespace's events (one more LIST, alongside the others) and gives
//! each service its diagnoses ([`zeabur_gaps::diagnose`]): what is going wrong right now, in
//! sentence keys the client translates, with the status fields and events as evidence. When the
//! person may not list events, the diagnoses rest on the pods' status alone and the refusal is
//! named in `unreadable`.
//!
//! # Health in the list
//!
//! `list` gives each adopted project the counts of its healthy, rolling-out, warning and
//! failing services ([`zeabur_graph::status`]), and each service's health, replicas, restarts
//! and failing reason, read from its workloads and pods only (six LISTs per project, at most
//! eight projects at a time), so a navigation list or a home page can say which service fails
//! in which project without fetching every project's document, environment variables included.
//! Namespaces that are not adopted get neither: there may be hundreds.
//!
//! [k8s-concepts]: https://kubernetes.io/docs/reference/using-api/api-concepts/

pub mod backups;
pub mod builds;
pub mod databases;
pub(crate) mod events;
pub mod namespace;
pub mod network_policy;
pub mod rights;
pub mod usage;
pub mod writes;

use std::sync::Arc;

use axum::Json;
use axum::extract::Path;
use futures_util::StreamExt as _;
use zeabur_core::names::{LABEL_PROJECT, is_system_namespace};
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_graph::Snapshot;
use zeabur_kube::k8s_openapi::api::events::v1::Event;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{Cluster, ClusterExt as _, NamespaceObjects};
use zeabur_model::{Origin, ProjectDetails, ProjectHealth, id};

use crate::error::{ApiError, list_read, project_read, read_problem};
use crate::handlers::Person;
use crate::views::{
    ProjectDocument, ProjectExtensions, ProjectList, ProjectSummary, ServiceSummary, UnreadableView,
};

/// How many adopted projects' workloads are read at the same time for their health.
const HEALTH_CONCURRENCY: usize = 8;

/// The projects you may see, with the health of the adopted ones.
#[utoipa::path(get, path = "/api/v1/projects", tag = "projects",
    responses((status = 200, body = ProjectList), (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn list(Person(cluster): Person) -> Result<Json<ProjectList>, ApiError> {
    let namespaces = cluster.namespaces().await.map_err(|e| {
        let mut problem = e.into_problem("the list of namespaces");
        if problem.kind == Kind::Forbidden {
            problem.deny(list_read("", "namespaces", None));
            // Many people may use some namespaces without listing all of them: say how to go on.
            "You do not have permission to list the cluster's namespaces, so Zeabur cannot show your projects."
                .clone_into(&mut problem.detail);
            problem.fix_for_occasion(
                "Ask a cluster administrator for `list` on `namespaces`, for example with a \
                 ClusterRole granting it and a ClusterRoleBinding to you.",
                zeabur_core::message::Message::new("problem.forbidden.fix.askForRule")
                    .with("rule", "`list` on `namespaces`"),
            );
        }
        ApiError::from(problem)
    })?;
    let mut items: Vec<ProjectSummary> = namespaces
        .into_iter()
        .filter_map(|namespace| {
            let name = namespace.metadata.name?;
            let adopted = namespace
                .metadata
                .labels
                .is_some_and(|labels| labels.contains_key(LABEL_PROJECT));
            Some(ProjectSummary {
                id: id::project(&name),
                origin: Origin::complete("v1", "Namespace", None, &name),
                system: is_system_namespace(&name),
                details: ProjectDetails::from_annotations(namespace.metadata.annotations.as_ref()),
                name,
                adopted,
                health: None,
                services: None,
            })
        })
        .collect();
    items.sort_by(|a, b| b.adopted.cmp(&a.adopted).then_with(|| a.name.cmp(&b.name)));
    let adopted: Vec<(usize, String)> = items
        .iter()
        .enumerate()
        .filter(|(_, p)| p.adopted)
        .map(|(i, p)| (i, p.name.clone()))
        .collect();
    let healths: Vec<(usize, Option<Services>)> = futures_util::stream::iter(adopted)
        .map(|(i, name)| {
            let cluster = Arc::clone(&cluster);
            async move { (i, health_of(cluster, name).await) }
        })
        .buffered(HEALTH_CONCURRENCY)
        .collect()
        .await;
    for (i, health) in healths {
        if let (Some(item), Some((counts, services))) = (items.get_mut(i), health) {
            item.health = Some(counts);
            item.services = Some(services);
        }
    }
    let can_create = can_create(cluster.as_ref()).await;
    Ok(Json(ProjectList { items, can_create }))
}

/// Whether the person may create a Namespace, from one `SelfSubjectAccessReview`.
async fn can_create(cluster: &dyn Cluster) -> namespace::Permission {
    let request = zeabur_core::problem::Denied {
        verb: "create".to_owned(),
        group: String::new(),
        resource: "namespaces".to_owned(),
        name: None,
        namespace: None,
    };
    match cluster.review_access(&request).await {
        Ok(true) => namespace::Permission::Allowed,
        Ok(false) => namespace::Permission::Denied,
        Err(_) => namespace::Permission::Unknown,
    }
}

/// A project's service health counts, and each service with its health.
type Services = (ProjectHealth, Vec<ServiceSummary>);

/// The service health counts of `project` and each service's health, from its workloads and
/// pods; `None` when they cannot be read.
async fn health_of(cluster: Arc<dyn Cluster>, project: String) -> Option<Services> {
    let objects = cluster.namespace_workloads(&project).await.ok()?;
    let (snapshot, _) = snapshot(objects);
    let projection = zeabur_graph::project(&snapshot);
    let services: Vec<ServiceSummary> = projection.services.iter().map(service_summary).collect();
    let counts = ProjectHealth::count(services.iter().map(|s| s.health));
    Some((counts, services))
}

/// A service as the project list shows it.
fn service_summary(service: &zeabur_model::Service) -> ServiceSummary {
    let workload = &service.workload;
    ServiceSummary {
        id: service.id.clone(),
        kind: workload.kind,
        name: workload.name.clone(),
        health: service.status.health,
        ready: workload.replicas.map(|r| r.ready),
        desired: workload.replicas.map(|r| r.desired),
        restarts: service
            .pods
            .iter()
            .fold(0_i32, |sum, pod| sum.saturating_add(pod.restarts)),
        reason: service.status.reason.clone(),
    }
}

/// Builds the projection's input from what the cluster returned. Secrets arrive without values;
/// only their key counts travel on.
pub(crate) fn snapshot(objects: NamespaceObjects) -> (Snapshot, Vec<UnreadableView>) {
    let mut secret_key_counts = std::collections::BTreeMap::new();
    let secrets = objects
        .secrets
        .into_iter()
        .map(|summary| {
            if let Some(name) = summary.secret.metadata.name.clone() {
                secret_key_counts.insert(name, summary.key_count);
            }
            summary.secret
        })
        .collect();
    let snapshot = Snapshot {
        namespace: objects.namespace,
        deployments: objects.deployments,
        stateful_sets: objects.stateful_sets,
        daemon_sets: objects.daemon_sets,
        jobs: objects.jobs,
        cron_jobs: objects.cron_jobs,
        pods: objects.pods,
        services: objects.services,
        config_maps: objects.config_maps,
        secrets,
        secret_key_counts,
        persistent_volume_claims: objects.persistent_volume_claims,
        ingresses: objects.ingresses,
        now: Some(Timestamp::now()),
    };
    let unreadable = objects
        .unreadable
        .into_iter()
        .map(UnreadableView::from)
        .collect();
    (snapshot, unreadable)
}

pub(crate) async fn document_of(
    cluster: &dyn Cluster,
    project: &str,
) -> Result<ProjectDocument, ApiError> {
    Ok(read(cluster, project).await?.document)
}

/// One read of a project: the document and what it was made from.
pub(crate) struct ProjectRead {
    pub(crate) document: ProjectDocument,
    pub(crate) snapshot: Snapshot,
    pub(crate) http_routes: Vec<serde_json::Value>,
    pub(crate) events: Vec<zeabur_gaps::Event>,
}

pub(crate) async fn read(cluster: &dyn Cluster, project: &str) -> Result<ProjectRead, ApiError> {
    let (objects, events) = futures_util::future::join(
        cluster.namespace_objects(project),
        cluster.list::<Event>(Some(project)),
    )
    .await;
    let objects = objects
        .map_err(|e| read_problem(e, &format!("project {project}"), project_read(project)))?;
    let http_routes = objects.http_routes.clone();
    let (snapshot, mut unreadable) = snapshot(objects);
    let mut projection = zeabur_graph::project(&snapshot);
    // Diagnoses read the namespace's events too. Without them (refused, or the read failed) the
    // pods' status still tells most of it, so the document is served and the refusal is named.
    let events: Vec<zeabur_gaps::Event> = match events {
        Ok(events) => events.iter().map(zeabur_gaps::Event::from_kube).collect(),
        Err(zeabur_kube::ClusterError::Status(status)) => {
            unreadable.push(UnreadableView {
                resource: "events.events.k8s.io".to_owned(),
                kubernetes: status,
            });
            Vec::new()
        }
        Err(_) => Vec::new(),
    };
    // A refused list leaves `snapshot.services` empty; it says nothing about which exist.
    let services_readable = !unreadable.iter().any(|u| u.resource == "services");
    let namespace = zeabur_gaps::Namespace::new(&snapshot, &events, &http_routes)
        .services_readable(services_readable);
    for service in &mut projection.services {
        service.diagnoses = namespace.diagnose(service);
    }
    let extensions = ProjectExtensions {
        network_policy: network_policy::extension(cluster, &snapshot, &projection, &mut unreadable)
            .await?,
    };
    Ok(ProjectRead {
        document: ProjectDocument {
            id: id::project(project),
            project: projection,
            unreadable,
            extensions,
        },
        snapshot,
        http_routes,
        events,
    })
}

/// A project's services, the resources they use, and how they relate: the project document.
#[utoipa::path(get, path = "/api/v1/projects/{project}", tag = "projects",
    params(("project" = String, Path, description = "The project's namespace")),
    responses((status = 200, body = ProjectDocument), (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    Person(cluster): Person,
    Path(project): Path<String>,
) -> Result<Json<ProjectDocument>, ApiError> {
    Ok(Json(document_of(cluster.as_ref(), &project).await?))
}

/// The `404` for a service or resource the project document does not have.
pub(crate) fn not_found(project: &str, kind: &str, name: &str) -> ApiError {
    ApiError::from(Problem::new(
        Kind::NotFound,
        format!("Project {project} has no {kind} named {name}, or you cannot see it."),
    ))
}
