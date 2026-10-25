//! What a project and the cluster are missing ([`zeabur_gaps`]), each finding with the guidance
//! it rests on and its fix.
//!
//! # Shape
//!
//! Gaps are their own read, not part of the project document: the rules need two cluster-wide
//! facts (whether Gateway API serves a Gateway, and which `IngressClasses` belong to a retired
//! controller) that change rarely and that a polled document should not pay for every few
//! seconds. Each gap carries its fix: a plan template id the web interface turns into a plan (or
//! into the form that asks for what the plan needs), or an install from the catalogue.
//!
//! The cluster-wide facts are read as the person. Someone who may not list `IngressClasses` or
//! Gateways gets the rules that do not need them, and the refused reads are named in
//! `unreadable`; a missing Gateway API (not served) is not a refusal.
//!
//! Diagnoses (what is going wrong with a service right now) are not gaps: they ride on each
//! service in the project document, see [`crate::projects`].

use axum::Json;
use axum::extract::Path;
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::isolation::{PLATFORM_LABEL, PROJECT_POLICY};
use zeabur_core::names::LABEL_PROJECT;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use zeabur_gaps::{
    BUILTIN, ClusterTraits, GatewayRef, IngressClassFact, Input, NamespaceFact, Rule, cluster_gaps,
    evaluate, retired_classes,
};
use zeabur_kube::{Cluster, ClusterError, ClusterExt as _, ResourceRef};
use zeabur_model::{Citation, Gap, HttpRoute};
use zeabur_modules::Finding;

use crate::error::{ApiError, project_read, read_problem};
use crate::handlers::Person;
use crate::views::UnreadableView;

/// What a project or the cluster is missing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GapList {
    /// The findings: warnings first, then by rule and subject.
    pub items: Vec<Gap>,
    /// Reads the person was refused; rules that need them did not run on their facts.
    pub unreadable: Vec<UnreadableView>,
}

/// The cluster-wide facts the rules need, and the refused reads.
struct Facts {
    classes: Vec<IngressClassFact>,
    /// `None` when the person may not list them.
    gateways: Option<Vec<GatewayRef>>,
    unreadable: Vec<UnreadableView>,
}

fn ingress_classes() -> ResourceRef {
    ResourceRef::new("networking.k8s.io", "v1", "ingressclasses", "IngressClass")
}

async fn facts(cluster: &dyn Cluster) -> Result<Facts, ApiError> {
    let mut unreadable = Vec::new();
    let classes = ingress_class_facts(cluster, &mut unreadable).await?;
    // Refused (`None`): the rules run as if no Gateway were served and none says one is
    // missing; the plan names the refusal when a route is written.
    let gateways = cluster
        .gateways()
        .await
        .map_err(|other| ApiError::from(other.into_problem("Gateways")))?;
    Ok(Facts {
        classes,
        gateways,
        unreadable,
    })
}

/// The cluster's `IngressClasses`; a refusal is added to `unreadable` and gives none.
pub(crate) async fn ingress_class_facts(
    cluster: &dyn Cluster,
    unreadable: &mut Vec<UnreadableView>,
) -> Result<Vec<IngressClassFact>, ApiError> {
    let resource = ingress_classes();
    Ok(match cluster.list_objects(&resource, None).await {
        Ok(objects) => objects
            .unwrap_or_default()
            .into_iter()
            .filter_map(|object| {
                Some(IngressClassFact {
                    name: object.metadata.name.clone()?,
                    controller: object
                        .data
                        .get("spec")?
                        .get("controller")?
                        .as_str()?
                        .to_owned(),
                })
            })
            .collect(),
        Err(error) => {
            let refusal = error
                .into_unreadable(&resource.key())
                .map_err(|other| ApiError::from(other.into_problem("IngressClasses")))?;
            unreadable.push(refusal.into());
            Vec::new()
        }
    })
}

