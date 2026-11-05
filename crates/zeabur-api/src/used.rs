//! Which features a successful write used, for the experience program
//! (docs/reference/telemetry.mdx). The server reads only the kinds of what a plan creates, never
//! a name, and hands each feature to [`Used`].

use std::sync::Arc;

use serde_json::Value;
use zeabur_kube::Cluster;
use zeabur_modules::{
    Read, cloudnative_pg, dragonfly, mariadb_operator, moco, percona_mongodb, seaweedfs,
};
use zeabur_plan::{Change, created_object};
use zeabur_telemetry::{DatabaseKind, DomainKind, Feature, Source};

use crate::settings::{Settings, domains};

/// What receives a feature after the call that used it succeeded; it decides whether to send.
pub type Used = Arc<dyn Fn(Feature) + Send + Sync>;

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
    let mut workload = false;
    let mut build = None;
    for change in changes {
        if let Change::Template(template) = change {
            if let Some(id) = zeabur_catalog::template_id(&template.template) {
                found.feature(Feature::ServiceCreated(Source::Template(id)));
            }
            found.hosts.extend(template.domain.clone());
        } else if matches!(change, Change::CreateUploadService(_)) {
            found.feature(Feature::ServiceCreated(Source::Upload));
        } else if let Change::AddDomain(add) = change {
            found.hosts.push(add.hostname.clone());
        } else if let Some(Ok(object)) = created_object(change) {
            created(&object, &mut found, &mut workload, &mut build);
        }
    }
    if let Some(source) = build {
        found.feature(Feature::ServiceCreated(source));
    } else if workload {
        found.feature(Feature::ServiceCreated(Source::Image));
    }
    found
}

/// What one created object shows: a database, a build, a workload, or host names.
fn created(object: &Value, found: &mut Found, workload: &mut bool, build: &mut Option<Source>) {
    let api_version = text(object, "/apiVersion");
    let kind = text(object, "/kind");
    let group = api_version.split('/').next().unwrap_or_default();
    for (read, database) in DATABASES {
        if read.group == group && read.kind == kind {
            found.feature(Feature::DatabaseCreated(database));
        }
    }
    match (group, kind) {
        ("apps", "Deployment" | "StatefulSet") => *workload = true,
        (zeabur_plan::builds::BUILDER_GROUP, "Build") => {
            *build = Some(if object.pointer("/spec/source/archive").is_some() {
                Source::Upload
            } else {
                Source::Git
            });
        }
        ("networking.k8s.io", "Ingress") => {
            let rules = object.pointer("/spec/rules").and_then(Value::as_array);
            for rule in rules.into_iter().flatten() {
                found.hosts.extend(text_of(rule, "/host"));
            }
        }
        ("gateway.networking.k8s.io", "HTTPRoute") => {
            let names = object.pointer("/spec/hostnames").and_then(Value::as_array);
            for name in names.into_iter().flatten() {
                found.hosts.extend(name.as_str().map(str::to_owned));
            }
        }
        _ => {}
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

/// Hands `used` each feature the applied `changes` used.
pub(crate) async fn report(
    used: &Used,
    cluster: &dyn Cluster,
    settings: &Settings,
    changes: &[Change],
) {
    let found = read(changes);
    for feature in &found.features {
        used(*feature);
    }
    if found.hosts.is_empty() {
        return;
    }
    let (suffixes, ..) = domains::effective(cluster, settings).await;
    let mut kinds: Vec<DomainKind> = Vec::new();
    for host in &found.hosts {
        let generated = suffixes
            .iter()
            .any(|s| is_under(&host.to_ascii_lowercase(), s));
        let kind = if generated {
            DomainKind::Generated
        } else {
            DomainKind::Custom
        };
        if !kinds.contains(&kind) {
            kinds.push(kind);
            used(Feature::DomainAdded(kind));
        }
    }
}
