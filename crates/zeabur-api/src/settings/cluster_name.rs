//! What the interface calls the cluster, and where that name comes from.
//!
//! Kubernetes gives a cluster no name, and kube-apiserver's in-cluster address (`10.43.0.1`)
//! tells one cluster from another to no one. The first of these that gives one is the name:
//!
//! 1. A name chosen in the settings (the ConfigMap `zeabur-settings`, key `cluster-name`).
//! 2. The name the server was started with (`ZEABUR_CLUSTER_NAME`, the chart's `clusterName`).
//! 3. In local mode, the kubeconfig's name for the context's cluster, as it is.
//! 4. The host Zeabur is served at: the public URL's, else the request's `Host`.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, header};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::Cluster;
use zeabur_model::Origin;

use super::{
    CLUSTER_NAME_KEY, CONFIG_MAP, Settings, can_change, parse_cluster_name, read_stored, write_key,
};
use crate::error::ApiError;
use crate::handlers::{AppState, Person};
use crate::projects::namespace::Permission;
use crate::views::UnreadableView;

/// Where the cluster's name comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum NameSource {
    /// Chosen in Zeabur's settings.
    Setting,
    /// Given when Zeabur was installed (`ZEABUR_CLUSTER_NAME`).
    Install,
    /// The kubeconfig's name for the cluster (local mode).
    Kubeconfig,
    /// The host Zeabur is served at.
    Address,
}

/// The cluster's name for one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Named {
    pub(crate) name: String,
    pub(crate) source: NameSource,
}

impl Named {
    const fn new(name: String, source: NameSource) -> Self {
        Self { name, source }
    }
}

/// The host of a `Host` header, without its port.
fn host_of(headers: &HeaderMap) -> Option<String> {
    let host = headers.get(header::HOST)?.to_str().ok()?.trim();
    let host = if host.starts_with('[') {
        host.split_once(']')
            .map_or(host, |(h, _)| h)
            .trim_start_matches('[')
    } else {
        host.split(':').next().unwrap_or(host)
    };
    (!host.is_empty()).then(|| host.to_owned())
}

/// Whether `host` is an address rather than a name: an IPv4 or IPv6 literal, or a name made of
/// one (`ip-10-0-1-2.ec2.internal`).
fn is_address(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.contains(':') || host.parse::<std::net::Ipv4Addr>().is_ok() {
        return true;
    }
    let first = host.split('.').next().unwrap_or(host);
    first.strip_prefix("ip-").is_some_and(|rest| {
        let parts: Vec<&str> = rest.split('-').collect();
        parts.len() == 4 && parts.iter().all(|p| p.parse::<u8>().is_ok())
    })
}

/// The cluster's name for this person (see the module documentation).
pub(crate) async fn resolve(
    cluster: &dyn Cluster,
    settings: &Settings,
    headers: &HeaderMap,
) -> Named {
    let stored = read_stored(cluster, settings).await;
    if let Some(name) = stored
        .get(CLUSTER_NAME_KEY)
        .and_then(|text| parse_cluster_name(text).ok())
    {
        return Named::new(name, NameSource::Setting);
    }
    if let Some(name) = &settings.cluster_name {
        return Named::new(name.clone(), NameSource::Install);
    }
    if let Some(name) = cluster.kubeconfig_name() {
        return Named::new(name, NameSource::Kubeconfig);
    }
    let hosts: Vec<String> = settings
        .public_host
        .iter()
        .cloned()
        .chain(host_of(headers))
        .collect();
    let host = hosts
        .iter()
        .find(|host| !is_address(host))
        .or_else(|| hosts.first())
        .cloned()
        .unwrap_or_else(|| "localhost".to_owned());
    Named::new(host, NameSource::Address)
}

/// The cluster's name setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterNameSetting {
    /// `settings/cluster-name`.
    pub id: String,
    /// The name chosen in the settings; absent when none is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The name Zeabur was installed with (`ZEABUR_CLUSTER_NAME`); absent when it was not given
    /// one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// The ConfigMap a change is kept in; absent where Zeabur keeps no settings (on a person's
    /// computer).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stored_in: Option<Origin>,
    /// Whether the person may change it, as far as their RBAC rules in that namespace tell.
    pub can_change: Permission,
    /// The settings ConfigMap, when it could not be read.
    pub unreadable: Vec<UnreadableView>,
}

