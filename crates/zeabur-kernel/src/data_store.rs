//! The kinds of data store a service can be.
//!
//! One type for every layer: the adapters read it from an image's name, the projection decides
//! which services are data stores, and clients pick a service's glyph and place from it.

use serde::{Deserialize, Serialize};

/// What kind of data store a service is, as far as its image's name tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum DataStore {
    /// Postgres, or a database that speaks its protocol (`postgres`, `postgresql`, `cockroach`).
    Postgres,
    /// `mysql` and its fork `mariadb`.
    Mysql,
    /// An in-memory key-value store (`redis`, `valkey`, `keydb`, `memcached`).
    Redis,
    /// Mongo (`mongo`, `mongodb`).
    Mongo,
    /// A message queue or log (`rabbitmq`, `kafka`, `nats`).
    Queue,
    /// A search or analytics engine (`elasticsearch`, `opensearch`, `clickhouse`).
    Search,
    /// Any other data store, or one known only by its storage.
    Other,
}
