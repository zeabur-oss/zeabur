//! The plans a terminal needs first: a debug container (`debugContainer`) and a tools pod
//! (`clusterTools`); see `zeabur_plan::terminal` and the `tools` module.

use serde_json::Value;
use zeabur_core::problem::{Denied, Kind, Problem};
use zeabur_kube::k8s_openapi::api::core::v1::Pod;
use zeabur_kube::{Cluster, ClusterExt as _, ResourceRef};
use zeabur_plan::terminal::{self as plans, ToolsPod, ToolsScope};
use zeabur_plan::{Change, Plan, PlanRequest, Warning};

use super::plan_problem;
use crate::access::Handoff;
use crate::error::{ApiError, get_read, read_problem};
use crate::settings::TerminalSettings;

/// Who a terminal plan is for.
pub(crate) struct Planner<'a> {
    pub(crate) settings: &'a TerminalSettings,
    /// The person's user name.
    pub(crate) person: &'a str,
    /// Their token, where Zeabur holds one.
    pub(crate) handoff: Option<&'a Handoff>,
}

/// The namespace a terminal plan writes in: the tools namespace for a tools pod when the chart
/// names one, the project otherwise.
pub(crate) fn namespace<'a>(
    settings: &'a TerminalSettings,
    request: &PlanRequest,
    project: &'a str,
) -> &'a str {
    if request
        .changes
        .iter()
        .any(|c| matches!(c, Change::ClusterTools))
    {
        settings.tools_namespace.as_deref().unwrap_or(project)
    } else {
        project
    }
}

/// The plan for a request with a terminal change, which must be its only change.
pub(crate) async fn compute(
    planner: &Planner<'_>,
    cluster: &dyn Cluster,
    project: &str,
    request: &PlanRequest,
    made_at: &str,
) -> Result<Plan, ApiError> {
    let [change] = request.changes.as_slice() else {
        return Err(ApiError::from(Problem::new(
            Kind::BadRequest,
            "A debug container and a tools pod are planned on their own, without other changes.",
        )));
    };
    match change {
        Change::DebugContainer { pod, container } => {
            debug_container(
                planner,
                cluster,
                project,
                pod,
                container.as_deref(),
                made_at,
            )
            .await
        }
        Change::ClusterTools => tools(planner, cluster, project, made_at).await,
        Change::Env { .. }
        | Change::Data { .. }
        | Change::AddDomain(_)
        | Change::Certificate(_)
        | Change::RenameDomain { .. }
        | Change::RemoveDomain { .. }
        | Change::Redirect(_)
        | Change::EnvImport(_)
        | Change::PublicPort(_)
        | Change::BasicAuth(_)
        | Change::ErrorPageHtml(_)
        | Change::PageServer(_)
        | Change::Template(_)
        | Change::ErrorPages(_)
        | Change::Maintenance(_)
        | Change::Restart { .. }
        | Change::Rollback { .. }
        | Change::ExpandVolume { .. }
        | Change::LabelVolume(_)
        | Change::AddVolume { .. }
        | Change::EditMount { .. }
        | Change::RemoveVolume { .. }
        | Change::SwapClaim { .. }
        | Change::Probe { .. }
        | Change::Image { .. }
        | Change::Rebuild { .. }
        | Change::BuildOptions(_)
        | Change::AutoUpdate { .. }
        | Change::Scale { .. }
        | Change::ConfigReload { .. }
        | Change::SecurityProfile { .. }
        | Change::PodSecurity { .. }
        | Change::Resources { .. }
        | Change::Edit { .. }
        | Change::Create { .. }
        | Change::DeployKey { .. }
        | Change::CopyS3Keys(_)
        | Change::Delete { .. }
        | Change::CreateProject { .. }
        | Change::Adopt
        | Change::Unadopt
        | Change::Details { .. }
        | Change::ScheduleTask { .. }
        | Change::RunTask { .. }
        | Change::EditTask { .. } => Err(ApiError::from(Problem::new(
            Kind::BadRequest,
            "This change is not a terminal's.",
        ))),
    }
}

pub(crate) async fn read_pod(
    cluster: &dyn Cluster,
    namespace: &str,
    pod: &str,
) -> Result<Value, ApiError> {
    crate::pods::exec::outside_node_shells(namespace)?;
    let found = cluster
        .find_object(&ResourceRef::of::<Pod>(), Some(namespace), pod)
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("pod {pod}"),
                get_read("", "pods", Some(namespace), pod),
            )
        })?
        .ok_or_else(|| Problem::new(Kind::NotFound, format!("There is no pod named {pod}.")))?;
    serde_json::to_value(found)
        .map_err(|e| ApiError::from(Problem::new(Kind::Internal, e.to_string())))
}

fn right(verb: &str, resource: &str, namespace: &str, name: Option<&str>) -> Denied {
    Denied {
        verb: verb.to_owned(),
        group: String::new(),
        resource: resource.to_owned(),
        name: name.map(str::to_owned),
        namespace: Some(namespace.to_owned()),
    }
}

