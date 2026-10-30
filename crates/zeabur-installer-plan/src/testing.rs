//! The pinned catalog release for tests, with `testedOn` and `health` filled in where the release
//! has none yet.

use serde_json::{Value, json};

use crate::catalog::Index;

/// What the catalog's `KubeSolo` run installs, by capability.
const KUBESOLO: &[&str] = &["ingress", "certificates", "builds", "databases"];

/// The pinned `index.json`, as it is.
pub const INDEX: &str = include_str!(concat!(env!("ZEABUR_CATALOG_RELEASE"), "/index.json"));

/// The pinned index, with `testedOn` and `health` filled in where it has none.
///
/// A component without `testedOn` is tested on K3s and RKE2, and on `KubeSolo` when its
/// capability is one the catalog's `KubeSolo` run installs, within its `distributions`. A
/// component without `health` gets its `smoke` waits that need no test objects.
///
/// # Panics
///
/// When the pinned index does not parse.
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "a test cannot go on without its catalog"
)]
pub fn index() -> Index {
    let mut value: Value = serde_json::from_str(INDEX).expect("the pinned index.json");
    let components = value.get_mut("components").and_then(Value::as_array_mut);
    for component in components.into_iter().flatten() {
        if component.get("testedOn").is_none() {
            let capability = component["capability"].as_str().unwrap_or_default();
            let limits: Vec<&str> = component["distributions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let tested: Vec<&str> = ["k3s", "rke2", "kubesolo"]
                .into_iter()
                .filter(|d| *d != "kubesolo" || KUBESOLO.contains(&capability))
                .filter(|d| limits.is_empty() || limits.contains(d))
                .collect();
            component["testedOn"] = json!(tested);
        }
        if component.get("health").is_none() {
            let health: Vec<Value> = component["smoke"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|s| s.get("manifest").is_none() && s.get("wait").is_some())
                .map(|s| {
                    json!({
                        "name": s["name"],
                        "wait": s["wait"],
                        "timeout": "60s",
                        "advice": format!("Check {}.", component["id"].as_str().unwrap_or_default()),
                    })
                })
                .collect();
            component["health"] = json!(health);
        }
    }
    Index::parse(value.to_string().as_bytes()).expect("the pinned index with testedOn")
}
