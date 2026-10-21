//! Whether a pod has a debug container of Zeabur's for a container, and how it is doing.
//!
//! A debug container is added by a plan (the `debugContainer` change; see
//! `zeabur_plan::terminal`), then the terminal opens in it through `pods/exec` like any other
//! container (`…/exec?container=zeabur-debug-…`). Between the two, its image is pulled and it
//! starts; this says when it runs, and in plain words why it does not.

#![expect(
    unused_qualifications,
    reason = "utoipa's `path` macro expands `params(DebugParams)` to a qualified path"
)]

use axum::Json;
use axum::extract::{Path, RawQuery};
use serde::Serialize;
use utoipa::{IntoParams, ToSchema};
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_plan::terminal::{self, ContainerState, DebugContainer};

use crate::error::ApiError;
use crate::handlers::Person;
use crate::projects::writes::plan_problem;
use crate::projects::writes::terminal::read_pod;

/// The parameters of a debug container's status.
#[expect(
    dead_code,
    reason = "describes the parameters in the OpenAPI document; parsed by hand"
)]
#[derive(Debug, Default, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct DebugParams {
    /// The container it shares processes with; by default the pod's default container.
    container: Option<String>,
}

/// The state of a debug container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum DebugState {
    /// It runs: a terminal can open in it.
    Running,
    /// It is starting (its image is being pulled), or cannot start (see `problem`).
    Waiting,
    /// Its shell ended; it cannot start again, so a terminal needs a new one.
    Terminated,
    /// The kubelet has not reported it yet.
    Unknown,
}

/// A debug container of Zeabur's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebugContainerStatus {
    /// Its name: the terminal's `container`.
    pub name: String,
    /// Its image.
    pub image: String,
    /// Its state.
    pub state: DebugState,
    /// The kubelet's reason (`ErrImagePull`, `Completed`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// A pod's debug container for one of its containers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebugStatus {
    /// The pod.
    pub pod: String,
    /// The container it shares processes with.
    pub target: String,
    /// Zeabur's debug container for it: the newest that runs or is starting, else the newest;
    /// absent when there is none, and a plan adds one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<DebugContainerStatus>,
    /// Why it does not start, when it cannot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<Box<Problem>>,
}

const PULL_FAILURES: [&str; 4] = [
    "ErrImagePull",
    "ImagePullBackOff",
    "InvalidImageName",
    "ErrImageNeverPull",
];

/// Why a container that waits cannot start, in plain words: its image cannot be pulled (the fix
/// names the chart value `setting` that chooses it), or the runtime refuses it. `None` while it is
/// only starting.
pub(crate) fn start_problem(
    image: &str,
    container: &str,
    reason: &str,
    message: Option<&str>,
    setting: &str,
) -> Option<Box<Problem>> {
    let said = message.map_or_else(String::new, |m| format!(" The kubelet says: {m}"));
    if PULL_FAILURES.contains(&reason) {
        let mut problem = Problem::new(
            Kind::Conflict,
            format!("The node could not pull the image {image} ({reason}).{said}"),
        );
        problem.fix_for_occasion(
            format!(
                "Ask a cluster administrator to let the nodes pull this image, or to set \
                 `{setting}` in Zeabur's chart to one they can pull."
            ),
            zeabur_core::message::Message::new("problem.conflict.fix.terminalImage")
                .with("image", image)
                .with("setting", setting),
        );
        Some(Box::new(problem))
    } else if matches!(
        reason,
        "CreateContainerConfigError" | "CreateContainerError" | "RunContainerError"
    ) {
        let mut problem = Problem::new(
            Kind::Conflict,
            format!("Container {container} could not start ({reason}).{said}"),
        );
        problem.fix_for_occasion(
            "Read the pod's events for the reason; a policy may forbid the user it runs as.",
            zeabur_core::message::Message::new("problem.conflict.fix.terminalContainerStart"),
        );
        Some(Box::new(problem))
    } else {
        None
    }
}

fn status(container: &DebugContainer) -> (DebugContainerStatus, Option<Box<Problem>>) {
    let (state, reason, message) = match &container.state {
        ContainerState::Running => (DebugState::Running, None, None),
        ContainerState::Waiting { reason, message } => {
            (DebugState::Waiting, reason.clone(), message.clone())
        }
        ContainerState::Terminated { reason, .. } => (DebugState::Terminated, reason.clone(), None),
        ContainerState::Unknown => (DebugState::Unknown, None, None),
    };
    let problem = match state {
        DebugState::Waiting => reason.as_deref().and_then(|reason| {
            start_problem(
                &container.image,
                &container.name,
                reason,
                message.as_deref(),
                "terminal.debugImage",
            )
        }),
        DebugState::Running | DebugState::Terminated | DebugState::Unknown => None,
    };
    (
        DebugContainerStatus {
            name: container.name.clone(),
            image: container.image.clone(),
            state,
            reason,
        },
        problem,
    )
}

/// Whether the pod has a debug container of Zeabur's for a container, and its state.
///
/// Poll it after applying a `debugContainer` plan until `container.state` is `running`, then
/// open the terminal with `container` set to its name. `problem` says, in plain words, when its
/// image cannot be pulled or it cannot start.
#[utoipa::path(get, path = "/api/v1/projects/{project}/pods/{pod}/debug", tag = "pods",
    operation_id = "getDebugContainer",
    params(("project" = String, Path), ("pod" = String, Path), DebugParams),
    responses((status = 200, body = DebugStatus),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    Person(cluster): Person,
    Path((project, pod)): Path<(String, String)>,
    RawQuery(query): RawQuery,
) -> Result<Json<DebugStatus>, ApiError> {
    let asked = form_urlencoded::parse(query.unwrap_or_default().as_bytes())
        .find(|(name, value)| name == "container" && !value.is_empty())
        .map(|(_, value)| value.into_owned());
    let object = read_pod(cluster.as_ref(), &project, &pod).await?;
    let target = terminal::target_container(&object, asked.as_deref()).map_err(plan_problem)?;
    let ours: Vec<DebugContainer> = terminal::debug_containers(&object)
        .into_iter()
        .filter(|c| c.target.as_deref() == Some(target.as_str()))
        .collect();
    let newest = ours.iter().rfind(|c| c.usable()).or_else(|| ours.last());
    let (container, problem) = match newest.map(status) {
        Some((container, problem)) => (Some(container), problem),
        None => (None, None),
    };
    Ok(Json(DebugStatus {
        pod,
        target,
        container,
        problem,
    }))
}