/// A change of the cluster's name.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClusterNameChange {
    /// The name, at most 63 characters; `null` goes back to the name Zeabur finds itself.
    pub name: Option<String>,
}

async fn view(cluster: &dyn Cluster, settings: &Settings) -> ClusterNameSetting {
    let stored = read_stored(cluster, settings).await;
    let can_change = can_change(cluster, settings, stored.object.is_some()).await;
    ClusterNameSetting {
        id: "settings/cluster-name".to_owned(),
        name: stored
            .get(CLUSTER_NAME_KEY)
            .and_then(|text| parse_cluster_name(text).ok()),
        default: settings.cluster_name.clone(),
        stored_in: settings
            .namespace
            .as_deref()
            .map(|namespace| Origin::complete("v1", "ConfigMap", Some(namespace), CONFIG_MAP)),
        can_change,
        unreadable: stored.unreadable.into_iter().collect(),
    }
}

/// The cluster's name chosen in the settings, the one Zeabur was installed with, and whether you
/// may change it. Readable by everyone signed in.
#[utoipa::path(get, path = "/api/v1/settings/cluster-name", tag = "settings",
    operation_id = "getClusterName",
    responses((status = 200, body = ClusterNameSetting),
              (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get_setting(
    State(state): State<AppState>,
    Person(cluster): Person,
) -> Json<ClusterNameSetting> {
    Json(view(cluster.as_ref(), &state.settings).await)
}

/// Renames the cluster in Zeabur, or goes back to the name Zeabur finds with `null`, by writing
/// the settings ConfigMap as you (created the first time). Who may: whoever may update (and
/// create) the ConfigMap `zeabur-settings` in Zeabur's namespace. Changes nothing in the cluster
/// itself.
#[utoipa::path(put, path = "/api/v1/settings/cluster-name", tag = "settings",
    operation_id = "setClusterName",
    request_body = ClusterNameChange,
    responses((status = 200, body = ClusterNameSetting),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn set(
    State(state): State<AppState>,
    Person(cluster): Person,
    Json(change): Json<ClusterNameChange>,
) -> Result<Json<ClusterNameSetting>, ApiError> {
    let Some(namespace) = state.settings.namespace.clone() else {
        return Err(ApiError::from(Problem::new(
            Kind::BadRequest,
            "Zeabur runs on this computer and keeps no settings: it names the cluster as your \
             kubeconfig does.",
        )));
    };
    let name = change
        .name
        .as_deref()
        .map(parse_cluster_name)
        .transpose()
        .map_err(|detail| ApiError::from(Problem::new(Kind::BadRequest, detail)))?;
    let stored = read_stored(cluster.as_ref(), &state.settings).await;
    write_key(
        cluster.as_ref(),
        &namespace,
        stored.object.as_ref(),
        CLUSTER_NAME_KEY,
        name.as_deref(),
    )
    .await?;
    Ok(Json(view(cluster.as_ref(), &state.settings).await))
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn the_host_header_loses_its_port() {
        let mut headers = HeaderMap::new();
        for (value, host) in [
            ("zeabur.localhost", "zeabur.localhost"),
            ("zeabur.localhost:8443", "zeabur.localhost"),
            ("[::1]:8080", "::1"),
            ("127.0.0.1:5173", "127.0.0.1"),
        ] {
            headers.insert(header::HOST, HeaderValue::from_static(value));
            assert_eq!(host_of(&headers).as_deref(), Some(host), "{value}");
        }
        assert_eq!(host_of(&HeaderMap::new()), None);
    }

    #[test]
    fn addresses_are_told_from_names() {
        for address in [
            "10.43.0.1",
            "[fd00::1]",
            "fd00::1",
            "ip-10-0-1-2.ec2.internal",
        ] {
            assert!(is_address(address), "{address}");
        }
        for name in ["zeabur.localhost", "vps-1", "ip-range", "k3d-dev-server-0"] {
            assert!(!is_address(name), "{name}");
        }
    }
}