/// What a project is missing, by the built-in rules, each with its citations and fix.
#[utoipa::path(get, path = "/api/v1/projects/{project}/gaps", tag = "projects",
    params(("project" = String, Path, description = "The project's namespace")),
    responses((status = 200, body = GapList), (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn project(
    Person(cluster): Person,
    Path(project): Path<String>,
) -> Result<Json<GapList>, ApiError> {
    let cluster = cluster.as_ref();
    let (objects, facts) =
        futures_util::future::join(cluster.namespace_objects(&project), facts(cluster)).await;
    let objects = objects
        .map_err(|e| read_problem(e, &format!("project {project}"), project_read(&project)))?;
    let facts = facts?;
    let http_routes = objects.http_routes.clone();
    let (snapshot, mut unreadable) = crate::projects::snapshot(objects);
    // A refused list leaves `snapshot.services` empty; it says nothing about which exist.
    let services_readable = !unreadable.iter().any(|u| u.resource == "services");
    let projection = zeabur_graph::project(&snapshot);
    let found = module_findings(
        cluster,
        &project,
        &snapshot,
        &projection,
        &http_routes,
        &mut unreadable,
    )
    .await?;
    let routes: Vec<HttpRoute> = http_routes
        .iter()
        .filter_map(|route| serde_json::from_value(route.clone()).ok())
        .collect();
    let traits = ClusterTraits {
        retired_ingress_classes: retired_classes(&facts.classes),
        gateways: facts.gateways,
    };
    let modules: Vec<ModuleRule> = ModuleRule::group(&found);
    let mut rules: Vec<&dyn Rule> = BUILTIN.to_vec();
    for rule in &modules {
        rules.push(rule);
    }
    let gaps = evaluate(
        &rules,
        &Input {
            project: &projection,
            snapshot: &snapshot,
            http_routes: &routes,
            services_readable,
            cluster: &traits,
        },
    );
    unreadable.extend(facts.unreadable);
    Ok(Json(GapList {
        items: gaps,
        unreadable,
    }))
}

/// A module's findings, wrapped as a rule so they sort and show with the built-in ones. The
/// module computed them from its own reads; the rule only hands them over.
struct ModuleRule {
    id: &'static str,
    citations: Vec<Citation>,
    gaps: Vec<Gap>,
}

impl ModuleRule {
    /// One rule per rule id among `found`.
    fn group(found: &[Finding]) -> Vec<Self> {
        let mut rules: Vec<Self> = Vec::new();
        for finding in found {
            let Some(id) = MODULE_RULES.iter().find(|id| **id == finding.gap.rule) else {
                continue;
            };
            match rules.iter_mut().find(|r| r.id == *id) {
                Some(rule) => rule.gaps.push(finding.gap.clone()),
                None => rules.push(Self {
                    id,
                    citations: finding.gap.citations.clone(),
                    gaps: vec![finding.gap.clone()],
                }),
            }
        }
        rules
    }
}

impl Rule for ModuleRule {
    fn id(&self) -> &'static str {
        self.id
    }

    fn citations(&self) -> Vec<Citation> {
        self.citations.clone()
    }

    fn check(&self, _input: &Input<'_>) -> Vec<Gap> {
        self.gaps.clone()
    }
}

/// The rule ids modules may report.
const MODULE_RULES: &[&str] = &[
    zeabur_modules::network_policy::BLOCKED_CONNECTION,
    zeabur_modules::network_policy::NO_NETWORK_POLICY,
    zeabur_modules::backups::VOLUME_WITHOUT_BACKUP,
    zeabur_modules::backups::DATABASE_WITHOUT_BACKUP,
];

/// The gaps the modules find in `project`: NetworkPolicy blocking a connection the
/// project's variables imply, services no NetworkPolicy protects, and claims or databases no
/// backup schedule covers. Their reads are the person's; a refused read is named in `unreadable`
/// and its rule has nothing to say.
async fn module_findings(
    cluster: &dyn Cluster,
    project: &str,
    snapshot: &zeabur_graph::Snapshot,
    projection: &zeabur_model::Project,
    http_routes: &[serde_json::Value],
    unreadable: &mut Vec<UnreadableView>,
) -> Result<Vec<Finding>, ApiError> {
    let mut found = Vec::new();
    if let Some(facts) =
        crate::projects::network_policy::read(cluster, snapshot, projection, unreadable).await?
    {
        use zeabur_modules::network_policy::{self, Exposed};
        let reachability = network_policy::reachability(facts.input());
        found.extend(network_policy::gaps(
            project,
            &reachability,
            &facts.workloads,
        ));
        let (routed, published) = crate::projects::network_policy::exposed(projection, http_routes);
        found.extend(network_policy::unprotected(
            facts.input(),
            Exposed {
                routed: &routed,
                published: &published,
            },
        ));
    }
    {
        let backups = crate::projects::backups::read(cluster, project).await?;
        let installed = |module: &str| {
            backups.providers.iter().any(|p| {
                p.module == module && p.state == crate::projects::backups::ProviderState::Installed
            })
        };
        found.extend(zeabur_modules::backups::gaps(
            &backups.coverage,
            zeabur_modules::backups::Installed {
                k8up: installed("k8up"),
                velero: installed("velero"),
            },
        ));
        unreadable.extend(backups.unreadable);
    }
    Ok(found)
}

/// The cluster's namespaces and which hold the project policy. A refused read is added to
/// `unreadable` and gives none.
async fn namespace_facts(
    cluster: &dyn Cluster,
    unreadable: &mut Vec<UnreadableView>,
) -> Result<Vec<NamespaceFact>, ApiError> {
    let namespaces = ResourceRef::new("", "v1", "namespaces", "Namespace");
    let policies = ResourceRef::new(
        "networking.k8s.io",
        "v1",
        "networkpolicies",
        "NetworkPolicy",
    );
    let (found, isolated) = futures_util::future::join(
        cluster.list_metadata(&namespaces, None),
        cluster.list_metadata(&policies, None),
    )
    .await;
    let mut read =
        |resource: &ResourceRef, answer: Result<Option<Vec<_>>, ClusterError>| match answer {
            Ok(list) => Ok(Some(list.unwrap_or_default())),
            Err(error) => {
                let refusal = error
                    .into_unreadable(&resource.key())
                    .map_err(|other| ApiError::from(other.into_problem(&resource.kind)))?;
                unreadable.push(refusal.into());
                Ok::<_, ApiError>(None)
            }
        };
    let (Some(found), Some(isolated)) = (read(&namespaces, found)?, read(&policies, isolated)?)
    else {
        return Ok(Vec::new());
    };
    let isolated: std::collections::BTreeSet<&str> = isolated
        .iter()
        .filter(|p| p.name.as_deref() == Some(PROJECT_POLICY))
        .filter_map(|p| p.namespace.as_deref())
        .collect();
    Ok(found
        .iter()
        .filter_map(|meta| {
            let name = meta.name.clone()?;
            let labels = meta.labels.as_ref();
            Some(NamespaceFact {
                project: labels.is_some_and(|l| l.contains_key(LABEL_PROJECT)),
                platform: labels
                    .is_some_and(|l| l.get(PLATFORM_LABEL).map(String::as_str) == Some("true")),
                project_policy: isolated.contains(name.as_str()),
                name,
            })
        })
        .collect())
}

/// What the cluster itself is missing, apart from any project: an IngressClass served by a
/// retired controller, and a project namespace without its NetworkPolicy.
#[utoipa::path(get, path = "/api/v1/cluster/gaps", tag = "cluster",
    responses((status = 200, body = GapList), (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn cluster(Person(cluster): Person) -> Result<Json<GapList>, ApiError> {
    let cluster = cluster.as_ref();
    let mut facts = facts(cluster).await?;
    let namespaces = namespace_facts(cluster, &mut facts.unreadable).await?;
    Ok(Json(GapList {
        items: cluster_gaps(&facts.classes, &namespaces),
        unreadable: facts.unreadable,
    }))
}
