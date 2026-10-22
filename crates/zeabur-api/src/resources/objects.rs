//! Every object of a project, of any kind, and each one as YAML.
//!
//! The projection knows the kinds that make up services (workloads, Services, Ingresses,
//! ConfigMaps, Secrets, PersistentVolumeClaims). A namespace holds more: autoscalers, disruption
//! budgets, network policies, service accounts, and custom resources such as certificates. These
//! endpoints show them all, as the person may read them.
//!
//! # Listing: kinds first
//!
//! `GET /projects/{p}/objects` asks discovery which namespaced resources the cluster serves and
//! answers every listable kind, each with an `access` hint from one `SelfSubjectRulesReview`
//! (see `zeabur_kube::objects`). Only some kinds are read, metadata only (the form `kubectl get`
//! uses for a table), a few at a time:
//!
//! - without parameters, the kinds a project usually holds (`CURATED`: workloads, Services,
//!   Ingresses and HTTPRoutes, ConfigMaps, Secrets, claims, network policies, service accounts,
//!   autoscalers and disruption budgets);
//! - with `?kinds=` (`<group>/<resource>`, `core` for the core group, comma-separated), only those
//!   kinds, generated ones included, so a client can open one kind for a single list;
//! - with `?all=true`, every kind that controllers do not generate.
//!
//! Other kinds come back with `listed: false` and no items, for a client to read on demand. A
//! kind the hint says the person may not list is not read; one refused when read is named in
//! `unreadable`. Resources that are not objects a person keeps (`metrics.k8s.io`'s measurements)
//! and the core group's `events` (the same events as `events.k8s.io`) are left out. Kinds that
//! controllers create on their own (replica sets, endpoint slices, controller revisions, leases,
//! events, endpoints) are marked `generated` and read only when named.
//!
//! # One object
//!
//! `GET /projects/{p}/objects/{group}/{version}/{resource}/{name}` (`core` for the empty group)
//! returns the object as YAML without the server's bookkeeping, a Secret's values hidden (see
//! `zeabur_graph::manifest`), with the object's origin and the tool that manages it.

use std::collections::BTreeSet;

use axum::Json;
use axum::extract::{Path, RawQuery};
use futures_util::StreamExt as _;
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use zeabur_kube::{Access, AccessRules, ClusterError, ResourceRef, ServedResource};
use zeabur_model::{Origin, Provenance};

use crate::error::{ApiError, get_read, read_problem};
use crate::handlers::Person;
use crate::views::UnreadableView;

/// How many kinds are listed at the same time.
const CONCURRENCY: usize = 8;
/// The path segment for the core group, whose name is empty.
const CORE: &str = "core";

/// Kinds controllers create on their own, as `(group, resource)`.
pub(crate) const GENERATED: [(&str, &str); 6] = [
    ("apps", "replicasets"),
    ("apps", "controllerrevisions"),
    ("discovery.k8s.io", "endpointslices"),
    ("coordination.k8s.io", "leases"),
    ("events.k8s.io", "events"),
    ("", "endpoints"),
];

/// Kinds a project usually holds, read without being asked for, as `(group, resource)`.
pub(crate) const CURATED: [(&str, &str); 15] = [
    ("apps", "deployments"),
    ("apps", "statefulsets"),
    ("apps", "daemonsets"),
    ("batch", "jobs"),
    ("batch", "cronjobs"),
    ("", "services"),
    ("networking.k8s.io", "ingresses"),
    ("gateway.networking.k8s.io", "httproutes"),
    ("", "configmaps"),
    ("", "secrets"),
    ("", "persistentvolumeclaims"),
    ("networking.k8s.io", "networkpolicies"),
    ("", "serviceaccounts"),
    ("autoscaling", "horizontalpodautoscalers"),
    ("policy", "poddisruptionbudgets"),
];

/// Whether the person may list a kind, as one `SelfSubjectRulesReview` hints, or as a list
/// answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ObjectAccess {
    /// A rule allows it.
    Allowed,
    /// No rule allows it, or a list was refused.
    Denied,
    /// The review cannot tell (another authorizer decides too); reading it tells.
    Unknown,
}

