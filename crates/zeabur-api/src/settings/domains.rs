//! The domains new URLs are suggested under, and whether DNS sends each to the cluster.
//!
//! A suffix such as `apps.example.com` gives a new service the URL `<service>-<project>.<suffix>`.
//! One wildcard record, `*.apps.example.com`, sends every such URL to the cluster's ingress, and
//! each URL gets its own certificate from the cluster issuer (HTTP-01). A suffix can also be the
//! zone itself (`example.com`). `<address>.sslip.io` resolves to `<address>` with no record.
//!
//! The suffixes the server was started with (`ZEABUR_DOMAIN_SUFFIXES`, the chart's
//! `domains.suffixes`, which the installer fills) are the default. A list chosen on the cluster
//! page replaces it, kept in the settings ConfigMap under [`DOMAIN_SUFFIXES_KEY`].
//!
//! The check looks up a name no one uses, `zeabur-check-<random>.<suffix>`, from where Zeabur
//! runs: only a wildcard record answers it. It points here when one of the addresses is one the
//! cluster's Ingresses are served on (`status.loadBalancer`).

use std::net::{Ipv4Addr, Ipv6Addr};

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::{Cluster, ResourceRef};
use zeabur_model::{DnsState, Origin};

use super::{
    CONFIG_MAP, DOMAIN_SUFFIXES_KEY, SettingSource, Settings, can_change, read_stored, write_key,
};
use crate::error::ApiError;
use crate::handlers::{AppState, Person};
use crate::projects::namespace::Permission;
use crate::resolve::Resolved;
use crate::views::UnreadableView;

/// The most suffixes a cluster may have.
pub const SUFFIXES_MAX: usize = 20;

/// A suffix as a person or an installer wrote it: trimmed, lower case, without `*.` or the final
/// dot, a domain name with at least two labels.
///
/// # Errors
///
/// What is wrong with `text`, to show the person.
pub fn parse_suffix(text: &str) -> Result<String, String> {
    let suffix = text
        .trim()
        .trim_start_matches("*.")
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if !zeabur_plan::checks::is_dns_subdomain(&suffix) || !suffix.contains('.') {
        return Err(format!(
            "{:?} is not a domain name such as apps.example.com.",
            text.trim()
        ));
    }
    Ok(suffix)
}

/// Suffixes separated by commas or lines, each once, in their order.
///
/// # Errors
///
/// The first suffix that is not a domain name, or more than [`SUFFIXES_MAX`].
pub fn parse_suffixes(text: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for part in text.split([',', '\n']).filter(|p| !p.trim().is_empty()) {
        let suffix = parse_suffix(part)?;
        if !out.contains(&suffix) {
            out.push(suffix);
        }
    }
    if out.len() > SUFFIXES_MAX {
        return Err(format!("Give at most {SUFFIXES_MAX} suffixes."));
    }
    Ok(out)
}

/// The address an sslip.io or nip.io suffix resolves to: `203-0-113-7.sslip.io`,
/// `203.0.113.7.sslip.io` or `2001-db8--1.sslip.io`.
#[must_use]
pub fn sslip_address(suffix: &str) -> Option<String> {
    let rest = suffix
        .strip_suffix(".sslip.io")
        .or_else(|| suffix.strip_suffix(".nip.io"))?;
    if let Ok(v4) = rest.parse::<Ipv4Addr>() {
        return Some(v4.to_string());
    }
    if rest.contains('.') {
        return None;
    }
    if let Ok(v4) = rest.replace('-', ".").parse::<Ipv4Addr>() {
        return Some(v4.to_string());
    }
    rest.replace('-', ":")
        .parse::<Ipv6Addr>()
        .ok()
        .map(|v6| v6.to_string())
}

/// The suffixes in use for this person, and where they come from.
pub(crate) async fn effective(
    cluster: &dyn Cluster,
    settings: &Settings,
) -> (Vec<String>, SettingSource, super::store::Stored) {
    let stored = read_stored(cluster, settings).await;
    let chosen = stored
        .get(DOMAIN_SUFFIXES_KEY)
        .and_then(|text| parse_suffixes(text).ok());
    match chosen {
        Some(list) => (list, SettingSource::Setting, stored),
        None => (
            settings.domains.suffixes.clone(),
            SettingSource::Default,
            stored,
        ),
    }
}

/// How DNS answers for one suffix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SuffixCheck {
    /// The suffix, such as `apps.example.com`.
    pub name: String,
    /// The name looked up: one no one uses, so only a wildcard record answers it.
    pub probe: String,
    /// Whether it resolves to an address of the cluster's ingress.
    pub dns: DnsState,
    /// The addresses it resolved to.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub resolved: Vec<String>,
    /// For an sslip.io or nip.io suffix, the address it carries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sslip: Option<String>,
}

