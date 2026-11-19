//! Installing and removing Zeabur Catalog components, as the person.
//!
//! | Route | What it does |
//! |---|---|
//! | `POST /api/v1/cluster/components/{id}/plan` | what installing or removing the component writes, step by step, and how far each step has come; writes nothing |
//! | `POST /api/v1/cluster/components/{id}/apply` | writes the next step whose earlier steps are ready, and answers as `plan` |
//!
//! An install is the steps of [`zeabur_catalog::engine::install`]: each release's objects,
//! server-side applied with field manager `zeabur` and never forced, so a field someone else owns
//! fails the write and names them. A release often needs the one before it (cert-manager's
//! `ClusterIssuers` need its CRDs), so `apply` writes one step at a time, with a dry run first,
//! and the page calls it again until every step and health check is ready. The server keeps
//! nothing between calls, and calling `apply` again writes the same objects, which changes
//! nothing.
//!
//! A removal deletes, in reverse order, the objects Zeabur wrote, except namespaces: deleting a
//! `HelmChart` makes helm-controller uninstall its release, and what the catalog lists as
//! `leftovers` stays. It is refused while another installed component needs this one, while
//! objects of its custom resources exist outside its namespaces, or while volumes use its CSI
//! drivers [installers].
//!
//! Every route is refused unless the person may do everything everywhere: writing a `HelmChart`
//! makes helm-controller run Helm as a cluster administrator.
//!
//! [installers]: https://github.com/zeabur-oss/catalog/blob/main/docs/reference/installers.mdx

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::extract::State;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_catalog::api::{ComponentFile, Snapshot};
use zeabur_catalog::engine::{
    self, CHART_NAMESPACE, COMPONENT, Desired, HelmChart, MANAGED_BY, MANAGED_BY_ZEABUR, Progress,
    Refusal, Target,
};
use zeabur_core::problem::{Denied, Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::Resource as _;
use zeabur_kube::k8s_openapi::api::batch::v1::Job;
use zeabur_kube::k8s_openapi::api::core::v1::{Namespace, Node, PersistentVolume};
use zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{
    Cluster, ClusterExt, DynamicObject, ObjectTarget, Preconditions, ResourceRef, ServedResource,
    WriteBody,
};
use zeabur_model::PodSecurityLevel;

use super::node_shells::{require_admin, send, yaml};
use crate::error::ApiError;
use crate::extract::{Json, Path};
use crate::handlers::{AppState, Person};

const WHAT: &str = "installing components";
const WHY: &str = "installing a chart runs Helm as a cluster administrator.";

/// Install or remove.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ComponentAction {
    /// Install the component and what it needs.
    Install,
    /// Remove the component.
    Remove,
}

/// What to plan.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComponentRequest {
    /// Install or remove.
    pub action: ComponentAction,
    /// For `apply`: the catalog release the plan was made from; `409` when Zeabur has read
    /// another since.
    #[serde(default)]
    pub catalog_version: Option<String>,
}

/// How far a step or check has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum StepState {
    /// Not written yet.
    Pending,
    /// Written; the cluster works on it.
    Waiting,
    /// Done.
    Ready,
    /// It will not get ready without a change.
    Failed,
}

/// One write of a step.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ObjectWrite {
    /// The object's kind.
    pub kind: String,
    /// Its name.
    pub name: String,
    /// Its namespace, for a namespaced object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// The object as it is written, YAML; absent for a deletion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yaml: Option<String>,
}

/// One step: a release's objects.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComponentStep {
    /// The component.
    pub component: String,
    /// The release.
    pub release: String,
    /// How far it has come.
    pub state: StepState,
    /// Why it failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// What it writes or deletes.
    pub writes: Vec<ObjectWrite>,
}

