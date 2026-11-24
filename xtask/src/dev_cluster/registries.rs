//! The node's `registries.yaml`: zot through its fixed ClusterIP, the cluster's registry before the
//! mirrors of Docker Hub, and the mirrors of `ZEABUR_REGISTRY_MIRRORS`.

use super::cluster::REGISTRY_HOST;
use super::{ZOT_CA, ZOT_HOST, ZOT_IP};

/// A registry the node pulls through another endpoint.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Mirror {
    registry: String,
    endpoint: String,
}

/// Reads `docker.io=http://mirror:5001,ghcr.io=...`; empty entries are skipped.
pub(crate) fn parse_mirrors(value: &str) -> Result<Vec<Mirror>, String> {
    value
        .split(',')
        .filter(|entry| !entry.is_empty())
        .map(|entry| match entry.split_once('=') {
            Some((registry, endpoint)) if !registry.is_empty() && !endpoint.is_empty() => {
                Ok(Mirror {
                    registry: registry.to_owned(),
                    endpoint: endpoint.to_owned(),
                })
            }
            _ => Err(format!(
                "ZEABUR_REGISTRY_MIRRORS: want registry=endpoint, got {entry}"
            )),
        })
        .collect()
}

/// Docker Hub's images that were pushed to the cluster's registry (`busybox:1.37`) come from it;
/// the registry answers 404 for the rest, and the node tries the next endpoint.
pub(crate) fn render(mirrors: &[Mirror]) -> String {
    let quote = |s: &str| serde_json::Value::from(s).to_string();
    let yaml = format!(
        "configs:\n  {}:\n    tls:\n      ca_file: {}\nmirrors:\n",
        quote(&format!("{ZOT_IP}:5000")),
        quote(ZOT_CA)
    );
    let zot = format!("https://{ZOT_IP}:5000");
    let local = format!("http://{REGISTRY_HOST}");
    let mut grouped: Vec<(&str, Vec<&str>)> = Vec::new();
    let entries = [(ZOT_HOST, zot.as_str()), ("docker.io", local.as_str())]
        .into_iter()
        .chain(
            mirrors
                .iter()
                .map(|mirror| (mirror.registry.as_str(), mirror.endpoint.as_str())),
        );
    for (registry, endpoint) in entries {
        match grouped.iter_mut().find(|(name, _)| *name == registry) {
            Some((_, endpoints)) => endpoints.push(endpoint),
            None => grouped.push((registry, vec![endpoint])),
        }
    }
    let entries: Vec<String> = grouped
        .iter()
        .map(|(registry, endpoints)| {
            let endpoints: Vec<String> = endpoints.iter().map(|e| quote(e)).collect();
            format!(
                "  {}:\n    endpoint: [{}]\n",
                quote(registry),
                endpoints.join(", ")
            )
        })
        .collect();
    yaml + &entries.concat()
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{parse_mirrors, render};

    fn parsed(mirrors: &str) -> Value {
        serde_saphyr::from_str(&render(&parse_mirrors(mirrors).unwrap())).unwrap()
    }

    #[test]
    fn zot_is_pulled_through_its_cluster_ip_against_its_ca() {
        let yaml = parsed("");
        assert_eq!(
            yaml["mirrors"]["zot.zot.svc:5000"]["endpoint"],
            json!(["https://10.43.0.50:5000"])
        );
        assert_eq!(
            yaml["configs"]["10.43.0.50:5000"]["tls"]["ca_file"],
            "/etc/rancher/k3s/zot-ca.crt"
        );
    }

    #[test]
    fn each_mirror_is_an_endpoint_of_its_registry() {
        let yaml = parsed("docker.io=http://10.211.55.122:5001,ghcr.io=http://m:5002/a=b,");
        assert_eq!(
            yaml["mirrors"]["docker.io"]["endpoint"],
            json!([
                "http://zeabur-dev-registry:5000",
                "http://10.211.55.122:5001"
            ])
        );
        assert_eq!(
            yaml["mirrors"]["ghcr.io"]["endpoint"],
            json!(["http://m:5002/a=b"])
        );
        assert!(yaml["mirrors"]["zot.zot.svc:5000"].is_object());
    }

    #[test]
    fn docker_hub_comes_from_the_cluster_registry_when_there_is_no_mirror() {
        assert_eq!(
            parsed("")["mirrors"]["docker.io"]["endpoint"],
            json!(["http://zeabur-dev-registry:5000"])
        );
    }

    #[test]
    fn a_mirror_without_registry_or_endpoint_is_refused() {
        for bad in ["docker.io", "=http://m", "docker.io=", "a=b,c"] {
            assert!(parse_mirrors(bad).is_err(), "{bad}");
        }
    }
}