/// The domains new URLs are suggested under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DomainsSetting {
    /// `settings/domains`.
    pub id: String,
    /// The suffixes in use, the default first, each with its check.
    pub suffixes: Vec<SuffixCheck>,
    /// Whether they were chosen on the cluster page or come from the install.
    pub source: SettingSource,
    /// The suffixes Zeabur was installed with (`ZEABUR_DOMAIN_SUFFIXES`).
    pub default: Vec<String>,
    /// The addresses the cluster's Ingresses are served on: what a wildcard record points to.
    /// Empty when none could be read.
    pub target: Vec<String>,
    /// The `ClusterIssuer` a new URL's certificate comes from; absent: new URLs are HTTP only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_issuer: Option<String>,
    /// The ConfigMap a change is kept in; absent where Zeabur keeps no settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stored_in: Option<Origin>,
    /// Whether the person may change them.
    pub can_change: Permission,
    /// What could not be read.
    pub unreadable: Vec<UnreadableView>,
}

/// A change of the suffixes.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DomainsChange {
    /// The suffixes, the default first; `null` goes back to the ones Zeabur was installed with.
    pub suffixes: Option<Vec<String>>,
}

const INGRESSES: (&str, &str, &str, &str) = ("networking.k8s.io", "v1", "ingresses", "Ingress");