/// The rights of `wanted` the person lacks, as a warning; none when all are granted or
/// kube-apiserver would not say.
async fn rights_missing(cluster: &dyn Cluster, wanted: Vec<Denied>) -> Option<Warning> {
    let mut rights = Vec::new();
    for right in wanted {
        if matches!(cluster.review_access(&right).await, Ok(false)) {
            rights.push(right);
        }
    }
    (!rights.is_empty()).then_some(Warning::RightsMissing { rights })
}

async fn debug_container(
    planner: &Planner<'_>,
    cluster: &dyn Cluster,
    project: &str,
    pod: &str,
    container: Option<&str>,
    made_at: &str,
) -> Result<Plan, ApiError> {
    let object = read_pod(cluster, project, pod).await?;
    let mut plan =
        plans::debug_container(&object, container, &planner.settings.debug_image, made_at)
            .map_err(plan_problem)?;
    let mut wanted = Vec::new();
    if !plan.objects.is_empty() {
        wanted.push(right(
            "patch",
            "pods/ephemeralcontainers",
            project,
            Some(pod),
        ));
    }
    wanted.push(right("get", "pods/exec", project, Some(pod)));
    plan.warnings.extend(rights_missing(cluster, wanted).await);
    Ok(plan)
}

pub(crate) fn local_mode() -> ApiError {
    let mut problem = Problem::new(
        Kind::BadRequest,
        "A tools terminal needs Zeabur running in the cluster: on your own computer, Zeabur has \
         no credential of yours to hand to a pod.",
    );
    problem.fix_for_occasion(
        "Run kubectl and helm in your own terminal; they use the same kubeconfig as Zeabur.",
        zeabur_core::message::Message::new("problem.badRequest.fix.toolsLocally"),
    );
    ApiError::from(problem)
}

/// The person's tools pod for `scope` that still runs or is starting, among `pods`.
pub(crate) fn running_tools_pod(
    pods: &[Value],
    person: &str,
    scope: ToolsScope<'_>,
) -> Option<String> {
    let key = plans::person_key(person);
    pods.iter()
        .filter(|p| plans::is_tools_pod_of(p, &key, scope))
        .filter(|p| p.pointer("/metadata/deletionTimestamp").is_none())
        .find(|p| {
            matches!(
                p.pointer("/status/phase").and_then(Value::as_str),
                None | Some("Pending" | "Running")
            )
        })
        .and_then(|p| p.pointer("/metadata/name")?.as_str().map(str::to_owned))
}

pub(crate) async fn list_pods(
    cluster: &dyn Cluster,
    namespace: &str,
) -> Result<Vec<Value>, ApiError> {
    let pods = cluster
        .list_kind(&ResourceRef::of::<Pod>(), Some(namespace))
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("the pods of {namespace}"),
                Denied {
                    verb: "list".to_owned(),
                    group: String::new(),
                    resource: "pods".to_owned(),
                    name: None,
                    namespace: Some(namespace.to_owned()),
                },
            )
        })?;
    Ok(pods
        .into_iter()
        .filter_map(|p| serde_json::to_value(p).ok())
        .collect())
}

async fn tools(
    planner: &Planner<'_>,
    cluster: &dyn Cluster,
    project: &str,
    made_at: &str,
) -> Result<Plan, ApiError> {
    let handoff = planner.handoff.ok_or_else(local_mode)?;
    let namespace = planner
        .settings
        .tools_namespace
        .as_deref()
        .unwrap_or(project);
    let pods = list_pods(cluster, namespace).await?;
    let mut plan = tools_plan(
        planner,
        handoff,
        namespace,
        ToolsScope::Project(project),
        &pods,
        made_at,
    );
    if plan.objects.is_empty() {
        return Ok(plan);
    }
    plan.warnings.extend(
        rights_missing(
            cluster,
            vec![
                right("create", "pods", namespace, None),
                right("get", "pods/exec", namespace, None),
                right("delete", "pods", namespace, None),
            ],
        )
        .await,
    );
    Ok(plan)
}

/// The plan that starts the person's tools pod for `scope` in `namespace`, or uses the one among
/// `pods` that still runs.
pub(crate) fn tools_plan(
    planner: &Planner<'_>,
    handoff: &Handoff,
    namespace: &str,
    scope: ToolsScope<'_>,
    pods: &[Value],
    made_at: &str,
) -> Plan {
    if let Some(pod) = running_tools_pod(pods, planner.person, scope) {
        return plans::tools_pod_reused(namespace, &pod);
    }
    let settings = planner.settings;
    plans::tools_pod(&ToolsPod {
        namespace,
        scope,
        person: planner.person,
        admin: handoff.admin,
        credential_expires_at: Some(handoff.expires_at.to_string()),
        image: &settings.tools_image,
        deadline_seconds: settings.tools_deadline_seconds,
        grace_seconds: i64::try_from(settings.tools_grace.as_secs()).unwrap_or(i64::MAX),
        resources: &settings.tools_resources,
        made_at,
    })
}
