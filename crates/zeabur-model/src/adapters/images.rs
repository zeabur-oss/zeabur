//! What a container image's name says about the software in it.
//!
//! Image names are a convention, not a contract: `postgres:16` and `bitnami/postgresql` are
//! PostgreSQL by name only. Zeabur uses the name for hints, never for a decision a person cannot
//! see and correct.

pub use crate::kernel::data_store::DataStore;

/// Repository names (the last path segment, without tag or digest) of widely used images for
/// data stores: databases, caches, queues and object stores, with the kind of each.
const DATA_STORES: &[(&str, DataStore)] = &[
    ("cassandra", DataStore::Other),
    ("clickhouse", DataStore::Search),
    ("clickhouse-server", DataStore::Search),
    ("cockroach", DataStore::Postgres),
    ("couchdb", DataStore::Other),
    ("elasticsearch", DataStore::Search),
    ("etcd", DataStore::Other),
    ("kafka", DataStore::Queue),
    ("keydb", DataStore::Redis),
    ("mariadb", DataStore::Mysql),
    ("memcached", DataStore::Redis),
    ("minio", DataStore::Other),
    ("mongo", DataStore::Mongo),
    ("mongodb", DataStore::Mongo),
    ("mysql", DataStore::Mysql),
    ("nats", DataStore::Queue),
    ("opensearch", DataStore::Search),
    ("postgres", DataStore::Postgres),
    ("postgresql", DataStore::Postgres),
    ("rabbitmq", DataStore::Queue),
    ("redis", DataStore::Redis),
    ("valkey", DataStore::Redis),
];

/// The repository name of `image`: its last path segment without tag or digest, so
/// `ghcr.io/acme/shop-api:2.1` is `shop-api`.
#[must_use]
pub fn repository_name(image: &str) -> &str {
    let last = image.rsplit('/').next().unwrap_or(image);
    last.split(['@', ':']).next().unwrap_or(last)
}

/// The kind of data store `image` is named like, if it is named like a well-known one.
#[must_use]
pub fn data_store_image(image: &str) -> Option<DataStore> {
    let name = repository_name(image);
    DATA_STORES
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, kind)| *kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_name_drops_registry_tag_and_digest() {
        assert_eq!(repository_name("ghcr.io/acme/shop-api:2.1"), "shop-api");
        assert_eq!(repository_name("postgres"), "postgres");
        assert_eq!(repository_name("redis@sha256:abc"), "redis");
        assert_eq!(repository_name("localhost:5000/redis:7"), "redis");
    }

    #[test]
    fn data_store_images_are_recognised_by_name_only() {
        assert_eq!(data_store_image("postgres:16"), Some(DataStore::Postgres));
        assert_eq!(
            data_store_image("docker.io/bitnami/postgresql:16.2.0"),
            Some(DataStore::Postgres)
        );
        assert_eq!(data_store_image("valkey/valkey:8"), Some(DataStore::Redis));
        assert_eq!(
            data_store_image("clickhouse/clickhouse-server:25.8"),
            Some(DataStore::Search)
        );
        assert_eq!(data_store_image("couchdb:3"), Some(DataStore::Other));
        assert_eq!(data_store_image("ghcr.io/acme/postgres-exporter:1"), None);
        assert_eq!(data_store_image("nginx:1.27"), None);
    }

    #[test]
    fn every_known_image_is_listed_once() {
        for (index, (name, _)) in DATA_STORES.iter().enumerate() {
            assert!(
                DATA_STORES[index + 1..]
                    .iter()
                    .all(|(other, _)| other != name),
                "{name} is listed twice"
            );
        }
    }
}