/// The addresses in `status.loadBalancer.ingress` of `ingresses`.
fn load_balancers(ingresses: &[Value]) -> Vec<String> {
    let mut out: Vec<String> = ingresses
        .iter()
        .filter_map(|i| i.pointer("/status/loadBalancer/ingress")?.as_array())
        .flatten()
        .filter_map(|entry| {
            entry
                .get("ip")
                .or_else(|| entry.get("hostname"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The cluster's ingress addresses as the person may read them: every namespace's Ingresses,
/// else Zeabur's own.
async fn target(
    cluster: &dyn Cluster,
    settings: &Settings,
    unreadable: &mut Vec<UnreadableView>,
) -> Vec<String> {
    let (group, version, plural, kind) = INGRESSES;
    let resource = ResourceRef::new(group, version, plural, kind);
    let objects = match cluster.list_kind(&resource, None).await {
        Ok(objects) => objects,
        Err(error) => {
            if let Ok(refused) = error.into_unreadable(plural) {
                unreadable.push(refused.into());
            }
            match &settings.namespace {
                Some(namespace) => cluster
                    .list_kind(&resource, Some(namespace))
                    .await
                    .unwrap_or_default(),
                None => Vec::new(),
            }
        }
    };
    let values: Vec<Value> = objects
        .iter()
        .filter_map(|o| serde_json::to_value(o).ok())
        .collect();
    load_balancers(&values)
}

fn check(name: &str, probe: String, answer: Option<&Resolved>, target: &[String]) -> SuffixCheck {
    let (dns, resolved) = match answer {
        Some(Resolved::Addresses(found)) if !found.is_empty() => {
            let state = if target.is_empty() {
                DnsState::Unknown
            } else if found.iter().any(|a| target.contains(a)) {
                DnsState::PointsHere
            } else {
                DnsState::Elsewhere
            };
            (state, found.clone())
        }
        Some(Resolved::NotFound(_)) => (DnsState::Missing, Vec::new()),
        _ => (DnsState::Unknown, Vec::new()),
    };
    SuffixCheck {
        name: name.to_owned(),
        probe,
        dns,
        resolved,
        sslip: sslip_address(name),
    }
}

async fn view(state: &AppState, cluster: &dyn Cluster) -> DomainsSetting {
    let settings = &state.settings;
    let (suffixes, source, stored) = effective(cluster, settings).await;
    let mut unreadable: Vec<UnreadableView> = stored.unreadable.clone().into_iter().collect();
    let can_change = can_change(cluster, settings, stored.object.is_some()).await;
    let target = target(cluster, settings, &mut unreadable).await;
    let label = zeabur_core::crypto::random::<4>().map_or_else(
        |_| "0".to_owned(),
        |bytes| u32::from_be_bytes(bytes).to_string(),
    );
    let probes: Vec<String> = suffixes
        .iter()
        .map(|s| format!("zeabur-check-{label}.{s}"))
        .collect();
    let answers = crate::resolve::all(state.resolver.as_ref(), probes.clone()).await;
    DomainsSetting {
        id: "settings/domains".to_owned(),
        suffixes: suffixes
            .iter()
            .zip(probes)
            .map(|(name, probe)| {
                let answer = answers.get(&probe);
                check(name, probe, answer, &target)
            })
            .collect(),
        source,
        default: settings.domains.suffixes.clone(),
        target,
        cluster_issuer: settings.domains.cluster_issuer.clone(),
        stored_in: settings
            .namespace
            .as_deref()
            .map(|namespace| Origin::complete("v1", "ConfigMap", Some(namespace), CONFIG_MAP)),
        can_change,
        unreadable,
    }
}

/// The domains new URLs are suggested under, whether a wildcard record sends each to the
/// cluster, the addresses it should point to, and whether you may change them. Readable by
/// everyone signed in; each request looks the suffixes up again.
#[utoipa::path(get, path = "/api/v1/settings/domains", tag = "settings",
    operation_id = "getDomains",
    responses((status = 200, body = DomainsSetting),
              (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get_setting(
    State(state): State<AppState>,
    Person(cluster): Person,
) -> Json<DomainsSetting> {
    Json(view(&state, cluster.as_ref()).await)
}

/// Changes the domains new URLs are suggested under, or goes back to the ones Zeabur was
/// installed with (`null`), by writing the settings ConfigMap as you. Existing URLs keep their
/// names. Who may: whoever may update (and create) the ConfigMap `zeabur-settings` in Zeabur's
/// namespace.
#[utoipa::path(put, path = "/api/v1/settings/domains", tag = "settings",
    operation_id = "setDomains",
    request_body = DomainsChange,
    responses((status = 200, body = DomainsSetting),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn set(
    State(state): State<AppState>,
    Person(cluster): Person,
    Json(change): Json<DomainsChange>,
) -> Result<Json<DomainsSetting>, ApiError> {
    let Some(namespace) = state.settings.namespace.clone() else {
        return Err(ApiError::from(Problem::new(
            Kind::BadRequest,
            "Zeabur runs on this computer and keeps no settings: start it with \
             --domain-suffixes instead.",
        )));
    };
    let text = change
        .suffixes
        .map(|list| parse_suffixes(&list.join("\n")).map(|list| list.join("\n")))
        .transpose()
        .map_err(|detail| ApiError::from(Problem::new(Kind::BadRequest, detail)))?;
    let stored = read_stored(cluster.as_ref(), &state.settings).await;
    write_key(
        cluster.as_ref(),
        &namespace,
        stored.object.as_ref(),
        DOMAIN_SUFFIXES_KEY,
        text.as_deref(),
    )
    .await?;
    Ok(Json(view(&state, cluster.as_ref()).await))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn suffixes_are_domain_names_once_each() {
        assert_eq!(
            parse_suffixes("*.Apps.example.com., example.com\napps.example.com").unwrap(),
            ["apps.example.com", "example.com"]
        );
        assert_eq!(parse_suffixes(" , ").unwrap(), Vec::<String>::new());
        assert!(parse_suffix("localhost").is_err());
        assert!(parse_suffix("apps_example.com").is_err());
    }

    #[test]
    fn sslip_suffixes_carry_their_address() {
        assert_eq!(
            sslip_address("203-0-113-7.sslip.io").as_deref(),
            Some("203.0.113.7")
        );
        assert_eq!(
            sslip_address("203.0.113.7.nip.io").as_deref(),
            Some("203.0.113.7")
        );
        assert_eq!(
            sslip_address("2001-db8--1.sslip.io").as_deref(),
            Some("2001:db8::1")
        );
        assert_eq!(sslip_address("apps.sslip.io"), None);
        assert_eq!(sslip_address("apps.example.com"), None);
    }

    #[test]
    fn a_suffix_points_here_when_an_address_is_the_ingress() {
        let target = load_balancers(&[
            json!({"status": {"loadBalancer": {"ingress": [{"ip": "203.0.113.7"}]}}}),
            json!({"status": {"loadBalancer": {"ingress": [{"ip": "203.0.113.7"}, {"hostname": "lb.example.net"}]}}}),
            json!({"status": {}}),
        ]);
        assert_eq!(target, ["203.0.113.7", "lb.example.net"]);
        let here = Resolved::Addresses(vec!["203.0.113.7".to_owned()]);
        let away = Resolved::Addresses(vec!["198.51.100.1".to_owned()]);
        let none = Resolved::NotFound("no such host".to_owned());
        let probe = || "zeabur-check-1.apps.example.com".to_owned();
        assert_eq!(
            check("apps.example.com", probe(), Some(&here), &target).dns,
            DnsState::PointsHere
        );
        assert_eq!(
            check("apps.example.com", probe(), Some(&away), &target).dns,
            DnsState::Elsewhere
        );
        assert_eq!(
            check("apps.example.com", probe(), Some(&none), &target).dns,
            DnsState::Missing
        );
        assert_eq!(
            check("apps.example.com", probe(), Some(&here), &[]).dns,
            DnsState::Unknown
        );
    }
}
