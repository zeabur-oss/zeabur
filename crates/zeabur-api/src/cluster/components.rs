//! Installing and removing Zeabur Catalog components, as the person.
//!
//! | Route | What it does |
//! |---|---|
//! | `POST /api/v1/cluster/components/{id}/plan` | what installing or removing the component writes, step by step, and how far each step has come; writes nothing |
//! | `POST /api/v1/cluster/components/{id}/apply` | writes the next step whose earlier steps are ready, and answers with each write's result and the plan |
//!
//! An install is the steps of [`zeabur_catalog::engine::install`]: each release's objects,
//! with the files of manifest releases downloaded and checked by
//! [`super::catalog::Catalog::manifests`], server-side applied with field manager `zeabur` and
//! never forced, so a field someone else owns fails the write and names them. A release often needs
//! the one before it (cert-manager's `ClusterIssuers` need its CRDs), so `apply` writes one step at
//! a time, with a dry run first, and the page calls it again until every step and health check is
//! ready. A component's health checks run once its steps are ready, and the next component's steps
//! wait for them. The server keeps nothing between calls, and calling `apply` again writes the same
//! objects, which changes nothing. A release with `skipIfPresent` is left out when every CRD it
//! lists exists and Zeabur did not write it.
//!
//! A component that adds values to another's release writes that release again, as
//! [`zeabur_catalog::engine`] describes. Zeabur never changes a `HelmChartConfig` it did not
//! write: the plan warns, with the values to add, and leaves it as it is.
//!
//! A removal first writes again the releases the component added values to, then deletes, in
//! reverse order, the objects Zeabur wrote, except namespaces and CRDs: deleting a `HelmChart`
//! makes helm-controller uninstall its release, and what the catalog lists as `leftovers` stays.
//! Deleting a CRD deletes every object of its kind, including ones a person or another component
//! made, so the CRDs Zeabur wrote are added to the leftovers. It is refused while a component that
//! another tool or Zeabur installed needs this one, while objects of its custom resources exist
//! outside its namespaces, or while volumes use its CSI drivers [installers].
//!
//! A component's inputs travel in `values`, the same in `plan` and every `apply`. `path` inputs go
//! into their release's values, and the given ones are kept in the `catalog.zeabur.com/inputs`
//! annotation of its `HelmChart`; `secret` inputs go into Secrets written before the release. A
//! later plan reads both back, so an upgrade or a change of one value keeps the others, and a
//! generated Secret key is never made again. A removal leaves the Secrets. A `HelmChart` or Secret
//! that holds other content than the plan's is written again.
//!
//! Every route is refused unless the person may do everything everywhere: writing a `HelmChart`
//! makes helm-controller run Helm as a cluster administrator.
//!
//! [installers]: https://github.com/zeabur-oss/catalog/blob/main/docs/reference/installers.mdx

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use axum::extract::State;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use zeabur_catalog::InstallSource;
use zeabur_catalog::api::{ComponentFile, Snapshot};
use zeabur_catalog::engine::{
    self, CHART_NAMESPACE, COMPONENT, Desired, FIELD_MANAGER, HelmChart, HelmChartConfig, INPUTS,
    InputKind, InputState, Inputs, MANAGED_BY, MANAGED_BY_ZEABUR, Manifests, Progress, Refusal,
    Target, Values,
};
use zeabur_catalog::format::common::Generate;
use zeabur_catalog::format::component::{Health, InputValue, Release};
use zeabur_core::message::Message;
use zeabur_core::problem::{Denied, Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::api::batch::v1::Job;
use zeabur_kube::k8s_openapi::api::core::v1::{Namespace, Node, PersistentVolume, Secret};
use zeabur_kube::k8s_openapi::api::networking::v1::{IPAddress, ServiceCIDR};
use zeabur_kube::k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::k8s_openapi::{ByteString, Resource as _};
use zeabur_kube::{
    Cluster, ClusterExt, DynamicObject, ObjectTarget, Preconditions, ResourceRef, ServedResource,
    WriteBody,
};
use zeabur_model::PodSecurityLevel;

use super::catalog::Catalog;
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComponentRequest {
    /// Install or remove.
    pub action: ComponentAction,
    /// For `apply`: the catalog release the plan was made from; `409` when Zeabur has read
    /// another since.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_version: Option<String>,
    /// For an install: the values of inputs, by component and then by input name. Send the same
    /// to `plan` and to every `apply`; an input left out keeps the value an earlier install
    /// wrote, and an empty text is no value.
    #[serde(default)]
    #[schema(value_type = BTreeMap<String, BTreeMap<String, InputValueSchema>>)]
    pub values: Values,
}

/// An input's value: the catalog's `default`, or what a person gives.
#[derive(ToSchema)]
#[schema(as = InputValue)]
#[serde(untagged)]
#[expect(dead_code, reason = "only the schema of the catalog's InputValue")]
enum InputValueSchema {
    /// `true` or `false`.
    Flag(bool),
    /// A whole number.
    Integer(i64),
    /// Text.
    Text(String),
}

/// How far a step or check has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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

/// What one write of `apply` left on the cluster.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ObjectWritten {
    /// The object's kind.
    pub kind: String,
    /// Its name.
    pub name: String,
    /// Its namespace, for a namespaced object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// `metadata.uid` after the write; absent for a deletion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    /// `metadata.resourceVersion` after the write, when the cluster answered with the object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_version: Option<String>,
    /// `metadata.generation` after the write, for the kinds that keep one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<i64>,
}