/// One health check of the catalog, run once every step is ready.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComponentCheck {
    /// The component.
    pub component: String,
    /// The check.
    pub name: String,
    /// How far it has come; `pending` until every step is ready.
    pub state: StepState,
    /// What to do when it does not pass in time.
    pub advice: String,
    /// How long it may take once every step is ready, in seconds.
    pub timeout_seconds: i64,
}

/// What to know before installing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ComponentWarning {
    /// The catalog's tests have not passed the component on this distribution and platform.
    #[serde(rename_all = "camelCase")]
    Untested {
        /// The component.
        component: String,
    },
    /// The component's namespaces allow pods more than the `restricted` Pod Security level.
    #[serde(rename_all = "camelCase")]
    Privileged {
        /// The component.
        component: String,
        /// `baseline` or `privileged`.
        level: PodSecurityLevel,
    },
}

/// What installing or removing a component writes, and how far it has come.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComponentPlan {
    /// Install or remove.
    pub action: ComponentAction,
    /// The catalog release it is made from; send it to `apply`.
    pub catalog_version: String,
    /// The components it installs, the ones needed first; the one it removes.
    pub components: Vec<String>,
    /// The steps, in order.
    pub steps: Vec<ComponentStep>,
    /// The health checks; none for a removal.
    pub checks: Vec<ComponentCheck>,
    /// What to know.
    pub warnings: Vec<ComponentWarning>,
    /// What a removal leaves, as the catalog lists it (`customresourcedefinitions`,
    /// `secrets/<name>`).
    pub leftovers: Vec<String>,
    /// Every step and check is ready.
    pub done: bool,
}

/// A plan, the objects of each of its steps, and whether something Zeabur installed for the
/// component is on the cluster.
struct Planned {
    plan: ComponentPlan,
    objects: Vec<Vec<Placed>>,
    zeaburs: bool,
}

