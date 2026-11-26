//! Which features a successful write used, for the experience program
//! (docs/reference/telemetry.mdx). The server reads only the kinds of what a plan creates, never
//! a name, and hands each feature to [`Used`].

use std::sync::Arc;

use serde_json::Value;
use zeabur_kube::Cluster;
use zeabur_model::ImagePolicy;
use zeabur_modules::{
    Read, cloudnative_pg, dragonfly, k8up, mariadb_operator, moco, percona_mongodb, seaweedfs,
};
use zeabur_plan::{BuildSource, Change, Update, created_object};
use zeabur_telemetry::{
    AccessLimit, DatabaseKind, DomainKind, Feature, RestoreKind, Source, UpdatePolicy,
};

use crate::handlers::AppState;
use crate::projects::templates::PAGE_SERVER_TEMPLATE;
use crate::settings::{Settings, domains};

/// What receives a feature after the call that used it succeeded; it decides whether to send.
pub type Used = Arc<dyn Fn(Feature) + Send + Sync>;

/// Hands `feature` to the program; nothing when the program is off.
pub(crate) fn tell(state: &AppState, feature: Feature) {
    if let Some(used) = &state.used {
        used(feature);
    }
}

const DATABASES: [(&Read, DatabaseKind); 6] = [
    (&cloudnative_pg::CLUSTERS, DatabaseKind::Postgresql),
    (&percona_mongodb::CLUSTERS, DatabaseKind::Mongodb),
    (&moco::CLUSTERS, DatabaseKind::Mysql),
    (&mariadb_operator::SERVERS, DatabaseKind::Mariadb),
    (&dragonfly::STORES, DatabaseKind::Dragonfly),
    (&seaweedfs::SEAWEEDS, DatabaseKind::ObjectStorage),
];

/// What the changes of an applied plan used, and the host names they add.
#[derive(Debug, Default)]
struct Found {
    features: Vec<Feature>,
    hosts: Vec<String>,
}

impl Found {
    fn feature(&mut self, feature: Feature) {
        if !self.features.contains(&feature) {
            self.features.push(feature);
        }
    }
}

fn read(changes: &[Change]) -> Found {
    let mut found = Found::default();
    let mut workloads = 0;
    let mut builds = 0;
    for change in changes {
        if let Change::Template(template) = change {
            if let Some(id) = zeabur_catalog::template_id(&template.template) {
                found.feature(Feature::ServiceCreated(Source::Template(id)));
            }
            found.hosts.extend(template.domain.clone());
        } else if let Change::PageServer(_) = change {
            if let Some(id) = zeabur_catalog::template_id(PAGE_SERVER_TEMPLATE) {
                found.feature(Feature::ServiceCreated(Source::Template(id)));
            }
        } else if let Change::CreateBuiltService(service) = change {
            found.feature(Feature::ServiceCreated(match service.build.source {
                BuildSource::Upload { .. } => Source::Upload,
                BuildSource::Git { .. } => Source::Git,
            }));
            found.hosts.extend(service.host.clone());
        } else if let Change::SetBuild(set) = change {
            found.feature(Feature::BuildSourceSet(match set.build.source {
                BuildSource::Upload { .. } => Source::Upload,
                BuildSource::Git { .. } => Source::Git,
            }));
        } else if let Change::CreateImageService(image) = change {
            found.feature(Feature::ServiceCreated(Source::Image));
            found.hosts.extend(image.host.clone());
        } else if let Change::AddDomain(add) = change {
            found.hosts.push(add.hostname.clone());
        } else if let Some(feature) = turned_on(change) {
            found.feature(feature);
        } else if let Some(Ok(object)) = created_object(change) {
            created(&object, &mut found, &mut workloads, &mut builds);
        }
    }
    // Each built service has one Build and one workload; a workload more is an image service.
    if workloads > builds {
        found.feature(Feature::ServiceCreated(Source::Image));
    }
    found
}

