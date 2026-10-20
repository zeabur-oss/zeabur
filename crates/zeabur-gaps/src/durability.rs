//! Data that goes with the pod: a database whose data directory is on no persistent volume.
//!
//! A container's own filesystem lasts as long as the container, and an `emptyDir` volume as long
//! as the pod [k8s-volumes] [k8s-emptydir]; a new pod, from a rollout, an eviction or a node
//! drain, starts empty. Only a PersistentVolumeClaim outlives it [k8s-pv].
//!
//! Zeabur does not read image configurations, so it cannot see the `VOLUME`s an image declares.
//! It knows the data directories of the Docker official images that declare one, by repository
//! name, as each image's Dockerfile declares it [docker-postgres] [docker-mysql] [docker-mariadb]
//! [docker-mongo] [docker-redis] [docker-valkey] [docker-rabbitmq]. `postgres` 18 and later keep
//! their data under `/var/lib/postgresql`, earlier majors under `/var/lib/postgresql/data`. An
//! image from another publisher (`bitnami/postgresql`) keeps its data elsewhere and is not judged.
//!
//! A directory counts as kept when a persistent mount is at it, above it or below it: a mount
//! below it may not hold all of it, but saying "nothing is kept" then would be wrong.
//!
//! [k8s-volumes]: https://kubernetes.io/docs/concepts/storage/volumes/
//! [k8s-emptydir]: https://kubernetes.io/docs/concepts/storage/volumes/#emptydir
//! [k8s-pv]: https://kubernetes.io/docs/concepts/storage/persistent-volumes/
//! [docker-postgres]: https://github.com/docker-library/postgres
//! [docker-mysql]: https://github.com/docker-library/mysql
//! [docker-mariadb]: https://github.com/MariaDB/mariadb-docker
//! [docker-mongo]: https://github.com/docker-library/mongo
//! [docker-redis]: https://github.com/docker-library/redis
//! [docker-valkey]: https://github.com/valkey-io/valkey-container
//! [docker-rabbitmq]: https://github.com/docker-library/rabbitmq

use zeabur_model::{Citation, Container, Fix, FixKind, Gap, Message, MountCategory, Severity};

use crate::rules::{Input, Rule};

/// Where an official image keeps its data, by repository; `None` for an image this rule does
/// not know.
fn data_directory(image: &str) -> Option<(&'static str, &'static str)> {
    let (path, tag) = match image.rsplit_once(':') {
        Some((path, tag)) if !tag.contains('/') => (path, tag),
        _ => (image, ""),
    };
    let path = path.split('@').next().unwrap_or(path);
    let official = |name: &str| {
        [
            name.to_owned(),
            format!("library/{name}"),
            format!("docker.io/{name}"),
            format!("docker.io/library/{name}"),
        ]
        .contains(&path.to_owned())
    };
    let major: Option<u32> = tag.split(['.', '-']).next().and_then(|m| m.parse().ok());
    if official("postgres") {
        return Some(match major {
            Some(m) if m < 18 => ("postgres", "/var/lib/postgresql/data"),
            _ => ("postgres", "/var/lib/postgresql"),
        });
    }
    [
        ("mysql", "/var/lib/mysql"),
        ("mariadb", "/var/lib/mysql"),
        ("mongo", "/data/db"),
        ("redis", "/data"),
        ("rabbitmq", "/var/lib/rabbitmq"),
    ]
    .into_iter()
    .find(|(name, _)| official(name))
    .or_else(|| (path == "valkey/valkey").then_some(("valkey", "/data")))
}

/// Whether one of `a` and `b` is the other or a directory above it.
fn overlaps(a: &str, b: &str) -> bool {
    let a = a.trim_end_matches('/');
    let b = b.trim_end_matches('/');
    let within = |inner: &str, outer: &str| {
        inner == outer
            || outer.is_empty()
            || inner
                .strip_prefix(outer)
                .is_some_and(|r| r.starts_with('/'))
    };
    within(a, b) || within(b, a)
}

/// What keeps `directory` in `container`: a persistent mount, an `emptyDir`, or nothing.
fn kept_by(container: &Container, directory: &str) -> Option<MountCategory> {
    let at = |category: MountCategory| {
        container
            .mounts
            .iter()
            .any(|m| m.category == category && overlaps(&m.mount_path, directory))
    };
    if at(MountCategory::Persistent) {
        Some(MountCategory::Persistent)
    } else if at(MountCategory::Ephemeral) {
        Some(MountCategory::Ephemeral)
    } else {
        None
    }
}

/// A database whose data directory is on no persistent volume; see the module documentation.
#[derive(Debug, Clone, Copy)]
pub struct EphemeralData;