/// What installing or removing component `id` writes, and how far each step has come; writes
/// nothing. `403` unless the person may do everything everywhere; `503` while Zeabur has never
/// read the catalog; `409` when the component was installed by someone else, or another
/// component needs it; `422` when Zeabur cannot install it on this cluster.
#[utoipa::path(post, path = "/api/v1/cluster/components/{id}/plan", tag = "cluster",
    operation_id = "planComponent", request_body = ComponentRequest,
    params(("id" = String, Path)),
    responses((status = 200, body = ComponentPlan),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM),
              (status = 503, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn plan(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path(id): Path<String>,
    Json(request): Json<ComponentRequest>,
) -> Result<Json<ComponentPlan>, ApiError> {
    require_admin(cluster.as_ref(), WHAT, WHY).await?;
    let world = World::read(&state, cluster.as_ref()).await?;
    let Planned { plan, zeaburs, .. } = world.plan(cluster.as_ref(), &id, request.action).await?;
    if request.action == ComponentAction::Remove && !zeaburs {
        return Err(ApiError::from(Problem::new(
            Kind::Conflict,
            format!("Zeabur did not install {id}, so it does not remove it."),
        )));
    }
    Ok(Json(plan))
}

/// Writes the next step of the plan whose earlier steps are ready, dry run first, and answers as
/// `plan` does. `409` when Zeabur has read another catalog release than the plan's.
#[utoipa::path(post, path = "/api/v1/cluster/components/{id}/apply", tag = "cluster",
    operation_id = "applyComponent", request_body = ComponentRequest,
    params(("id" = String, Path)),
    responses((status = 200, body = ComponentPlan),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM),
              (status = 503, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn apply(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path(id): Path<String>,
    Json(request): Json<ComponentRequest>,
) -> Result<Json<ComponentPlan>, ApiError> {
    require_admin(cluster.as_ref(), WHAT, WHY).await?;
    let world = World::read(&state, cluster.as_ref()).await?;
    let version = &world.snapshot.index.catalog.version;
    if request.catalog_version.as_deref() != Some(version.as_str()) {
        return Err(ApiError::from(Problem::new(
            Kind::Conflict,
            format!("Zeabur now reads Zeabur Catalog {version}. Review the plan again."),
        )));
    }
    let Planned { plan, objects, .. } = world.plan(cluster.as_ref(), &id, request.action).await?;
    let next = plan
        .steps
        .iter()
        .zip(&objects)
        .find(|(s, _)| s.state != StepState::Ready);
    let Some((first, step)) = next else {
        return Ok(Json(plan));
    };
    if first.state == StepState::Failed {
        return Ok(Json(plan));
    }
    // A dry run of an object in a namespace that does not exist yet fails, so namespaces go
    // first.
    let (namespaces, rest): (Vec<&Placed>, Vec<&Placed>) =
        step.iter().partition(|o| o.target.kind == Namespace::KIND);
    for (dry_run, group) in [
        (true, &namespaces),
        (false, &namespaces),
        (true, &rest),
        (false, &rest),
    ] {
        for object in group {
            match request.action {
                ComponentAction::Install => {
                    let Some(body) = &object.body else {
                        continue;
                    };
                    let body = serde_json::to_value(body)
                        .map_err(|e| ApiError::from(Problem::new(Kind::Internal, e.to_string())))?;
                    let target = object.target.clone();
                    let refused = right("patch", &target);
                    send(
                        cluster.as_ref(),
                        target,
                        WriteBody::Apply(body),
                        dry_run,
                        refused,
                    )
                    .await?;
                }
                ComponentAction::Remove => {
                    let Some(uid) = object.uid.clone() else {
                        continue;
                    };
                    let target = object.target.clone();
                    let refused = right("delete", &target);
                    let body = WriteBody::Delete(Preconditions {
                        uid: Some(uid),
                        ..Preconditions::default()
                    });
                    send(cluster.as_ref(), target, body, dry_run, refused).await?;
                }
            }
        }
    }
    Ok(Json(
        world
            .plan(cluster.as_ref(), &id, request.action)
            .await?
            .plan,
    ))
}

fn right(verb: &str, target: &ObjectTarget) -> Denied {
    let group = target
        .api_version
        .split_once('/')
        .map_or("", |(g, _)| g)
        .to_owned();
    Denied {
        verb: verb.to_owned(),
        group,
        resource: target.plural.clone(),
        name: Some(target.name.clone()),
        namespace: target.namespace().map(str::to_owned),
    }
}

/// One object of a step, as written or to delete.
struct Placed {
    target: ObjectTarget,
    /// For an install: the object to apply.
    body: Option<DynamicObject>,
    /// For a removal: the uid of the object found, `None` when it is gone.
    uid: Option<String>,
}

/// One install step against the cluster.
struct PlannedStep {
    view: ComponentStep,
    placed: Vec<Placed>,
    /// Zeabur wrote an object of the step.
    ours: bool,
    /// An object of the step that exists and that Zeabur did not write.
    foreign: Option<String>,
}

/// What a plan is made against.
struct World {
    snapshot: Arc<Snapshot>,
    served: Vec<ServedResource>,
    profile: &'static str,
    distribution: Option<&'static str>,
    platform: String,
    capabilities: BTreeSet<String>,
    installed: BTreeSet<String>,
    helm_releases: BTreeSet<String>,
    now: Timestamp,
}

impl World {
    async fn read(state: &AppState, cluster: &dyn Cluster) -> Result<Self, ApiError> {
        let current = super::catalog::current(state).await;
        let snapshot = current.snapshot.ok_or_else(|| {
            ApiError::from(Problem::new(
                Kind::Unavailable,
                format!(
                    "Zeabur cannot reach Zeabur Catalog at {}: {}",
                    state.catalog.url(),
                    current.error.unwrap_or_default()
                ),
            ))
        })?;
        let facts = cluster
            .cluster_facts()
            .await
            .map_err(|e| ApiError::from(e.into_problem("the cluster's installed components")))?;
        let detected = zeabur_catalog::detect(&state.catalogue, &facts);
        let capabilities = detected
            .iter()
            .filter(|c| c.existing)
            .map(|c| c.name.clone())
            .collect();
        let installed = detected
            .iter()
            .flat_map(|c| c.installations.iter().map(|i| i.component.clone()))
            .collect();
        let served = cluster
            .served_resources()
            .await
            .map_err(|e| ApiError::from(e.into_problem("the cluster's resources")))?;
        if !served
            .iter()
            .any(|r| r.group == HelmChart::GROUP && r.kind == HelmChart::KIND)
        {
            return Err(ApiError::from(Problem::new(
                Kind::Invalid,
                "Zeabur installs components with helm-controller, which K3s and RKE2 run, and \
                 this cluster has none.",
            )));
        }
        let nodes: Vec<Node> = cluster
            .list(None)
            .await
            .map_err(|e| ApiError::from(e.into_problem("the cluster's nodes")))?;
        let platform = nodes
            .first()
            .and_then(|n| n.metadata.labels.as_ref())
            .and_then(|l| l.get("kubernetes.io/arch"))
            .map_or_else(|| "linux/amd64".to_owned(), |arch| format!("linux/{arch}"));
        Ok(Self {
            snapshot,
            served,
            profile: zeabur_catalog::profile(&facts),
            distribution: zeabur_catalog::distribution(&facts),
            platform,
            capabilities,
            installed,
            helm_releases: facts.helm_releases.clone(),
            now: (state.clock)(),
        })
    }

    async fn plan(
        &self,
        cluster: &dyn Cluster,
        id: &str,
        action: ComponentAction,
    ) -> Result<Planned, ApiError> {
        match action {
            ComponentAction::Install => self.install(cluster, id).await,
            ComponentAction::Remove => self.remove(cluster, id).await,
        }
    }

    fn target(&self) -> Target<'_> {
        Target {
            profile: self.profile,
            distribution: self.distribution,
            platform: &self.platform,
            capabilities: &self.capabilities,
            components: &self.installed,
        }
    }

    async fn install(&self, cluster: &dyn Cluster, id: &str) -> Result<Planned, ApiError> {
        let install =
            engine::install(&self.snapshot, id, &self.target()).map_err(|r| refusal(&r))?;
        let mut steps = Vec::new();
        let mut placed_steps = Vec::new();
        let mut managed = BTreeSet::new();
        for step in &install.steps {
            let earlier_ready = steps
                .iter()
                .all(|s: &ComponentStep| s.state == StepState::Ready);
            let planned = self.install_step(cluster, step, earlier_ready).await?;
            if let Some(object) = planned.foreign {
                return Err(ApiError::from(Problem::new(
                    Kind::Conflict,
                    format!(
                        "{object} of {} already exists, and Zeabur did not write it. Zeabur does \
                         not change a component it did not install.",
                        step.component
                    ),
                )));
            }
            if planned.ours {
                managed.insert(step.component.clone());
            }
            steps.push(planned.view);
            placed_steps.push(planned.placed);
        }
        // A Helm release of the same name that no `HelmChart` of Zeabur's made: removing the
        // chart would uninstall someone else's release.
        let elsewhere = install.steps.iter().find(|step| {
            let Some(file) = self.snapshot.components.get(&step.component) else {
                return false;
            };
            !managed.contains(&step.component)
                && file.declaration.releases.iter().any(|r| {
                    r.name == step.release
                        && self.helm_releases.contains(&format!(
                            "{}/{}",
                            r.namespace,
                            r.helm_name()
                        ))
                })
        });
        if let Some(elsewhere) = elsewhere.map(|s| &s.component) {
            return Err(ApiError::from(Problem::new(
                Kind::Conflict,
                format!(
                    "{elsewhere} is already installed, and not by Zeabur. Zeabur does not change \
                     a component it did not install."
                ),
            )));
        }
        let ready = steps.iter().all(|s| s.state == StepState::Ready);
        let mut checks = Vec::new();
        for component in &install.components {
            let Some(file) = self.snapshot.components.get(component) else {
                continue;
            };
            checks.extend(self.checks(cluster, file, ready).await?);
        }
        let done = ready && checks.iter().all(|c| c.state == StepState::Ready);
        let warnings = install
            .warnings
            .into_iter()
            .map(|w| match w {
                engine::Warning::Untested { component } => ComponentWarning::Untested { component },
                engine::Warning::Privileged { component, level } => {
                    ComponentWarning::Privileged { component, level }
                }
            })
            .collect();
        Ok(Planned {
            plan: ComponentPlan {
                action: ComponentAction::Install,
                catalog_version: self.snapshot.index.catalog.version.clone(),
                components: install.components,
                steps,
                checks,
                warnings,
                leftovers: Vec::new(),
                done,
            },
            objects: placed_steps,
            zeaburs: !managed.is_empty(),
        })
    }

    /// One step of an install: how far it has come, its objects, and whether Zeabur wrote one
    /// of them. An object whose kind the cluster does not serve waits for an earlier step, and
    /// fails once those are ready.
    async fn install_step(
        &self,
        cluster: &dyn Cluster,
        step: &engine::Step,
        earlier_ready: bool,
    ) -> Result<PlannedStep, ApiError> {
        let mut progress = Vec::new();
        let mut writes = Vec::new();
        let mut placed = Vec::new();
        let mut zeaburs = false;
        let mut foreign = None;
        for desired in &step.objects {
            let Some(target) = self.place(desired) else {
                let kind = kind(&desired.object);
                progress.push(if earlier_ready {
                    Progress::Failed(format!("the cluster does not serve {kind}"))
                } else {
                    Progress::Pending
                });
                writes.push(view(&desired.object, None, Some(&desired.namespace)));
                continue;
            };
            let mut body = desired.object.clone();
            if let Some(namespace) = target.namespace() {
                body.metadata.namespace = Some(namespace.to_owned());
            }
            let found = read(cluster, &target).await?;
            if let Some(found) = &found {
                if target.kind == Namespace::KIND {
                    keep_looser_level(&mut body.metadata, &found.metadata);
                } else if ours(&found.metadata, &step.component) {
                    zeaburs = true;
                } else {
                    foreign.get_or_insert_with(|| format!("{} {}", target.kind, target.name));
                }
            }
            progress.push(self.progress(cluster, &target, found.as_ref()).await?);
            writes.push(view(&body, Some(&target), None));
            placed.push(Placed {
                target,
                body: Some(body),
                uid: None,
            });
        }
        let (state, message) = combine(&progress);
        let view = ComponentStep {
            component: step.component.clone(),
            release: step.release.clone(),
            state,
            message,
            writes,
        };
        Ok(PlannedStep {
            view,
            placed,
            ours: zeaburs,
            foreign,
        })
    }

    async fn remove(&self, cluster: &dyn Cluster, id: &str) -> Result<Planned, ApiError> {
        let file = self
            .snapshot
            .components
            .get(id)
            .ok_or_else(|| refusal(&Refusal::Unknown(id.to_owned())))?;
        let needing = engine::needed_by(&self.snapshot, id, &self.installed);
        if !needing.is_empty() {
            return Err(ApiError::from(Problem::new(
                Kind::Conflict,
                format!("{} needs {id}. Remove it first.", needing.join(", ")),
            )));
        }
        let version = &self.snapshot.index.catalog.version;
        let own = engine::steps_of(file, version, self.profile).map_err(|r| refusal(&r))?;
        let mut steps = Vec::new();
        let mut placed_steps = Vec::new();
        let mut zeaburs = false;
        for step in own.iter().rev() {
            let mut writes = Vec::new();
            let mut placed = Vec::new();
            let mut progress = Vec::new();
            for desired in step.objects.iter().rev() {
                if kind(&desired.object) == Namespace::KIND {
                    continue;
                }
                let Some(target) = self.place(desired) else {
                    continue;
                };
                let found = read(cluster, &target)
                    .await?
                    .filter(|o| ours(&o.metadata, id));
                zeaburs |= found.is_some();
                progress.push(match &found {
                    None => Progress::Ready,
                    Some(o) if o.metadata.deletion_timestamp.is_some() => Progress::Waiting,
                    Some(_) => Progress::Pending,
                });
                let mut write = view(&desired.object, Some(&target), None);
                write.yaml = None;
                writes.push(write);
                placed.push(Placed {
                    uid: found.and_then(|o| o.metadata.uid),
                    target,
                    body: None,
                });
            }
            let (state, message) = combine(&progress);
            steps.push(ComponentStep {
                component: step.component.clone(),
                release: step.release.clone(),
                state,
                message,
                writes,
            });
            placed_steps.push(placed);
        }
        if zeaburs {
            self.still_used(cluster, file).await?;
        }
        Ok(Planned {
            plan: ComponentPlan {
                action: ComponentAction::Remove,
                catalog_version: version.clone(),
                components: vec![id.to_owned()],
                done: steps.iter().all(|s| s.state == StepState::Ready),
                steps,
                checks: Vec::new(),
                warnings: Vec::new(),
                leftovers: file.declaration.leftovers.clone(),
            },
            objects: placed_steps,
            zeaburs,
        })
    }

    /// Refuses a removal while objects of the component's custom resources exist outside its
    /// namespaces, or volumes use its CSI drivers [installers].
    ///
    /// [installers]: https://github.com/zeabur-oss/catalog/blob/main/docs/reference/installers.mdx
    async fn still_used(
        &self,
        cluster: &dyn Cluster,
        file: &ComponentFile,
    ) -> Result<(), ApiError> {
        let d = &file.declaration;
        let own: BTreeSet<&str> = d.releases.iter().map(|r| r.namespace.as_str()).collect();
        let mut users = Vec::new();
        for crd in &d.detection.crds {
            if d.internal_crds.contains(crd) {
                continue;
            }
            let Some((plural, group)) = crd.split_once('.') else {
                continue;
            };
            let Some(served) = self
                .served
                .iter()
                .find(|r| r.group == group && r.resource == plural)
            else {
                continue;
            };
            let resource = ResourceRef::new(
                &served.group,
                &served.version,
                &served.resource,
                &served.kind,
            );
            let objects = cluster
                .list_kind(&resource, None)
                .await
                .map_err(|e| ApiError::from(e.into_problem(&format!("{crd} objects"))))?;
            for object in objects {
                let namespace = object.metadata.namespace.clone().unwrap_or_default();
                let ours = object
                    .metadata
                    .labels
                    .as_ref()
                    .and_then(|l| l.get(COMPONENT))
                    .is_some_and(|c| *c == file.id);
                if !ours && !own.contains(namespace.as_str()) {
                    let name = object.metadata.name.unwrap_or_default();
                    users.push(if namespace.is_empty() {
                        format!("{} {name}", served.kind)
                    } else {
                        format!("{} {namespace}/{name}", served.kind)
                    });
                }
            }
        }
        if !d.detection.csi_drivers.is_empty() {
            let volumes: Vec<PersistentVolume> = cluster
                .list(None)
                .await
                .map_err(|e| ApiError::from(e.into_problem("the cluster's volumes")))?;
            for volume in volumes {
                let driver = volume
                    .spec
                    .as_ref()
                    .and_then(|s| s.csi.as_ref())
                    .map(|c| c.driver.clone());
                if driver.is_some_and(|d| file.declaration.detection.csi_drivers.contains(&d)) {
                    users.push(format!(
                        "PersistentVolume {}",
                        volume.metadata.name.unwrap_or_default()
                    ));
                }
            }
        }
        if users.is_empty() {
            return Ok(());
        }
        let more = users.len().saturating_sub(5);
        users.truncate(5);
        let list = if more > 0 {
            format!("{} and {more} more", users.join(", "))
        } else {
            users.join(", ")
        };
        Err(ApiError::from(Problem::new(
            Kind::Conflict,
            format!(
                "{} still uses {}: {list}. Delete them first.",
                file.id, file.id
            ),
        )))
    }

    /// Where an object goes: its resource from discovery and its namespace; `None` while the
    /// cluster does not serve its kind.
    fn place(&self, desired: &Desired) -> Option<ObjectTarget> {
        let types = desired.object.types.as_ref()?;
        let metadata = &desired.object.metadata;
        let name = metadata.name.clone()?;
        let (group, _) = types
            .api_version
            .split_once('/')
            .unwrap_or(("", &types.api_version));
        let served = self
            .served
            .iter()
            .find(|r| r.group == group && r.kind == types.kind)?;
        let namespace = if served.namespaced {
            metadata
                .namespace
                .clone()
                .unwrap_or_else(|| desired.namespace.clone())
        } else {
            String::new()
        };
        Some(ObjectTarget {
            api_version: types.api_version.clone(),
            kind: types.kind.clone(),
            plural: served.resource.clone(),
            namespace,
            name,
        })
    }

    async fn progress(
        &self,
        cluster: &dyn Cluster,
        target: &ObjectTarget,
        found: Option<&DynamicObject>,
    ) -> Result<Progress, ApiError> {
        let Some(found) = found else {
            return Ok(Progress::Pending);
        };
        if target.kind != HelmChart::KIND {
            return Ok(engine::object_progress(Some(found)));
        }
        let chart: HelmChart = found.clone().try_parse().unwrap_or_default();
        let job_name = chart.status.as_ref().and_then(|s| s.job_name.clone());
        let job = match job_name {
            Some(name) => {
                let resource = ResourceRef::of::<Job>();
                cluster
                    .find_object(&resource, Some(CHART_NAMESPACE), &name)
                    .await
                    .map_err(|e| ApiError::from(e.into_problem(&format!("Job {name}"))))?
                    .and_then(|o| o.try_parse::<Job>().ok())
            }
            None => None,
        };
        Ok(engine::chart_progress(&chart, job.as_ref()))
    }

    async fn checks(
        &self,
        cluster: &dyn Cluster,
        file: &ComponentFile,
        ready: bool,
    ) -> Result<Vec<ComponentCheck>, ApiError> {
        let now = self.now;
        let mut checks = Vec::new();
        for check in &file.declaration.health {
            let state = if ready {
                let objects = self.check_objects(cluster, file, check).await?;
                match engine::check_progress(check, &objects, now) {
                    Progress::Ready => StepState::Ready,
                    Progress::Failed(_) => StepState::Failed,
                    Progress::Pending | Progress::Waiting => StepState::Waiting,
                }
            } else {
                StepState::Pending
            };
            checks.push(ComponentCheck {
                component: file.id.clone(),
                name: check.name.clone(),
                state,
                advice: check.advice.clone(),
                timeout_seconds: engine::timeout_seconds(&check.timeout),
            });
        }
        Ok(checks)
    }

    async fn check_objects(
        &self,
        cluster: &dyn Cluster,
        file: &ComponentFile,
        check: &zeabur_catalog::api::Health,
    ) -> Result<Vec<Value>, ApiError> {
        let r = &check.resource;
        let Some((group, version, plural, kind)) = engine::health_resource(&r.kind) else {
            return Ok(Vec::new());
        };
        let resource = ResourceRef::new(group, version, plural, kind);
        let namespace = r.namespace.clone().or_else(|| {
            file.declaration
                .releases
                .first()
                .map(|x| x.namespace.clone())
        });
        let namespaced = !matches!(kind, "APIService" | "GatewayClass");
        let namespace = namespaced.then_some(namespace).flatten();
        let what = format!("{kind} objects of {}", file.id);
        if let Some(name) = &r.name {
            let found = cluster
                .find_object(&resource, namespace.as_deref(), name)
                .await
                .map_err(|e| ApiError::from(e.into_problem(&what)))?;
            return Ok(found
                .and_then(|o| serde_json::to_value(o).ok())
                .into_iter()
                .collect());
        }
        let objects = cluster
            .list_objects(&resource, namespace.as_deref())
            .await
            .map_err(|e| ApiError::from(e.into_problem(&what)))?
            .unwrap_or_default();
        let selector = r.selector.clone().unwrap_or_default();
        Ok(objects
            .into_iter()
            .filter(|o| {
                let labels = o.metadata.labels.clone().unwrap_or_default();
                selector.iter().all(|(k, v)| labels.get(k) == Some(v))
            })
            .filter_map(|o| serde_json::to_value(o).ok())
            .collect())
    }
}

/// A step's state from its objects': failed, else pending, else waiting, else ready.
fn combine(progress: &[Progress]) -> (StepState, Option<String>) {
    if let Some(message) = progress.iter().find_map(|p| match p {
        Progress::Failed(m) => Some(m.clone()),
        Progress::Pending | Progress::Waiting | Progress::Ready => None,
    }) {
        return (StepState::Failed, Some(message));
    }
    let state = if progress.contains(&Progress::Pending) {
        StepState::Pending
    } else if progress.contains(&Progress::Waiting) {
        StepState::Waiting
    } else {
        StepState::Ready
    };
    (state, None)
}

async fn read(
    cluster: &dyn Cluster,
    target: &ObjectTarget,
) -> Result<Option<DynamicObject>, ApiError> {
    cluster
        .find_object(&target.resource(), target.namespace(), &target.name)
        .await
        .map_err(|e| ApiError::from(e.into_problem(&format!("{} {}", target.kind, target.name))))
}

/// Keeps a namespace's Pod Security level when it is looser than the desired one: components
/// share namespaces, and one that needs `privileged` must not lose it to one that needs less.
fn keep_looser_level(desired: &mut ObjectMeta, found: &ObjectMeta) {
    let level = |m: &ObjectMeta| {
        m.labels
            .as_ref()?
            .get(PodSecurityLevel::ENFORCE)
            .and_then(|l| PodSecurityLevel::parse(l))
    };
    if let Some(current) = level(found)
        && level(desired).is_none_or(|wanted| current < wanted)
    {
        desired.labels.get_or_insert_default().insert(
            PodSecurityLevel::ENFORCE.to_owned(),
            current.as_str().to_owned(),
        );
    }
}

/// Whether Zeabur wrote the object for `component`.
fn ours(metadata: &ObjectMeta, component: &str) -> bool {
    metadata.labels.as_ref().is_some_and(|l| {
        l.get(COMPONENT).is_some_and(|c| c == component)
            && l.get(MANAGED_BY).is_some_and(|m| m == MANAGED_BY_ZEABUR)
    })
}

fn kind(object: &DynamicObject) -> &str {
    object.types.as_ref().map_or("", |t| t.kind.as_str())
}

fn view(
    object: &DynamicObject,
    target: Option<&ObjectTarget>,
    namespace: Option<&str>,
) -> ObjectWrite {
    ObjectWrite {
        kind: kind(object).to_owned(),
        name: object.metadata.name.clone().unwrap_or_default(),
        namespace: target
            .and_then(ObjectTarget::namespace)
            .or(namespace)
            .filter(|n| !n.is_empty())
            .map(str::to_owned),
        yaml: serde_json::to_value(object).ok().as_ref().and_then(yaml),
    }
}

fn refusal(refusal: &Refusal) -> ApiError {
    ApiError::from(Problem::new(Kind::Invalid, refusal.to_string()))
}