/// What `apply` wrote, and the plan as it is after the writes.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComponentApplied {
    /// The plan after the writes, as `plan` answers it.
    pub plan: ComponentPlan,
    /// Each object written, in order; empty when no step was due.
    pub written: Vec<ObjectWritten>,
}

/// One step: a release's objects.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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

/// One health check of the catalog, run once the component's steps are ready. The steps of the
/// components after it wait for it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
    /// Why it failed: the object, and its kstatus reason or the condition that says so.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// How long it may take once every step is ready, in seconds.
    pub timeout_seconds: i64,
}

/// What to know before installing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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
    /// Another tool installed or configured a component the install adds values to, so Zeabur
    /// leaves it as it is.
    #[serde(rename_all = "camelCase")]
    Unchanged {
        /// The component.
        component: String,
        /// The values to add to it, YAML.
        values: String,
    },
}

impl From<engine::Warning> for ComponentWarning {
    fn from(warning: engine::Warning) -> Self {
        match warning {
            engine::Warning::Untested { component } => Self::Untested { component },
            engine::Warning::Privileged { component, level } => {
                Self::Privileged { component, level }
            }
            engine::Warning::Unchanged { component, values } => {
                Self::Unchanged { component, values }
            }
        }
    }
}

/// An input of a component in the plan, with the value the plan writes.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComponentInput {
    /// The component.
    pub component: String,
    /// The input's name, the key in `values`.
    pub name: String,
    /// What it is, in English, from the catalog.
    pub description: String,
    /// How to ask for it.
    pub kind: InputKind,
    /// An install needs a value.
    pub required: bool,
    /// The catalog's value when none is given.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<InputValueSchema>)]
    pub default: Option<InputValue>,
    /// Zeabur makes a random value when none is given or kept.
    pub generated: bool,
    /// Where the plan's value comes from.
    pub state: InputState,
    /// The value the plan writes; never for a Secret.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<InputValueSchema>)]
    pub value: Option<InputValue>,
}

/// What installing or removing a component writes, and how far it has come.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
    /// The inputs of the components it installs, in order; none for a removal. `apply` refuses
    /// while one is `missing`.
    pub inputs: Vec<ComponentInput>,
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
    /// The first required input without a value.
    missing: Option<Refusal>,
    /// The next step waits for the health checks of a component before it, so apply writes
    /// nothing yet.
    held: bool,
}

/// What installing or removing component `id` writes, and how far each step has come; writes
/// nothing. `403` unless the person may do everything everywhere; `503` while Zeabur has never
/// read the catalog; `409` when the component was installed by someone else, or another
/// component needs it; `422` when Zeabur cannot install it on this cluster, or a value is not of
/// its input's kind. A required input without a value is `missing` in the plan.
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
    let Planned { plan, zeaburs, .. } = world.plan(cluster.as_ref(), &id, &request).await?;
    if request.action == ComponentAction::Remove && !zeaburs {
        return Err(ApiError::from(Problem::new(
            Kind::Conflict,
            format!("Zeabur did not install {id}, so it does not remove it."),
        )));
    }
    Ok(Json(plan))
}