impl Rule for EphemeralData {
    fn id(&self) -> &'static str {
        "ephemeral-data"
    }

    fn citations(&self) -> Vec<Citation> {
        vec![
            Citation {
                title: "Volumes".to_owned(),
                url: "https://kubernetes.io/docs/concepts/storage/volumes/".to_owned(),
            },
            Citation {
                title: "Persistent Volumes".to_owned(),
                url: "https://kubernetes.io/docs/concepts/storage/persistent-volumes/".to_owned(),
            },
        ]
    }

    fn check(&self, input: &Input<'_>) -> Vec<Gap> {
        let mut gaps = Vec::new();
        for service in &input.project.services {
            for container in service.containers.iter().filter(|c| !c.init) {
                let Some((software, directory)) =
                    container.image.as_deref().and_then(data_directory)
                else {
                    continue;
                };
                let variant = match kept_by(container, directory) {
                    Some(MountCategory::Persistent) => continue,
                    Some(_) => "ephemeral",
                    None => "none",
                };
                gaps.push(Gap {
                    rule: self.id().to_owned(),
                    severity: if matches!(software, "redis" | "valkey") {
                        Severity::Advice
                    } else {
                        Severity::Warning
                    },
                    subject: container.origin.clone(),
                    service: Some(service.id.clone()),
                    message: Message::new(&format!("gap.ephemeralData.{variant}"))
                        .with("container", &container.name)
                        .with("software", software)
                        .with("path", directory),
                    fix: Fix {
                        kind: FixKind::Plan,
                        id: "storage.addVolume".to_owned(),
                        params: [
                            ("service", service.id.as_str()),
                            ("container", container.name.as_str()),
                            ("mountPath", directory),
                        ]
                        .into_iter()
                        .map(|(k, v)| (k.to_owned(), v.to_owned()))
                        .collect(),
                    },
                    citations: self.citations(),
                });
            }
        }
        gaps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_official_images_have_a_known_directory() {
        assert_eq!(
            data_directory("postgres:16"),
            Some(("postgres", "/var/lib/postgresql/data"))
        );
        assert_eq!(
            data_directory("postgres:18-alpine"),
            Some(("postgres", "/var/lib/postgresql"))
        );
        assert_eq!(
            data_directory("postgres"),
            Some(("postgres", "/var/lib/postgresql"))
        );
        assert_eq!(
            data_directory("docker.io/library/mysql:8.4"),
            Some(("mysql", "/var/lib/mysql"))
        );
        assert_eq!(data_directory("valkey/valkey:8"), Some(("valkey", "/data")));
        assert_eq!(data_directory("bitnami/postgresql:16"), None);
        assert_eq!(data_directory("ghcr.io/acme/redis:1"), None);
        assert_eq!(data_directory("registry.local:5000/redis"), None);
    }

    fn gaps(volumes: &serde_json::Value, mounts: &serde_json::Value) -> Vec<Gap> {
        let deployment = serde_json::json!({"apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {"name": "db"},
            "spec": {"selector": {"matchLabels": {"app": "db"}},
                "template": {"metadata": {"labels": {"app": "db"}},
                    "spec": {"volumes": volumes, "containers": [{"name": "postgres",
                        "image": "postgres:17", "volumeMounts": mounts}]}}}});
        let mut snapshot = zeabur_graph::Snapshot {
            namespace: "shop".to_owned(),
            ..zeabur_graph::Snapshot::default()
        };
        snapshot.insert(deployment).unwrap();
        let project = zeabur_graph::project(&snapshot);
        EphemeralData.check(&Input {
            project: &project,
            snapshot: &snapshot,
            http_routes: &[],
            services_readable: true,
            cluster: &crate::ClusterTraits::default(),
        })
    }

    #[test]
    fn a_database_on_the_pod_alone_is_a_warning_with_an_add_volume_fix() {
        let none = gaps(&serde_json::json!([]), &serde_json::json!([]));
        assert_eq!(none.len(), 1);
        assert_eq!(none[0].severity, Severity::Warning);
        assert_eq!(none[0].message.key, "gap.ephemeralData.none");
        assert_eq!(none[0].fix.id, "storage.addVolume");
        assert_eq!(
            none[0].fix.params.get("mountPath").map(String::as_str),
            Some("/var/lib/postgresql/data")
        );
        let scratch = gaps(
            &serde_json::json!([{"name": "data", "emptyDir": {}}]),
            &serde_json::json!([{"name": "data", "mountPath": "/var/lib/postgresql/data"}]),
        );
        assert_eq!(scratch[0].message.key, "gap.ephemeralData.ephemeral");
        let kept = gaps(
            &serde_json::json!([{"name": "data",
                "persistentVolumeClaim": {"claimName": "db-data"}}]),
            &serde_json::json!([{"name": "data", "mountPath": "/var/lib/postgresql/data"}]),
        );
        assert_eq!(kept, Vec::new());
    }

    #[test]
    fn a_mount_at_above_or_below_the_directory_keeps_it() {
        assert!(overlaps("/var/lib/postgresql/data", "/var/lib/postgresql"));
        assert!(overlaps("/var/lib/postgresql", "/var/lib/postgresql/data"));
        assert!(overlaps("/data/", "/data"));
        assert!(!overlaps("/data-old", "/data"));
        assert!(!overlaps("/var/lib/mysql-files", "/var/lib/mysql"));
    }
}