impl From<Access> for ObjectAccess {
    fn from(access: Access) -> Self {
        match access {
            Access::Allowed => Self::Allowed,
            Access::Denied => Self::Denied,
            Access::Unknown => Self::Unknown,
        }
    }
}

/// The parameters of the objects list (for the description; parsed by hand like
/// [`crate::query`]).
#[expect(
    dead_code,
    reason = "describes the parameters in the OpenAPI document; parsed by hand"
)]
#[derive(Debug, Default, utoipa::IntoParams)]
#[into_params(parameter_in = Query, rename_all = "camelCase")]
pub(crate) struct ObjectListParams {
    /// Kinds to read instead of the usual ones, as `<group>/<resource>` (`core` for the core
    /// group), comma-separated: `apps/replicasets,cert-manager.io/certificates`.
    kinds: Option<String>,
    /// `true`: read every kind that controllers do not generate ("scan all kinds").
    all: Option<bool>,
}

/// One object in a listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ObjectSummary {
    /// `projects/<p>/objects/<group>/<version>/<resource>/<name>`, `core` for the empty group.
    pub id: String,
    /// Its name.
    pub name: String,
    /// When it was created, RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// The kind of its controlling owner, when it has one (`ReplicaSet` for a pod).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_kind: Option<String>,
}

/// The objects of one kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ObjectKind {
    /// The API group; empty for the core group.
    pub group: String,
    /// The version.
    pub version: String,
    /// The plural resource name.
    pub resource: String,
    /// The kind.
    pub kind: String,
    /// Controllers create these on their own; read only when named in `kinds`.
    pub generated: bool,
    /// One of the kinds a project usually holds, read without being asked for.
    pub curated: bool,
    /// Whether the person may list it.
    pub access: ObjectAccess,
    /// Whether it was read in this answer; when `false`, `items` is empty and says nothing.
    pub listed: bool,
    /// By name; empty when not `listed`.
    pub items: Vec<ObjectSummary>,
}

/// Every object of a project, by kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ObjectList {
    /// `projects/<p>/objects`.
    pub id: String,
    /// Every listable namespaced kind the cluster serves, read or not, by group then kind.
    pub items: Vec<ObjectKind>,
    /// Every kind that controllers do not generate was read (`?all=true`).
    pub scanned: bool,
    /// Kinds the person may not list.
    pub unreadable: Vec<UnreadableView>,
}

/// One object as YAML.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ObjectDocument {
    /// `projects/<p>/objects/<group>/<version>/<resource>/<name>`.
    pub id: String,
    /// The plural resource name.
    pub resource: String,
    /// The object as YAML, without the server's bookkeeping; a Secret's values hidden.
    pub yaml: String,
    /// The object.
    pub origin: Origin,
    /// The tool that manages it, if any.
    pub provenance: Provenance,
}

const fn segment(group: &str) -> &str {
    if group.is_empty() { CORE } else { group }
}

fn object_id(project: &str, resource: &ResourceRef, name: &str) -> String {
    format!(
        "projects/{project}/objects/{}/{}/{}/{name}",
        segment(&resource.group),
        resource.version,
        resource.resource
    )
}

/// Whether a served resource belongs in a project's objects.
fn listed(served: &ServedResource) -> bool {
    served.namespaced
        && served.listable()
        && served.group != "metrics.k8s.io"
        && !(served.group.is_empty() && served.resource == "events")
}

fn summary(project: &str, resource: &ResourceRef, metadata: ObjectMeta) -> ObjectSummary {
    let owner_kind = zeabur_adapters::controller_of(&metadata).map(|o| o.kind.clone());
    let name = metadata.name.unwrap_or_default();
    ObjectSummary {
        id: object_id(project, resource, &name),
        owner_kind,
        created: metadata.creation_timestamp.map(|t| t.0.to_string()),
        name,
    }
}

