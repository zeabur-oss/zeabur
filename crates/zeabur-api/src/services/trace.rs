//! The way traffic travels to a service, hop by hop ([`zeabur_gaps::trace`]): each domain and
//! each private address, from DNS to a ready pod, with what was expected and what was seen.
//!
//! The trace is its own read: it looks the domains up from where Zeabur runs and lists the
//! cluster's ingress controllers, `IngressClasses` and the namespace's cert-manager `Certificates`,
//! which a polled document should not pay for. The first hop that fails on each address becomes a
//! diagnosis, with a plan where Zeabur can write the fix: a Service whose `targetPort` names no
//! port of the containers, when the containers declare exactly one, is pointed at it.

use axum::Json;
use axum::extract::{Path, State};
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_gaps::{Certificates, TraceInput};
use zeabur_kube::ResourceRef;
use zeabur_model::{Diagnosis, HopStatus, HopStep, RouteTrace, Service, WorkloadKind, id};
use zeabur_plan::{Change, PlanRequest};

use crate::error::ApiError;
use crate::handlers::{AppState, Person};
use crate::projects::{ProjectRead, not_found};
use crate::views::UnreadableView;

/// One address's way, and the plan that fixes where it stops when Zeabur can write one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RouteTraceView {
    /// The way.
    #[serde(flatten)]
    pub trace: RouteTrace,
    /// The changes that fix its first failed hop, to send to `plan`; absent when Zeabur cannot
    /// write the fix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<PlanRequest>,
}

/// Every way to a service, and the diagnoses where they stop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceTrace {
    /// `projects/<project>/services/<kind>/<name>/trace`.
    pub id: String,
    /// Its domains, by route and host, then its private addresses.
    pub routes: Vec<RouteTraceView>,
    /// The first failed hop of each address, as a diagnosis, once per cause.
    pub diagnoses: Vec<Diagnosis>,
    /// Reads the person was refused; hops that needed them are unknown.
    pub unreadable: Vec<UnreadableView>,
}

fn certificates() -> ResourceRef {
    ResourceRef::new("cert-manager.io", "v1", "certificates", "Certificate")
}