/// Writes the next step of the plan whose earlier steps are ready, dry run first, and answers with
/// each write's result and the plan after it. `409` when Zeabur has read another catalog release
/// than the plan's; `422` while a required input has no value.
#[utoipa::path(post, path = "/api/v1/cluster/components/{id}/apply", tag = "cluster",
    operation_id = "applyComponent", request_body = ComponentRequest,
    params(("id" = String, Path)),
    responses((status = 200, body = ComponentApplied),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM),
              (status = 503, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn apply(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path(id): Path<String>,
    Json(request): Json<ComponentRequest>,
) -> Result<Json<ComponentApplied>, ApiError> {
    require_admin(cluster.as_ref(), WHAT, WHY).await?;
    let world = World::read(&state, cluster.as_ref()).await?;
    let version = &world.snapshot.index.catalog.version;
    if request.catalog_version.as_deref() != Some(version.as_str()) {
        return Err(ApiError::from(Problem::new(
            Kind::Conflict,
            format!("Zeabur now reads Zeabur Catalog {version}. Review the plan again."),
        )));
    }
    let Planned {
        plan,
        objects,
        missing,
        held,
        ..
    } = world.plan(cluster.as_ref(), &id, &request).await?;
    if let Some(missing) = missing {
        return Err(refusal(&missing));
    }
    let next = plan
        .steps
        .iter()
        .zip(&objects)
        .find(|(s, _)| s.state != StepState::Ready);
    let unwritten = |plan| {
        Json(ComponentApplied {
            plan,
            written: Vec::new(),
        })
    };
    let Some((first, step)) = next else {
        return Ok(unwritten(plan));
    };
    if first.state == StepState::Failed || held {
        return Ok(unwritten(plan));
    }
    // A dry run of an object in a namespace that does not exist yet fails, so namespaces go
    // first.
    let (namespaces, rest): (Vec<&Placed>, Vec<&Placed>) =
        step.iter().partition(|o| o.target.kind == Namespace::KIND);
    let mut written = Vec::new();
    for (dry_run, group) in [
        (true, &namespaces),
        (false, &namespaces),
        (true, &rest),
        (false, &rest),
    ] {
        for object in group {
            let (body, verb) = match (&object.body, &object.uid) {
                (Some(body), _) => {
                    let body = serde_json::to_value(body)
                        .map_err(|e| ApiError::from(Problem::new(Kind::Internal, e.to_string())))?;
                    (WriteBody::Apply(body), "patch")
                }
                (None, Some(uid)) => {
                    let preconditions = Preconditions {
                        uid: Some(uid.clone()),
                        resource_version: object.resource_version.clone(),
                        ..Preconditions::default()
                    };
                    (WriteBody::Delete(preconditions), "delete")
                }
                (None, None) => continue,
            };
            let target = object.target.clone();
            let refused = right(verb, &target);
            let deleting = matches!(body, WriteBody::Delete(_));
            let result = match send(cluster.as_ref(), target, body, dry_run, refused).await {
                Ok(result) => result,
                // The object went away after the plan read it, which is what the delete wants.
                Err(e) if deleting && e.0.kind == Kind::NotFound => continue,
                Err(e) => return Err(e),
            };
            if !dry_run {
                written.push(ObjectWritten {
                    kind: object.target.kind.clone(),
                    name: object.target.name.clone(),
                    namespace: object.target.namespace().map(str::to_owned),
                    uid: result.uid,
                    resource_version: result.resource_version,
                    generation: result.generation,
                });
            }
        }
    }
    let plan = world.plan(cluster.as_ref(), &id, &request).await?.plan;
    Ok(Json(ComponentApplied { plan, written }))
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

/// One object of a step, to apply or to delete.
struct Placed {
    target: ObjectTarget,
    /// The object to apply.
    body: Option<DynamicObject>,
    /// Without `body`: the uid of the object to delete, `None` when it is gone or already being
    /// deleted.
    uid: Option<String>,
    /// Without `body`: the `resourceVersion` of the object to delete.
    resource_version: Option<String>,
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
    catalog: Arc<Catalog>,
    served: Vec<ServedResource>,
    profile: &'static str,
    distribution: Option<&'static str>,
    platform: String,
    /// kube-apiserver's `gitVersion`.
    kube_version: String,
    /// The capabilities a running component gives.
    capabilities: BTreeSet<String>,
    /// The components found by more than CRDs a removal left.
    running: BTreeSet<String>,
    /// How each installed component was installed.
    sources: BTreeMap<String, InstallSource>,
    /// The components Zeabur wrote.
    written: BTreeSet<String>,
    /// The components the distribution packages, with their `HelmChart` in `kube-system`.
    packaged: BTreeMap<String, String>,
    helm_releases: BTreeSet<String>,
    /// The namespace Zeabur runs in.
    zeabur_namespace: String,
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
        let running: BTreeSet<String> = detected
            .iter()
            .flat_map(|c| &c.installations)
            .filter(|i| i.running)
            .map(|i| i.component.clone())
            .collect();
        let capabilities = detected
            .iter()
            .filter(|c| c.installations.iter().any(|i| i.running))
            .map(|c| c.name.clone())
            .collect();
        let sources: BTreeMap<String, InstallSource> = detected
            .iter()
            .flat_map(|c| {
                c.installations
                    .iter()
                    .map(|i| (i.component.clone(), i.source))
            })
            .collect();
        let packaged = detected
            .iter()
            .flat_map(|c| &c.installations)
            .filter_map(|i| Some((i.component.clone(), i.packaged_chart.clone()?)))
            .collect();
        let written = engine::written(&facts).into_values().collect();
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
                 this cluster has none. To install with Helm, Flux or Argo CD, print the \
                 component with `zeaburctl components export <id> --format helm|flux|argocd`.",
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
            catalog: Arc::clone(&state.catalog),
            served,
            profile: zeabur_catalog::profile(&facts),
            distribution: zeabur_catalog::distribution(&facts),
            platform,
            kube_version: facts.version.clone(),
            capabilities,
            running,
            sources,
            written,
            packaged,
            helm_releases: facts.helm_releases.clone(),
            zeabur_namespace: state
                .settings
                .namespace
                .clone()
                .unwrap_or_else(|| "zeabur".to_owned()),
            now: (state.clock)(),
        })
    }

    async fn plan(
        &self,
        cluster: &dyn Cluster,
        id: &str,
        request: &ComponentRequest,
    ) -> Result<Planned, ApiError> {
        match request.action {
            ComponentAction::Install => self.install(cluster, id, &request.values).await,
            ComponentAction::Remove => self.remove(cluster, id).await,
        }
    }

    /// The values earlier installs of `components` wrote: the `INPUTS` annotation of each
    /// `HelmChart` and the keys of each Secret, where Zeabur wrote them for the component.
    async fn kept(&self, cluster: &dyn Cluster, components: &[String]) -> Result<Values, ApiError> {
        let mut kept = Values::new();
        for id in components {
            let Some(file) = self.snapshot.components.get(id) else {
                continue;
            };
            let d = &file.declaration;
            let mut charts = BTreeMap::new();
            let mut secrets = BTreeMap::new();
            for input in &d.inputs {
                let value = if let Some(at) = &input.secret {
                    let key = (at.namespace.clone(), at.name.clone());
                    if !secrets.contains_key(&key) {
                        let found = our_secret(cluster, &at.namespace, &at.name, id).await?;
                        secrets.insert(key.clone(), found);
                    }
                    secrets
                        .get(&key)
                        .and_then(Option::as_ref)
                        .and_then(|values| values.get(&at.key))
                        .and_then(|b| String::from_utf8(b.clone()).ok())
                        .map(InputValue::Text)
                } else {
                    let Some(release) = d.input_release(input).and_then(Release::helm) else {
                        continue;
                    };
                    let name = release.helm_name().to_owned();
                    if !charts.contains_key(&name) {
                        let found = find::<HelmChart>(cluster, Some(CHART_NAMESPACE), &name, id)
                            .await?
                            .and_then(|c| c.metadata.annotations?.remove(INPUTS))
                            .and_then(|text| {
                                serde_json::from_str::<BTreeMap<String, InputValue>>(&text).ok()
                            });
                        charts.insert(name.clone(), found);
                    }
                    charts
                        .get(&name)
                        .and_then(Option::as_ref)
                        .and_then(|values| values.get(&input.name))
                        .cloned()
                };
                if let Some(value) = value {
                    kept.entry(id.clone())
                        .or_default()
                        .insert(input.name.clone(), value);
                }
            }
        }
        Ok(kept)
    }

    fn target(&self) -> Target<'_> {
        Target {
            profile: self.profile,
            distribution: self.distribution,
            platform: &self.platform,
            kube_version: &self.kube_version,
            capabilities: &self.capabilities,
            components: &self.running,
            written: &self.written,
            packaged: &self.packaged,
            zeabur_namespace: &self.zeabur_namespace,
        }
    }

    async fn install(
        &self,
        cluster: &dyn Cluster,
        id: &str,
        given: &Values,
    ) -> Result<Planned, ApiError> {
        if self
            .sources
            .get(id)
            .is_some_and(|s| !matches!(s, InstallSource::Zeabur | InstallSource::Other))
        {
            return Err(ApiError::from(Problem::new(
                Kind::Conflict,
                format!(
                    "{id} is installed, and Zeabur did not install it. Zeabur does not change a \
                     component it did not install."
                ),
            )));
        }
        let install = self.engine_install(cluster, id, given).await?;
        let missing = install.missing();
        let mut steps = Vec::new();
        let mut placed_steps = Vec::new();
        let mut managed = BTreeSet::new();
        let mut warnings: Vec<ComponentWarning> = install
            .warnings
            .into_iter()
            .map(ComponentWarning::from)
            .collect();
        let mut checks: Vec<ComponentCheck> = Vec::new();
        let mut checked = BTreeSet::new();
        let mut held = false;
        for (i, step) in install.steps.iter().enumerate() {
            let steps_ready = steps
                .iter()
                .all(|s: &ComponentStep| s.state == StepState::Ready);
            let checks_ready = checks.iter().all(|c| c.state == StepState::Ready);
            held |= steps_ready && !checks_ready;
            let later = install.steps.get(i + 1..).unwrap_or_default();
            let planned = self
                .install_step(cluster, step, later, steps_ready && checks_ready)
                .await?;
            if planned.foreign.is_some()
                && let Some(values) = config_values(step)
            {
                warnings.push(ComponentWarning::Unchanged {
                    component: step.component.clone(),
                    values,
                });
                continue;
            }
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
            // A component's health checks run once its steps are ready, and the next component's
            // steps wait for them: a chart can need the webhook of an operator installed before
            // it. A target written again after its contributor is not waited for twice.
            let next_other = later.first().is_none_or(|s| s.component != step.component);
            if next_other
                && install.components.contains(&step.component)
                && let Some(file) = self.snapshot.components.get(&step.component)
                && checked.insert(step.component.clone())
            {
                let ready = steps.iter().all(|s| s.state == StepState::Ready);
                checks.extend(self.checks(cluster, file, ready).await?);
            }
        }
        self.refuse_releases_elsewhere(&install.steps, &managed)?;
        let ready = steps.iter().all(|s| s.state == StepState::Ready);
        for component in &install.components {
            let Some(file) = self.snapshot.components.get(component) else {
                continue;
            };
            if checked.insert(component.clone()) {
                checks.extend(self.checks(cluster, file, ready).await?);
            }
        }
        let done = ready && checks.iter().all(|c| c.state == StepState::Ready);
        let inputs = install.inputs.iter().map(input_view).collect();
        Ok(Planned {
            plan: ComponentPlan {
                action: ComponentAction::Install,
                catalog_version: self.snapshot.index.catalog.version.clone(),
                components: install.components,
                steps,
                checks,
                warnings,
                inputs,
                leftovers: Vec::new(),
                done,
            },
            objects: placed_steps,
            zeaburs: !managed.is_empty(),
            missing,
            held,
        })
    }

    /// Refuses a Helm release of the same name that no `HelmChart` of Zeabur's made: removing the
    /// chart would uninstall someone else's release.
    fn refuse_releases_elsewhere(
        &self,
        steps: &[engine::Step],
        managed: &BTreeSet<String>,
    ) -> Result<(), ApiError> {
        let elsewhere = steps.iter().find(|step| {
            let Some(file) = self.snapshot.components.get(&step.component) else {
                return false;
            };
            config_values(step).is_none()
                && !managed.contains(&step.component)
                && file
                    .declaration
                    .releases
                    .iter()
                    .filter_map(Release::helm)
                    .any(|r| {
                        r.name == step.release
                            && self.helm_releases.contains(&format!(
                                "{}/{}",
                                r.namespace,
                                r.helm_name()
                            ))
                    })
        });
        if let Some(step) = elsewhere {
            return Err(ApiError::from(Problem::new(
                Kind::Conflict,
                format!(
                    "{} is already installed, and not by Zeabur. Zeabur does not change a \
                     component it did not install.",
                    step.component
                ),
            )));
        }
        Ok(())
    }

    /// One step of an install: how far it has come, its objects, and whether Zeabur wrote one
    /// of them. An object whose kind the cluster does not serve waits for an earlier step, and
    /// fails once those are ready.
    ///
    /// An object a `later` step of the plan writes again is also current when it holds that
    /// write: a target written before its contributor and again after it is done once it has
    /// the contributor's values.
    async fn install_step(
        &self,
        cluster: &dyn Cluster,
        step: &engine::Step,
        later: &[engine::Step],
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
            let mut found = read(cluster, &target).await?;
            if target.kind == Secret::KIND
                && let Some(found) = found.as_mut()
                && let Some(namespace) = target.namespace()
                && let Some(values) =
                    our_secret(cluster, namespace, &target.name, &step.component).await?
                && let Some(fields) = found.data.as_object_mut()
            {
                let values: BTreeMap<String, ByteString> = values
                    .into_iter()
                    .map(|(k, v)| (k, ByteString(v)))
                    .collect();
                fields.insert(
                    "data".to_owned(),
                    serde_json::to_value(values).unwrap_or_default(),
                );
            }
            if let Some(found) = &found {
                if target.kind == Namespace::KIND {
                    keep_looser_level(&mut body.metadata, &found.metadata);
                } else if ours(&found.metadata, &step.component) {
                    zeaburs = true;
                } else {
                    foreign.get_or_insert_with(|| format!("{} {}", target.kind, target.name));
                }
            }
            let again = later
                .iter()
                .flat_map(|s| &s.objects)
                .rev()
                .find(|d| self.place(d).as_ref() == Some(&target));
            progress.push(match &found {
                Some(found)
                    if !engine::holds(&body, found)
                        && !again.is_some_and(|d| engine::holds(&d.object, found)) =>
                {
                    Progress::Pending
                }
                _ => self.progress(cluster, &target, found.as_ref()).await?,
            });
            writes.push(view(&body, Some(&target), None));
            placed.push(Placed {
                target,
                body: Some(body),
                uid: None,
                resource_version: None,
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

    /// The steps of `file` a removal deletes: no inputs, and no values from other components.
    async fn removed_steps(&self, file: &ComponentFile) -> Result<Vec<engine::Step>, ApiError> {
        let manifests = self.manifests([file]).await?;
        engine::steps_of(
            file,
            &self.snapshot.index.catalog.version,
            self.profile,
            &manifests,
            &[],
            &engine::Contributions::default(),
            &self.zeabur_namespace,
        )
        .map_err(|r| refusal(&r))
    }

    async fn remove(&self, cluster: &dyn Cluster, id: &str) -> Result<Planned, ApiError> {
        let file = self
            .snapshot
            .components
            .get(id)
            .ok_or_else(|| refusal(&Refusal::Unknown(id.to_owned())))?;
        let needing: BTreeSet<String> = self.running.union(&self.written).cloned().collect();
        let needing = engine::needed_by(&self.snapshot, id, &needing);
        if !needing.is_empty() {
            return Err(ApiError::from(Problem::new(
                Kind::Conflict,
                format!("{} needs {id}. Remove it first.", needing.join(", ")),
            )));
        }
        let version = &self.snapshot.index.catalog.version;
        let own = self.removed_steps(file).await?;
        let mut zeaburs = false;
        let mut leftovers = file.declaration.leftovers.clone();
        leftovers.extend(engine::input_secrets(file));
        let (rewrites, emptied) = self.rewrites_without(cluster, id).await?;
        let (mut steps, mut placed_steps): (Vec<_>, Vec<_>) =
            rewrites.into_iter().map(|p| (p.view, p.placed)).unzip();
        for step in own.iter().rev().chain(&emptied) {
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
                    .filter(|o| ours(&o.metadata, &step.component));
                if target.kind == CustomResourceDefinition::KIND {
                    if found.is_some() {
                        leftovers.push(format!(
                            "{}/{}",
                            CustomResourceDefinition::URL_PATH_SEGMENT,
                            target.name
                        ));
                    }
                    continue;
                }
                zeaburs |= found.is_some();
                progress.push(gone(found.as_ref()));
                let mut write = view(&desired.object, Some(&target), None);
                write.yaml = None;
                writes.push(write);
                // An object already being deleted needs no second delete.
                let (uid, resource_version) = found
                    .filter(|o| o.metadata.deletion_timestamp.is_none())
                    .map_or((None, None), |o| {
                        (o.metadata.uid, o.metadata.resource_version)
                    });
                placed.push(Placed {
                    uid,
                    resource_version,
                    target,
                    body: None,
                });
            }
            if writes.is_empty() {
                continue;
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
                inputs: Vec::new(),
                leftovers,
            },
            objects: placed_steps,
            zeaburs,
            missing: None,
            held: false,
        })
    }

    /// The engine's install of `id` with `given` values over the kept ones, and the files of its
    /// manifest releases. A step whose
    /// `skipIfPresent` CRDs the cluster has keeps only its namespaces: the engine writes each
    /// namespace in the first release that uses it, and later releases need it.
    async fn engine_install(
        &self,
        cluster: &dyn Cluster,
        id: &str,
        given: &Values,
    ) -> Result<engine::Install, ApiError> {
        let target = self.target();
        let order = engine::components(&self.snapshot, id, &target).map_err(|r| refusal(&r))?;
        // A contribution writes its target's chart again, with the target's kept values.
        let written: Vec<String> = order
            .iter()
            .cloned()
            .chain(self.targets(&order))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let kept = self.kept(cluster, &written).await?;
        let manifests = self
            .manifests(order.iter().filter_map(|c| self.snapshot.components.get(c)))
            .await?;
        let addresses = self.addresses(cluster, &order, given, &kept).await?;
        let inputs = Inputs {
            given,
            kept: &kept,
            addresses: addresses.as_ref(),
        };
        let mut install = engine::install(&self.snapshot, id, &target, &manifests, inputs)
            .map_err(|r| refusal(&r))?;
        for step in &mut install.steps {
            if skipped(cluster, step).await? {
                step.objects.retain(|d| kind(&d.object) == Namespace::KIND);
            }
        }
        install.steps.retain(|s| !s.objects.is_empty());
        Ok(install)
    }

    /// The cluster's Service addresses, read only when a component of `order` needs Zeabur to
    /// pick a `ClusterIP` that is neither given nor kept.
    async fn addresses(
        &self,
        cluster: &dyn Cluster,
        order: &[String],
        given: &Values,
        kept: &Values,
    ) -> Result<Option<engine::ServiceAddresses>, ApiError> {
        let has = |values: &Values, id: &str, name: &str| {
            values
                .get(id)
                .and_then(|m| m.get(name))
                .is_some_and(|v| !matches!(v, InputValue::Text(t) if t.is_empty()))
        };
        let needed = order.iter().any(|id| {
            self.snapshot.components.get(id).is_some_and(|file| {
                file.declaration.inputs.iter().any(|i| {
                    i.generate == Some(Generate::ClusterIp)
                        && !has(given, id, &i.name)
                        && !has(kept, id, &i.name)
                })
            })
        });
        if !needed {
            return Ok(None);
        }
        let problem = |e: zeabur_kube::ClusterError| {
            ApiError::from(e.into_problem("the cluster's Service addresses"))
        };
        let cidrs = cluster
            .list::<ServiceCIDR>(None)
            .await
            .map_err(problem)?
            .into_iter()
            .flat_map(|c| c.spec.and_then(|s| s.cidrs).unwrap_or_default())
            .collect();
        let taken = cluster
            .list::<IPAddress>(None)
            .await
            .map_err(problem)?
            .into_iter()
            .filter_map(|a| a.metadata.name?.parse().ok())
            .collect();
        Ok(Some(engine::ServiceAddresses { cidrs, taken }))
    }

    /// The files of the manifest releases of `files`.
    async fn manifests(
        &self,
        files: impl IntoIterator<Item = &ComponentFile>,
    ) -> Result<Manifests, ApiError> {
        let mut downloads = Vec::new();
        for file in files {
            downloads.extend(engine::downloads(file).map_err(|r| refusal(&r))?);
        }
        self.catalog.manifests(downloads).await
    }

    /// The components that `ids` add values to.
    fn targets(&self, ids: &[String]) -> Vec<String> {
        ids.iter()
            .filter_map(|id| self.snapshot.components.get(id))
            .flat_map(|f| &f.declaration.contributes)
            .map(|c| c.target.component.clone())
            .collect()
    }

    /// The steps that write the targets of `id`'s contributions again without them, and the ones
    /// of those that leave a config empty. Zeabur leaves a config it did not write as it is.
    ///
    /// A removal runs them first, so helm-controller installs each target without the values
    /// before the objects it used go, and deletes an empty config last.
    async fn rewrites_without(
        &self,
        cluster: &dyn Cluster,
        id: &str,
    ) -> Result<(Vec<PlannedStep>, Vec<engine::Step>), ApiError> {
        let kept = self.kept(cluster, &self.targets(&[id.to_owned()])).await?;
        let rewrites =
            engine::without(&self.snapshot, id, &self.target(), &kept).map_err(|r| refusal(&r))?;
        let mut planned_steps: Vec<PlannedStep> = Vec::new();
        let mut emptied = Vec::new();
        for step in rewrites {
            if config_values(&step).is_some_and(|v| v.is_empty()) {
                let found = match step.objects.first().and_then(|d| self.place(d)) {
                    Some(target) => read(cluster, &target).await?,
                    None => None,
                };
                // A config already deleted has no values to clear.
                if found.is_none() {
                    continue;
                }
            }
            let earlier_ready = planned_steps
                .iter()
                .all(|s| s.view.state == StepState::Ready);
            let planned = self
                .install_step(cluster, &step, &[], earlier_ready)
                .await?;
            if planned.foreign.is_some() {
                continue;
            }
            if config_values(&step).is_some_and(|v| v.is_empty()) {
                emptied.push(step);
            }
            planned_steps.push(planned);
        }
        Ok((planned_steps, emptied))
    }

    /// Refuses a removal while objects of the component's custom resources are in use, or volumes
    /// use its CSI drivers [installers]. An object is in use when Zeabur wrote it for another
    /// component, or when nobody labelled it and it is outside the component's namespaces.
    ///
    /// [installers]: https://github.com/zeabur-oss/catalog/blob/main/docs/reference/installers.mdx
    async fn still_used(
        &self,
        cluster: &dyn Cluster,
        file: &ComponentFile,
    ) -> Result<(), ApiError> {
        let d = &file.declaration;
        let own: BTreeSet<&str> = d.releases.iter().filter_map(Release::namespace).collect();
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
                let written_for = object
                    .metadata
                    .labels
                    .as_ref()
                    .and_then(|l| l.get(COMPONENT));
                let user = written_for.map_or_else(
                    || !own.contains(namespace.as_str()),
                    |component| *component != file.id,
                );
                if user {
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
        let mut problem = Problem::new(
            Kind::Conflict,
            format!("{list} still use {}. Delete them first.", file.id),
        );
        problem.summary = Message::new("problem.summary.inUse")
            .with("users", users.join(", "))
            .with("more", more.to_string());
        problem.fix_for_occasion(
            "Delete them first, then remove it again.",
            Message::new("problem.conflict.fix.inUse"),
        );
        Err(ApiError::from(problem))
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
        if target.kind == HelmChartConfig::KIND {
            let resource = ResourceRef::of::<HelmChart>();
            let chart = cluster
                .find_object(&resource, Some(CHART_NAMESPACE), &target.name)
                .await
                .map_err(|e| ApiError::from(e.into_problem(&format!("HelmChart {}", target.name))))?
                .and_then(|o| o.try_parse::<HelmChart>().ok())
                .unwrap_or_default();
            let job = self.job(cluster, &chart).await?;
            return Ok(engine::config_progress(
                &found.metadata,
                &chart,
                job.as_ref(),
            ));
        }
        if target.kind != HelmChart::KIND {
            let (group, _) = target
                .api_version
                .split_once('/')
                .unwrap_or(("", &target.api_version));
            return Ok(engine::object_progress(
                group,
                &target.kind,
                Some(found),
                self.now,
            ));
        }
        let chart: HelmChart = found.clone().try_parse().unwrap_or_default();
        let job = self.job(cluster, &chart).await?;
        Ok(engine::chart_progress(&chart, job.as_ref()))
    }

    /// The Job a `HelmChart`'s status names.
    async fn job(&self, cluster: &dyn Cluster, chart: &HelmChart) -> Result<Option<Job>, ApiError> {
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
        Ok(job)
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
            let resource = engine::health_resource(check.resource.kind, &self.served);
            let progress = match resource {
                Some(resource) if ready => {
                    let objects = self.check_objects(cluster, file, check, resource).await?;
                    engine::check_progress(check, resource, &objects, now)
                }
                None if ready => Progress::Waiting,
                Some(_) | None => Progress::Pending,
            };
            let (state, message) = match progress {
                Progress::Ready => (StepState::Ready, None),
                Progress::Failed(why) => (StepState::Failed, Some(why)),
                Progress::Waiting => (StepState::Waiting, None),
                Progress::Pending => (StepState::Pending, None),
            };
            checks.push(ComponentCheck {
                component: file.id.clone(),
                name: check.name.clone(),
                state,
                advice: check.advice.clone(),
                message,
                timeout_seconds: engine::timeout_seconds(&check.timeout),
            });
        }
        Ok(checks)
    }

    async fn check_objects(
        &self,
        cluster: &dyn Cluster,
        file: &ComponentFile,
        check: &Health,
        served: &ServedResource,
    ) -> Result<Vec<DynamicObject>, ApiError> {
        let r = &check.resource;
        let resource = ResourceRef::new(
            &served.group,
            &served.version,
            &served.resource,
            &served.kind,
        );
        let namespace = r.namespace.clone().or_else(|| {
            file.declaration
                .releases
                .first()
                .and_then(|x| x.namespace().map(str::to_owned))
        });
        let namespace = served.namespaced.then_some(namespace).flatten();
        let what = format!("{} objects of {}", served.kind, file.id);
        if let Some(name) = &r.name {
            let found = cluster
                .find_object(&resource, namespace.as_deref(), name)
                .await
                .map_err(|e| ApiError::from(e.into_problem(&what)))?;
            return Ok(found.into_iter().collect());
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
            .collect())
    }
}

