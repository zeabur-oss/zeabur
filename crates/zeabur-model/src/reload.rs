//! Which ConfigMaps and Secrets a workload asks Stakater Reloader to restart it for.

use serde::{Deserialize, Serialize};

/// What a workload's Reloader annotations ask for [reloader]: its pods are replaced when one of
/// these ConfigMaps or Secrets changes, whoever changed it, while Reloader runs in the cluster.
///
/// Reloader also reads `reloader.stakater.com/search` with `reloader.stakater.com/match` on the
/// ConfigMap or Secret, and `reloader.stakater.com/ignore`; those live on the data objects and
/// are not part of this.
///
/// [reloader]: https://github.com/stakater/Reloader#how-to-use-reloader
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ConfigReload {
    /// `reloader.stakater.com/auto: "true"`: every ConfigMap and Secret it reads. This is what
    /// Zeabur's setting writes.
    pub auto: bool,
    /// `configmap.reloader.stakater.com/auto: "true"`: every ConfigMap it reads.
    pub all_config_maps: bool,
    /// `secret.reloader.stakater.com/auto: "true"`: every Secret it reads.
    pub all_secrets: bool,
    /// ConfigMaps named in `configmap.reloader.stakater.com/reload`.
    pub config_maps: Vec<String>,
    /// Secrets named in `secret.reloader.stakater.com/reload`.
    pub secrets: Vec<String>,
}

impl ConfigReload {
    /// The annotation Zeabur's setting writes on the workload.
    pub const AUTO: &'static str = "reloader.stakater.com/auto";
    const CONFIG_MAPS_AUTO: &'static str = "configmap.reloader.stakater.com/auto";
    const SECRETS_AUTO: &'static str = "secret.reloader.stakater.com/auto";
    const CONFIG_MAPS: &'static str = "configmap.reloader.stakater.com/reload";
    const SECRETS: &'static str = "secret.reloader.stakater.com/reload";

    /// What `annotations`, a workload's, ask for; `None` when they ask for nothing.
    #[must_use]
    pub fn from_annotations<'a>(
        annotations: impl IntoIterator<Item = (&'a String, &'a String)>,
    ) -> Option<Self> {
        let mut reload = Self::default();
        let names = |value: &str| -> Vec<String> {
            value
                .split(',')
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(str::to_owned)
                .collect()
        };
        for (key, value) in annotations {
            let on = value.trim().eq_ignore_ascii_case("true");
            match key.as_str() {
                Self::AUTO => reload.auto = on,
                Self::CONFIG_MAPS_AUTO => reload.all_config_maps = on,
                Self::SECRETS_AUTO => reload.all_secrets = on,
                Self::CONFIG_MAPS => reload.config_maps = names(value),
                Self::SECRETS => reload.secrets = names(value),
                _ => {}
            }
        }
        (reload != Self::default()).then_some(reload)
    }

    /// Whether a change of the ConfigMap (`secret` false) or Secret `name` restarts it.
    #[must_use]
    pub fn follows(&self, secret: bool, name: &str) -> bool {
        if secret {
            self.auto || self.all_secrets || self.secrets.iter().any(|n| n == name)
        } else {
            self.auto || self.all_config_maps || self.config_maps.iter().any(|n| n == name)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn read(pairs: &[(&str, &str)]) -> Option<ConfigReload> {
        let map: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        ConfigReload::from_annotations(&map)
    }

    #[test]
    fn auto_follows_every_config_map_and_secret() {
        let reload = read(&[("reloader.stakater.com/auto", "true")]).unwrap();
        assert!(reload.follows(false, "app"));
        assert!(reload.follows(true, "db"));
    }

    #[test]
    fn named_lists_follow_only_those() {
        let reload = read(&[
            ("configmap.reloader.stakater.com/reload", "app, flags"),
            ("secret.reloader.stakater.com/reload", "db"),
        ])
        .unwrap();
        assert!(reload.follows(false, "flags"));
        assert!(!reload.follows(false, "db"));
        assert!(reload.follows(true, "db"));
        assert!(!reload.follows(true, "app"));
    }

    #[test]
    fn false_and_other_annotations_ask_for_nothing() {
        assert_eq!(read(&[("reloader.stakater.com/auto", "false")]), None);
        assert_eq!(read(&[("example.com/auto", "true")]), None);
    }
}