/// The way traffic travels to one service, hop by hop: DNS, the ingress controller's address and
/// readiness, the certificate, the route, the Service, its target port and the pods' readiness.
#[utoipa::path(get, path = "/api/v1/projects/{project}/services/{kind}/{name}/trace", tag = "services",
    operation_id = "traceService",
    params(("project" = String, Path),
           ("kind" = String, Path, description = "deployment, statefulset, daemonset, job or cronjob; any case"),
           ("name" = String, Path)),
    responses((status = 200, body = ServiceTrace), (status = 404, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn trace(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path((project, kind, name)): Path<(String, String, String)>,
) -> Result<Json<ServiceTrace>, ApiError> {
    let Some(workload_kind) = WorkloadKind::parse(&kind) else {
        return Err(not_found(&project, &kind, &name));
    };
    let cluster = cluster.as_ref();
    let read = crate::projects::read(cluster, &project).await?;
    let service = read
        .document
        .project
        .services
        .iter()
        .find(|s| s.workload.kind == workload_kind && s.workload.name == name)
        .ok_or_else(|| not_found(&project, workload_kind.as_str(), &name))?;
    Ok(Json(of_service(&state, cluster, &read, service).await?))
}

/// The trace of `service`, read from `read` and the reads only a trace makes.
pub(crate) async fn of_service(
    state: &AppState,
    cluster: &dyn zeabur_kube::Cluster,
    read: &ProjectRead,
    service: &Service,
) -> Result<ServiceTrace, ApiError> {
    let project = &read.document.project.namespace;
    let mut unreadable = read.document.unreadable.clone();
    let refused = unreadable.len();
    let classes = crate::gaps::ingress_class_facts(cluster, &mut unreadable).await?;
    let classes_readable = unreadable.len() == refused;
    let components = crate::incidents::components(cluster, &mut unreadable).await?;
    let resource = certificates();
    let listed = match cluster.list_objects(&resource, Some(project)).await {
        Ok(Some(objects)) => Some(
            objects
                .into_iter()
                .filter_map(|o| serde_json::to_value(o).ok())
                .collect::<Vec<_>>(),
        ),
        Ok(None) => None,
        Err(error) => {
            let refusal = error
                .into_unreadable(&resource.key())
                .map_err(|other| ApiError::from(other.into_problem("Certificates")))?;
            unreadable.push(refusal.into());
            Some(Vec::new())
        }
    };
    let certificate_refused = unreadable.iter().any(|u| u.resource == resource.key());
    let certificates = match &listed {
        _ if certificate_refused => Certificates::Unreadable,
        Some(listed) => Certificates::Listed(listed),
        None => Certificates::NotInstalled,
    };
    let readable = |resource: &str| {
        !read
            .document
            .unreadable
            .iter()
            .any(|u| u.resource == resource)
    };
    let mut input = TraceInput {
        snapshot: &read.snapshot,
        http_routes: &read.http_routes,
        classes: classes_readable.then_some(classes.as_slice()),
        components: components.as_deref(),
        certificates,
        resolved: &std::collections::BTreeMap::new(),
        services_readable: readable("services"),
        secrets_readable: readable("secrets"),
    };
    let names = zeabur_gaps::names_to_resolve(&input, service);
    let resolved = crate::resolve::all(state.resolver.as_ref(), names).await;
    input.resolved = &resolved;
    let traces = zeabur_gaps::trace(&input, service);
    let diagnoses = zeabur_gaps::trace_diagnoses(&traces, service);
    let routes = traces
        .into_iter()
        .map(|trace| RouteTraceView {
            fix: fix(read, service, &trace),
            trace,
        })
        .collect();
    Ok(ServiceTrace {
        id: format!(
            "{}/trace",
            id::service(project, service.workload.kind, &service.workload.name)
        ),
        routes,
        diagnoses,
        unreadable,
    })
}

/// The plan that fixes the first failed hop of `trace`, where Zeabur can write one: a target port
/// pointed at the one port the containers declare.
fn fix(read: &ProjectRead, service: &Service, trace: &RouteTrace) -> Option<PlanRequest> {
    let hop = trace.first_failure()?;
    if hop.step != HopStep::TargetPort || hop.status != HopStatus::Failed {
        return None;
    }
    let endpoint = hop.message.params.get("endpoint")?;
    let wrong = hop.saw.as_deref()?;
    let workload = zeabur_graph::workloads(&read.snapshot)
        .into_iter()
        .find(|w| w.kind() == service.workload.kind && w.name() == service.workload.name)?;
    let ports: Vec<i32> = workload
        .pod_spec()?
        .containers
        .iter()
        .flat_map(|c| c.ports.iter().flatten().map(|p| p.container_port))
        .collect();
    let [port] = ports.as_slice() else {
        return None;
    };
    let object = read
        .snapshot
        .services
        .iter()
        .find(|s| s.metadata.name.as_deref() == Some(endpoint.as_str()))?;
    let mut value = serde_json::to_value(object).ok()?;
    let mut changed = false;
    for entry in value.pointer_mut("/spec/ports")?.as_array_mut()?.iter_mut() {
        let target = entry
            .get("targetPort")
            .or_else(|| entry.get("port"))
            .map(|t| t.as_str().map_or_else(|| t.to_string(), str::to_owned));
        if target.as_deref() == Some(wrong) {
            entry["targetPort"] = serde_json::json!(port);
            changed = true;
        }
    }
    if !changed {
        return None;
    }
    if let Some(metadata) = value.get_mut("metadata").and_then(|m| m.as_object_mut()) {
        metadata.remove("managedFields");
    }
    if let Some(object) = value.as_object_mut() {
        object.remove("status");
        object
            .entry("apiVersion")
            .or_insert_with(|| serde_json::json!("v1"));
        object
            .entry("kind")
            .or_insert_with(|| serde_json::json!("Service"));
    }
    Some(PlanRequest {
        changes: vec![Change::Edit {
            resource: zeabur_plan::ResourceRef {
                group: String::new(),
                version: "v1".to_owned(),
                resource: "services".to_owned(),
            },
            name: endpoint.clone(),
            yaml: serde_json::to_string_pretty(&value).ok()?,
        }],
    })
}