fn unreadable(error: ClusterError, resource: &ResourceRef) -> Result<UnreadableView, ApiError> {
    error
        .into_unreadable(&resource.key())
        .map(Into::into)
        .map_err(|other| ApiError::from(other.into_problem("the project's objects")))
}

fn key(group: &str, resource: &str) -> String {
    format!("{}/{resource}", segment(group))
}

/// Reads `?kinds=` and `?all=`.
fn parse_query(query: Option<&str>) -> Result<(BTreeSet<String>, bool), ApiError> {
    let mut kinds = BTreeSet::new();
    let mut all = false;
    for (name, value) in form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
        match name.as_ref() {
            "kinds" => kinds.extend(
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|k| !k.is_empty())
                    .map(str::to_owned),
            ),
            "all" => {
                all = match value.as_ref() {
                    "true" => true,
                    "false" => false,
                    other => {
                        return Err(ApiError::from(Problem::new(
                            Kind::BadRequest,
                            format!("`all` must be `true` or `false`, not {other:?}."),
                        )));
                    }
                };
            }
            _ => {}
        }
    }
    Ok((kinds, all))
}

/// Every listable namespaced kind of a project, kinds first: the usual kinds (or only those named
/// in `kinds`) and, with `all`, every kind that controllers do not generate are read (names, ages
/// and owners only); the rest come back unread. Each kind carries an access hint; kinds refused
/// when read are named in `unreadable`.
#[utoipa::path(get, path = "/api/v1/projects/{project}/objects", tag = "resources", operation_id = "listObjects",
    params(("project" = String, Path), ObjectListParams),
    responses((status = 200, body = ObjectList), (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn list(
    Person(cluster): Person,
    Path(project): Path<String>,
    RawQuery(query): RawQuery,
) -> Result<Json<ObjectList>, ApiError> {
    let (requested, all) = parse_query(query.as_deref())?;
    let served = cluster
        .served_resources()
        .await
        .map_err(|e| ApiError::from(e.into_problem("the cluster's APIs")))?;
    // The review is open to every signed-in person; if it fails anyway, every kind is unknown and
    // reading it tells.
    let rules = cluster.access_rules(&project).await.unwrap_or(AccessRules {
        rules: Vec::new(),
        incomplete: true,
    });
    let mut kinds: Vec<(ObjectKind, ResourceRef)> = served
        .iter()
        .filter(|s| listed(s))
        .map(|s| {
            let pair = (s.group.as_str(), s.resource.as_str());
            let reference = s.as_ref();
            let kind = ObjectKind {
                group: s.group.clone(),
                version: s.version.clone(),
                resource: s.resource.clone(),
                kind: s.kind.clone(),
                generated: GENERATED.contains(&pair),
                curated: CURATED.contains(&pair),
                access: rules.list(&reference).into(),
                listed: false,
                items: Vec::new(),
            };
            (kind, reference)
        })
        .collect();
    let wanted = |kind: &ObjectKind| {
        kind.access != ObjectAccess::Denied
            && ((kind.curated && requested.is_empty())
                || requested.contains(&key(&kind.group, &kind.resource))
                || (all && !kind.generated))
    };
    let to_read: Vec<(usize, ResourceRef)> = kinds
        .iter()
        .enumerate()
        .filter(|(_, (kind, _))| wanted(kind))
        .map(|(i, (_, reference))| (i, reference.clone()))
        .collect();
    let cluster = cluster.as_ref();
    let project_ref = project.as_str();
    let reads: Vec<_> = futures_util::stream::iter(to_read)
        .map(|(i, resource)| async move {
            let read = cluster.list_served_metadata(&resource, project_ref).await;
            (i, resource, read)
        })
        .buffered(CONCURRENCY)
        .collect()
        .await;
    let mut refused = Vec::new();
    for (i, resource, read) in reads {
        let Some((kind, _)) = kinds.get_mut(i) else {
            continue;
        };
        match read {
            Ok(Some(metadata)) => {
                let mut objects: Vec<ObjectSummary> = metadata
                    .into_iter()
                    .map(|m| summary(&project, &resource, m))
                    .collect();
                objects.sort_by(|a, b| a.name.cmp(&b.name));
                kind.items = objects;
                kind.listed = true;
            }
            Ok(None) => kind.listed = true,
            Err(error) => {
                refused.push(unreadable(error, &resource)?);
                kind.access = ObjectAccess::Denied;
            }
        }
    }
    // Nothing at all may be listed: the project itself is out of reach, not a few kinds.
    if !kinds.is_empty() && kinds.iter().all(|(k, _)| k.access == ObjectAccess::Denied) {
        return Err(ApiError::from(Problem {
            kubernetes: refused.into_iter().next().map(|r| r.kubernetes),
            ..Problem::new(
                Kind::Forbidden,
                format!("You do not have permission to list anything in project {project}."),
            )
        }));
    }
    kinds.sort_by(|(a, _), (b, _)| (&a.group, &a.kind).cmp(&(&b.group, &b.kind)));
    Ok(Json(ObjectList {
        id: format!("projects/{project}/objects"),
        items: kinds.into_iter().map(|(k, _)| k).collect(),
        scanned: all,
        unreadable: refused,
    }))
}

fn not_found(project: &str, what: &str) -> ApiError {
    ApiError::from(Problem::new(
        Kind::NotFound,
        format!("Project {project} has no {what}, or you cannot see it."),
    ))
}

/// One object of a project as YAML, without the server's bookkeeping and with a Secret's values
/// hidden; any namespaced kind the cluster serves (`core` names the empty group).
#[utoipa::path(get, path = "/api/v1/projects/{project}/objects/{group}/{version}/{resource}/{name}", tag = "resources", operation_id = "getObject",
    params(("project" = String, Path),
           ("group" = String, Path, description = "The API group; `core` for the core group"),
           ("version" = String, Path),
           ("resource" = String, Path, description = "The plural resource name, such as `deployments`"),
           ("name" = String, Path)),
    responses((status = 200, body = ObjectDocument), (status = 404, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    Person(cluster): Person,
    Path((project, group, version, resource, name)): Path<(String, String, String, String, String)>,
) -> Result<Json<ObjectDocument>, ApiError> {
    let group = if group == CORE { String::new() } else { group };
    let what = format!("{resource} named {name}");
    let served = cluster
        .served_resources()
        .await
        .map_err(|e| ApiError::from(e.into_problem("the cluster's APIs")))?;
    let Some(found) = served.iter().find(|s| {
        s.group == group && s.version == version && s.resource == resource && s.namespaced
    }) else {
        return Err(not_found(&project, &what));
    };
    let reference = found.as_ref();
    let object = match cluster.get_object(&reference, Some(&project), &name).await {
        Ok(Some(object)) => object,
        Ok(None) => return Err(not_found(&project, &what)),
        Err(error) if error.code() == Some(404) => return Err(not_found(&project, &what)),
        Err(error) => {
            return Err(read_problem(
                error,
                &what,
                get_read(&group, &resource, Some(&project), &name),
            ));
        }
    };
    let provenance = zeabur_graph::provenance(&object.metadata);
    let mut value = serde_json::to_value(&object)
        .map_err(|e| ApiError::from(Problem::new(Kind::Internal, e.to_string())))?;
    if let Some(top) = value.as_object_mut() {
        top.insert("apiVersion".to_owned(), reference.api_version().into());
        top.insert("kind".to_owned(), reference.kind.clone().into());
    }
    let yaml = zeabur_graph::manifest::to_yaml(&value)
        .map_err(|e| ApiError::from(Problem::new(Kind::Internal, e)))?;
    Ok(Json(ObjectDocument {
        id: object_id(&project, &reference, &name),
        resource: reference.resource.clone(),
        yaml,
        origin: Origin::complete(
            &reference.api_version(),
            &reference.kind,
            Some(&project),
            &name,
        ),
        provenance,
    }))
}
