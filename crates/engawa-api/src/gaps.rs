//! What a project and the cluster are missing ([`engawa_gaps`]), each finding with the guidance
//! it rests on and its fix.
//!
//! # Shape
//!
//! Gaps are their own read, not part of the project document: the rules need two cluster-wide
//! facts (whether Gateway API serves a Gateway, and which `IngressClasses` belong to a retired
//! controller) that change rarely and that a polled document should not pay for every few
//! seconds. Each gap carries its fix: a plan template id the web interface turns into a plan, an
//! install from the catalogue, or a YAML `snippet` to copy when Engawa cannot write the fix.
//!
//! The cluster-wide facts are read as the person. Someone who may not list `IngressClasses` or
//! Gateways gets the rules that do not need them, and the refused reads are named in
//! `unreadable`; a missing Gateway API (not served) is not a refusal.
//!
//! Diagnoses (what is going wrong with a service right now) are not gaps: they ride on each
//! service in the project document, see [`crate::projects`].

use axum::Json;
use axum::extract::Path;
use engawa_core::problem::{MEDIA_TYPE as PROBLEM, Problem};
use engawa_gaps::{
    BUILTIN, ClusterTraits, GatewayRef, IngressClassFact, Input, Rule, cluster_gaps, evaluate,
    retired_classes, snippet,
};
use engawa_graph::manifest;
use engawa_kube::{Cluster, ClusterExt as _, ResourceRef};
use engawa_model::{Citation, Gap};
use engawa_modules::Finding;
use serde::Serialize;
use utoipa::ToSchema;

use crate::error::{ApiError, project_read, read_problem};
use crate::handlers::Person;
use crate::views::UnreadableView;

/// One gap and, when its fix is a YAML fragment to copy, that fragment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GapView {
    /// The finding.
    #[serde(flatten)]
    pub gap: Gap,
    /// YAML that fixes it, to copy into the object's manifest; absent when the fix is an install
    /// or a plan Engawa writes itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

/// What a project or the cluster is missing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GapList {
    /// The findings: warnings first, then by rule and subject.
    pub items: Vec<GapView>,
    /// Reads the person was refused; rules that need them did not run on their facts.
    pub unreadable: Vec<UnreadableView>,
}

/// The cluster-wide facts the rules need, and the refused reads.
struct Facts {
    classes: Vec<IngressClassFact>,
    gateways: Vec<GatewayRef>,
    unreadable: Vec<UnreadableView>,
}

fn ingress_classes() -> ResourceRef {
    ResourceRef::new("networking.k8s.io", "v1", "ingressclasses", "IngressClass")
}

async fn facts(cluster: &dyn Cluster) -> Result<Facts, ApiError> {
    let mut unreadable = Vec::new();
    let resource = ingress_classes();
    let classes = match cluster.list_objects(&resource, None).await {
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
    };
    let gateways = match cluster.gateways().await {
        Ok(Some(gateways)) => gateways,
        // Refused: the rules run as if no Gateway existed; the plan names the refusal when a
        // route is written.
        Ok(None) => Vec::new(),
        Err(other) => return Err(ApiError::from(other.into_problem("Gateways"))),
    };
    Ok(Facts {
        classes,
        gateways,
        unreadable,
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
    let projection = engawa_graph::project(&snapshot);
    let found = module_findings(
        cluster,
        &project,
        &snapshot,
        &projection,
        &http_routes,
        &mut unreadable,
    )
    .await?;
    let traits = ClusterTraits {
        gateway_api: !facts.gateways.is_empty(),
        retired_ingress_classes: retired_classes(&facts.classes),
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
            cluster: &traits,
        },
    );
    let items = gaps
        .into_iter()
        .map(|gap| GapView {
            snippet: found.iter().find(|f| f.gap == gap).map_or_else(
                || snippet(&gap, &projection, &facts.gateways),
                |f| f.object.as_ref().and_then(|o| manifest::to_yaml(o).ok()),
            ),
            gap,
        })
        .collect();
    unreadable.extend(facts.unreadable);
    Ok(Json(GapList { items, unreadable }))
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
    engawa_modules::network_policy::BLOCKED_CONNECTION,
    engawa_modules::network_policy::NO_NETWORK_POLICY,
    engawa_modules::backups::VOLUME_WITHOUT_BACKUP,
    engawa_modules::backups::DATABASE_WITHOUT_BACKUP,
];

/// The gaps the modules find in `project`: NetworkPolicy blocking a connection the
/// project's variables imply, services no NetworkPolicy protects, and claims or databases no
/// backup schedule covers. Their reads are the person's; a refused read is named in `unreadable`
/// and its rule has nothing to say. The nodes, read to learn whether NetworkPolicy is enforced,
/// are the exception: refused, enforcement is unknown and the finding says so itself.
async fn module_findings(
    cluster: &dyn Cluster,
    project: &str,
    snapshot: &engawa_graph::Snapshot,
    projection: &engawa_model::Project,
    http_routes: &[serde_json::Value],
    unreadable: &mut Vec<UnreadableView>,
) -> Result<Vec<Finding>, ApiError> {
    let mut found = Vec::new();
    if let Some(facts) =
        crate::projects::network_policy::read(cluster, snapshot, projection, unreadable).await?
    {
        use engawa_modules::network_policy::{self, Exposed};
        let reachability = network_policy::reachability(facts.input());
        found.extend(network_policy::gaps(
            project,
            &reachability,
            &facts.workloads,
        ));
        let (routed, published) = crate::projects::network_policy::exposed(projection, http_routes);
        let enforcement = crate::projects::network_policy::enforcement(cluster).await;
        found.extend(network_policy::unprotected(
            facts.input(),
            Exposed {
                routed: &routed,
                published: &published,
            },
            enforcement,
        ));
    }
    {
        let backups = crate::projects::backups::read(cluster, project).await?;
        let installed = |module: &str| {
            backups.providers.iter().any(|p| {
                p.module == module && p.state == crate::projects::backups::ProviderState::Installed
            })
        };
        found.extend(engawa_modules::backups::gaps(
            project,
            &backups.coverage,
            engawa_modules::backups::Installed {
                k8up: installed("k8up"),
                velero: installed("velero"),
            },
        ));
        unreadable.extend(backups.unreadable);
    }
    Ok(found)
}

/// What the cluster itself is missing, apart from any project: an IngressClass served by a
/// retired controller.
#[utoipa::path(get, path = "/api/v1/cluster/gaps", tag = "cluster",
    responses((status = 200, body = GapList), (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn cluster(Person(cluster): Person) -> Result<Json<GapList>, ApiError> {
    let facts = facts(cluster.as_ref()).await?;
    let items = cluster_gaps(&facts.classes)
        .into_iter()
        .map(|gap| GapView { gap, snippet: None })
        .collect();
    Ok(Json(GapList {
        items,
        unreadable: facts.unreadable,
    }))
}