/// The feature a change that turns something on or acts on a workload uses.
fn turned_on(change: &Change) -> Option<Feature> {
    if let Change::PublicPort(port) = change {
        (!port.remove).then_some(Feature::PublicPortOpened)
    } else if let Change::IpAllow(allow) = change {
        allow
            .enabled
            .then_some(Feature::AccessLimited(AccessLimit::IpAllow))
    } else if let Change::BasicAuth(auth) = change {
        auth.user
            .is_some()
            .then_some(Feature::AccessLimited(AccessLimit::BasicAuth))
    } else if let Change::Maintenance(page) = change {
        page.enabled
            .then_some(Feature::AccessLimited(AccessLimit::Maintenance))
    } else if let Change::AutoUpdate {
        update: Update::Image { policy, .. },
        ..
    } = change
    {
        Some(Feature::AutoUpdateEnabled(update_policy(*policy)))
    } else if let Change::ScheduleTask { .. } = change {
        Some(Feature::CronJobCreated)
    } else if let Change::Rollback { .. } = change {
        Some(Feature::RollbackUsed)
    } else {
        None
    }
}

/// What one created object shows: a database, a volume restore, a build, a workload, or host
/// names.
fn created(object: &Value, found: &mut Found, workloads: &mut usize, builds: &mut usize) {
    let api_version = text(object, "/apiVersion");
    let kind = text(object, "/kind");
    let group = api_version.split('/').next().unwrap_or_default();
    for (read, database) in DATABASES {
        if read.group == group && read.kind == kind {
            found.feature(Feature::DatabaseCreated(database));
        }
    }
    if group == k8up::RESTORES.group && kind == k8up::RESTORES.kind {
        found.feature(Feature::DataRestored(RestoreKind::Volume));
    }
    match (group, kind) {
        ("apps", "Deployment" | "StatefulSet") => *workloads += 1,
        (zeabur_plan::builds::BUILDER_GROUP, "Build") => {
            *builds += 1;
            found.feature(Feature::ServiceCreated(
                if object.pointer("/spec/source/type").and_then(Value::as_str) == Some("archive") {
                    Source::Upload
                } else {
                    Source::Git
                },
            ));
        }
        ("networking.k8s.io", "Ingress") => {
            let rules = object.pointer("/spec/rules").and_then(Value::as_array);
            found.hosts.extend(
                rules
                    .into_iter()
                    .flatten()
                    .filter_map(|rule| text_of(rule, "/host")),
            );
        }
        ("gateway.networking.k8s.io", "HTTPRoute") => {
            let names = object.pointer("/spec/hostnames").and_then(Value::as_array);
            found.hosts.extend(
                names
                    .into_iter()
                    .flatten()
                    .filter_map(|name| name.as_str().map(str::to_owned)),
            );
        }
        _ => {}
    }
}

const fn update_policy(policy: ImagePolicy) -> UpdatePolicy {
    match policy {
        ImagePolicy::SameTag => UpdatePolicy::SameTag,
        ImagePolicy::Patch => UpdatePolicy::Patch,
        ImagePolicy::Minor => UpdatePolicy::Minor,
        ImagePolicy::Major => UpdatePolicy::Major,
    }
}