/// How far deleting `found` has come.
const fn gone(found: Option<&DynamicObject>) -> Progress {
    match found {
        None => Progress::Ready,
        Some(o) if o.metadata.deletion_timestamp.is_some() => Progress::Waiting,
        Some(_) => Progress::Pending,
    }
}

/// The values of a step's `HelmChartConfig`; `None` when it has none or the step writes
/// something else.
fn config_values(step: &engine::Step) -> Option<String> {
    step.objects.iter().find_map(|d| {
        let config: HelmChartConfig = d
            .object
            .clone()
            .try_parse()
            .ok()
            .filter(|_| kind(&d.object) == HelmChartConfig::KIND)?;
        config.spec.values_content
    })
}

/// A step's state from its objects': pending while one still needs a write, so changed values
/// replace a failed release; else failed, else waiting, else ready.
fn combine(progress: &[Progress]) -> (StepState, Option<String>) {
    if progress.contains(&Progress::Pending) {
        return (StepState::Pending, None);
    }
    if let Some(message) = progress.iter().find_map(|p| match p {
        Progress::Failed(m) => Some(m.clone()),
        Progress::Pending | Progress::Waiting | Progress::Ready => None,
    }) {
        return (StepState::Failed, Some(message));
    }
    let state = if progress.contains(&Progress::Waiting) {
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

/// Whether every CRD of the step's `skipIfPresent` exists and Zeabur did not write it for the
/// component: the cluster already has what the release installs.
async fn skipped(cluster: &dyn Cluster, step: &engine::Step) -> Result<bool, ApiError> {
    if step.skip_if_present.is_empty() {
        return Ok(false);
    }
    let resource = ResourceRef::of::<CustomResourceDefinition>();
    for name in &step.skip_if_present {
        let found = cluster
            .find_object(&resource, None, name)
            .await
            .map_err(|e| {
                ApiError::from(e.into_problem(&format!("CustomResourceDefinition {name}")))
            })?;
        if found.is_none_or(|o| ours(&o.metadata, &step.component)) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Whether Zeabur wrote the object for `component`: its labels say so, or it has no component
/// label and Zeabur applied it. A controller can replace the labels of what it manages, as the
/// System Upgrade Controller does with its CRD when it starts.
fn ours(metadata: &ObjectMeta, component: &str) -> bool {
    let labels = metadata.labels.as_ref();
    let applied = || {
        metadata.managed_fields.iter().flatten().any(|m| {
            m.manager.as_deref() == Some(FIELD_MANAGER) && m.operation.as_deref() == Some("Apply")
        })
    };
    labels
        .and_then(|l| l.get(COMPONENT))
        .map_or_else(applied, |c| {
            c == component
                && labels
                    .and_then(|l| l.get(MANAGED_BY))
                    .is_some_and(|m| m == MANAGED_BY_ZEABUR)
        })
}

fn kind(object: &DynamicObject) -> &str {
    object.types.as_ref().map_or("", |t| t.kind.as_str())
}

/// An input as the plan shows it: a Secret's value stays on the server.
fn input_view(r: &engine::Resolved) -> ComponentInput {
    ComponentInput {
        component: r.component.clone(),
        name: r.input.name.clone(),
        description: r.input.description.trim().to_owned(),
        kind: InputKind::of(&r.input),
        required: r.input.required,
        default: r.input.default.clone(),
        generated: matches!(r.input.generate, Some(Generate::Random { .. })),
        state: r.state,
        value: r.input.secret.is_none().then(|| r.value.clone()).flatten(),
    }
}

/// What a step writes, as the review shows it: a Secret without its values.
fn view(
    object: &DynamicObject,
    target: Option<&ObjectTarget>,
    namespace: Option<&str>,
) -> ObjectWrite {
    let mut shown = object.clone();
    if kind(object) == Secret::KIND {
        shown.data.as_object_mut().map(|o| o.remove("data"));
    }
    let object = &shown;
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

/// An object Zeabur wrote for `component`, typed; `None` when there is none.
async fn find<
    K: zeabur_kube::k8s_openapi::Resource
        + zeabur_kube::k8s_openapi::Metadata<Ty = ObjectMeta>
        + serde::de::DeserializeOwned,
>(
    cluster: &dyn Cluster,
    namespace: Option<&str>,
    name: &str,
    component: &str,
) -> Result<Option<K>, ApiError> {
    let found = cluster
        .find_object(&ResourceRef::of::<K>(), namespace, name)
        .await
        .map_err(|e| ApiError::from(e.into_problem(&format!("{} {name}", K::KIND))))?;
    Ok(found
        .filter(|o| ours(&o.metadata, component))
        .and_then(|o| o.try_parse().ok()))
}

/// The values of the Secret Zeabur wrote for `component`, read only to keep them: Zeabur reads
/// Secrets without their values otherwise.
async fn our_secret(
    cluster: &dyn Cluster,
    namespace: &str,
    name: &str,
    component: &str,
) -> Result<Option<BTreeMap<String, Vec<u8>>>, ApiError> {
    if find::<Secret>(cluster, Some(namespace), name, component)
        .await?
        .is_none()
    {
        return Ok(None);
    }
    cluster
        .reveal_secret(namespace, name)
        .await
        .map(Some)
        .map_err(|e| ApiError::from(e.into_problem(&format!("Secret {name}"))))
}

/// A refusal of the engine, with what to do about it in place of the kind's "correct the
/// fields".
fn refusal(refusal: &Refusal) -> ApiError {
    let mut problem = Problem::new(Kind::Invalid, refusal.to_string());
    let (fix, key) = match refusal {
        Refusal::Unsupported { .. } => (
            "Install it with Helm, Flux or Argo CD. `zeaburctl components export` prints what to apply."
                .to_owned(),
            Message::new("problem.invalid.fix.componentUnsupported"),
        ),
        Refusal::Missing { input, .. } | Refusal::WrongValue { input, .. } => (
            format!("Fill in {input}, then review the plan again."),
            Message::new("problem.invalid.fix.componentValue").with("input", input),
        ),
        Refusal::Unknown(_) | Refusal::NotHere { .. } | Refusal::NoProvider(_) => (
            "Choose a component the catalog offers on this cluster.".to_owned(),
            Message::new("problem.invalid.fix.componentElsewhere"),
        ),
        Refusal::Broken { .. } => {
            return ApiError::from(Problem::new(Kind::Internal, refusal.to_string()));
        }
    };
    problem.fix_for_occasion(fix, key);
    ApiError::from(problem)
}