fn text<'a>(object: &'a Value, pointer: &str) -> &'a str {
    object
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn text_of(object: &Value, pointer: &str) -> Option<String> {
    object
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn is_under(host: &str, suffix: &str) -> bool {
    host.strip_suffix(suffix)
        .is_some_and(|front| front.ends_with('.'))
}

/// The kinds of domain `hosts` are, each once: under one of the offered `suffixes` is generated.
fn domain_kinds(hosts: &[String], suffixes: &[String]) -> Vec<DomainKind> {
    let mut kinds: Vec<DomainKind> = Vec::new();
    for host in hosts {
        let host = host.to_ascii_lowercase();
        let mut kind = DomainKind::Custom;
        for suffix in suffixes {
            if is_under(&host, suffix) {
                kind = DomainKind::Generated;
            }
        }
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    kinds
}

/// Hands `used` each feature the applied `changes` used.
pub(crate) async fn report(
    used: &Used,
    cluster: &dyn Cluster,
    settings: &Settings,
    changes: &[Change],
) {
    let found = read(changes);
    for feature in found.features {
        used(feature);
    }
    if found.hosts.is_empty() {
        return;
    }
    let (suffixes, ..) = domains::effective(cluster, settings).await;
    for kind in domain_kinds(&found.hosts, &suffixes) {
        used(Feature::DomainAdded(kind));
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn changes(value: Value) -> Vec<Change> {
        serde_json::from_value(value).unwrap()
    }

    fn features(value: Value) -> Vec<Feature> {
        read(&changes(value)).features
    }

    fn deployment() -> Value {
        json!({"op": "create", "object": {"apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {"name": "api"}}})
    }

    fn build() -> Value {
        json!({"op": "create", "object": {"apiVersion": "builder.zeabur.com/v1alpha1",
            "kind": "Build", "metadata": {"name": "site"},
            "spec": {"source": {"type": "git", "git": {"url": "https://github.com/acme/site.git"}}}}})
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).to_owned()).collect()
    }

    #[test]
    fn an_image_service_is_found_once() {
        assert_eq!(
            features(json!([deployment(), deployment()])),
            [Feature::ServiceCreated(Source::Image)]
        );
    }

    #[test]
    fn a_service_built_from_git_is_not_also_an_image_service() {
        assert_eq!(
            features(json!([build(), deployment()])),
            [Feature::ServiceCreated(Source::Git)]
        );
    }

    #[test]
    fn a_git_service_and_an_image_service_are_both_found() {
        assert_eq!(
            features(json!([build(), deployment(), deployment()])),
            [
                Feature::ServiceCreated(Source::Git),
                Feature::ServiceCreated(Source::Image)
            ]
        );
    }

    #[test]
    fn an_upload_service_brings_its_hostname() {
        let found = read(&changes(json!([
            {"op": "createBuiltService", "name": "site", "port": 8080, "host": "site.example.com",
             "build": {"source": {"type": "upload", "request": "r1"}, "method": "railpack"}},
        ])));
        assert_eq!(found.features, [Feature::ServiceCreated(Source::Upload)]);
        assert_eq!(found.hosts, ["site.example.com"]);
    }

    #[test]
    fn a_running_service_given_git_or_a_folder_is_found_by_its_source() {
        let set = |source: Value| {
            json!({"op": "setBuild", "workload": {"kind": "Deployment", "name": "web"},
                "build": {"source": source, "method": "railpack"}})
        };
        let git = json!({"type": "git", "url": "https://github.com/acme/site.git",
            "access": {"type": "public"}});
        let folder = json!({"type": "upload", "request": "r1"});
        assert_eq!(
            features(json!([set(git)])),
            [Feature::BuildSourceSet(Source::Git)]
        );
        assert_eq!(
            features(json!([set(folder)])),
            [Feature::BuildSourceSet(Source::Upload)]
        );
    }

    #[test]
    fn a_template_is_found_by_its_catalog_id_with_its_domain() {
        let found = read(&changes(json!([
            {"op": "template", "template": "miniflux", "revision": "r", "domain": "rss.example.com"},
        ])));
        assert_eq!(
            found.features,
            [Feature::ServiceCreated(Source::Template("miniflux"))]
        );
        assert_eq!(found.hosts, ["rss.example.com"]);
    }

    #[test]
    fn a_page_server_is_found_as_its_template() {
        assert_eq!(
            features(json!([{"op": "pageServer", "maintenance": "<p>Back soon</p>"}])),
            [Feature::ServiceCreated(Source::Template("error-pages"))]
        );
    }

    #[test]
    fn a_database_is_found_by_its_kind() {
        let created = json!([{"op": "create", "object": {
            "apiVersion": "postgresql.cnpg.io/v1", "kind": "Cluster",
            "metadata": {"name": "db"}}}]);
        assert_eq!(
            features(created),
            [Feature::DatabaseCreated(DatabaseKind::Postgresql)]
        );
    }

    #[test]
    fn a_write_that_creates_no_service_finds_nothing() {
        let scale = json!([{"op": "scale",
            "workload": {"kind": "Deployment", "name": "web"}, "replicas": 2}]);
        let found = read(&changes(scale));
        assert_eq!(found.features, Vec::<Feature>::new());
        assert_eq!(found.hosts, Vec::<String>::new());
    }

    #[test]
    fn a_domain_under_an_offered_suffix_is_generated_and_any_other_is_custom() {
        let suffixes = strings(&["apps.example.com"]);
        assert_eq!(
            domain_kinds(&strings(&["web-shop.apps.example.com"]), &suffixes),
            [DomainKind::Generated]
        );
        assert_eq!(
            domain_kinds(&strings(&["notapps.example.com"]), &suffixes),
            [DomainKind::Custom]
        );
    }

    #[test]
    fn each_kind_of_domain_is_found_once() {
        let hosts = strings(&["a.apps.example.com", "b.apps.example.com", "a.org", "b.org"]);
        assert_eq!(
            domain_kinds(&hosts, &strings(&["apps.example.com"])),
            [DomainKind::Generated, DomainKind::Custom]
        );
    }

    fn web() -> Value {
        json!({"kind": "Deployment", "name": "web"})
    }

    fn route() -> Value {
        json!({"kind": "Ingress", "name": "web"})
    }

    #[test]
    fn opening_a_port_is_found_and_closing_one_is_not() {
        let port = |remove| {
            json!([{"op": "publicPort", "workload": web(), "service": "web", "port": 25565,
                    "remove": remove}])
        };
        assert_eq!(features(port(false)), [Feature::PublicPortOpened]);
        assert_eq!(features(port(true)), Vec::<Feature>::new());
    }

    #[test]
    fn each_limit_turned_on_is_found_by_its_kind_and_one_turned_off_is_not() {
        let on = json!([
            {"op": "ipAllow", "route": route(), "enabled": true, "cidrs": ["10.0.0.0/8"]},
            {"op": "basicAuth", "route": route(),
             "user": {"username": "ada", "password": "correct horse"}},
            {"op": "maintenance", "route": route(), "hostname": "web.example.com", "enabled": true},
        ]);
        assert_eq!(
            features(on),
            [
                Feature::AccessLimited(AccessLimit::IpAllow),
                Feature::AccessLimited(AccessLimit::BasicAuth),
                Feature::AccessLimited(AccessLimit::Maintenance),
            ]
        );
        let off = json!([
            {"op": "ipAllow", "route": route(), "enabled": false},
            {"op": "basicAuth", "route": route()},
            {"op": "maintenance", "route": route(), "hostname": "web.example.com", "enabled": false},
        ]);
        assert_eq!(features(off), Vec::<Feature>::new());
    }

    #[test]
    fn automatic_image_updates_are_found_by_policy_and_other_updates_are_not() {
        let update =
            |update: Value| json!([{"op": "autoUpdate", "workload": web(), "update": update}]);
        assert_eq!(
            features(update(json!({"kind": "image", "policy": "sameTag"}))),
            [Feature::AutoUpdateEnabled(UpdatePolicy::SameTag)]
        );
        assert_eq!(
            features(update(json!({"kind": "image", "policy": "minor"}))),
            [Feature::AutoUpdateEnabled(UpdatePolicy::Minor)]
        );
        assert_eq!(
            features(update(json!({"kind": "off"}))),
            Vec::<Feature>::new()
        );
    }

    #[test]
    fn a_scheduled_task_and_a_rollback_are_found() {
        assert_eq!(
            features(
                json!([{"op": "scheduleTask", "from": web(), "name": "nightly",
                             "schedule": "0 3 * * *", "command": "./cleanup"}])
            ),
            [Feature::CronJobCreated]
        );
        assert_eq!(
            features(json!([{"op": "rollback", "workload": web(), "revision": 2}])),
            [Feature::RollbackUsed]
        );
    }

    #[test]
    fn a_volume_restore_is_found() {
        let restore = json!([{"op": "create", "object": {"apiVersion": "k8up.io/v1",
            "kind": "Restore", "metadata": {"name": "data-restore"}}}]);
        assert_eq!(
            features(restore),
            [Feature::DataRestored(RestoreKind::Volume)]
        );
    }
}
