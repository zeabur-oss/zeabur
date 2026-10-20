//! Kubernetes label selectors [k8s-labels], the one matcher the projection (a Deployment's pods)
//! and the modules (NetworkPolicy, K8up) share.
//!
//! [k8s-labels]: https://kubernetes.io/docs/concepts/overview/working-with-objects/labels/#label-selectors

use std::collections::BTreeMap;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;

/// Whether `labels` satisfy every requirement of `selector`.
///
/// An empty selector matches everything, as in a NetworkPolicy; a caller for which it means
/// "nothing" checks [`is_empty`] first.
#[must_use]
pub fn matches(selector: &LabelSelector, labels: &BTreeMap<String, String>) -> bool {
    let by_labels = selector
        .match_labels
        .iter()
        .flatten()
        .all(|(k, v)| labels.get(k) == Some(v));
    let by_expressions = selector.match_expressions.iter().flatten().all(|e| {
        let values = e.values.as_deref().unwrap_or_default();
        let value = labels.get(&e.key);
        match e.operator.as_str() {
            "In" => value.is_some_and(|v| values.contains(v)),
            "NotIn" => value.is_none_or(|v| !values.contains(v)),
            "Exists" => value.is_some(),
            "DoesNotExist" => value.is_none(),
            // An operator Kubernetes does not define selects nothing.
            _ => false,
        }
    });
    by_labels && by_expressions
}

/// Whether `selector` has no requirement at all.
#[must_use]
pub fn is_empty(selector: &LabelSelector) -> bool {
    selector
        .match_labels
        .as_ref()
        .is_none_or(BTreeMap::is_empty)
        && selector
            .match_expressions
            .as_ref()
            .is_none_or(Vec::is_empty)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selector(value: serde_json::Value) -> LabelSelector {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn selectors_follow_kubernetes_set_semantics() {
        let labels = BTreeMap::from([
            ("app".to_owned(), "web".to_owned()),
            ("tier".to_owned(), "front".to_owned()),
        ]);
        let cases = [
            (serde_json::json!({"matchLabels": {"app": "web"}}), true),
            (serde_json::json!({"matchLabels": {"app": "db"}}), false),
            (
                serde_json::json!({"matchExpressions": [{"key": "tier", "operator": "In", "values": ["front", "back"]}]}),
                true,
            ),
            (
                serde_json::json!({"matchExpressions": [{"key": "tier", "operator": "NotIn", "values": ["front"]}]}),
                false,
            ),
            (
                serde_json::json!({"matchExpressions": [{"key": "zone", "operator": "NotIn", "values": ["a"]}]}),
                true,
            ),
            (
                serde_json::json!({"matchExpressions": [{"key": "app", "operator": "Exists"}]}),
                true,
            ),
            (
                serde_json::json!({"matchExpressions": [{"key": "app", "operator": "DoesNotExist"}]}),
                false,
            ),
            (
                serde_json::json!({"matchExpressions": [{"key": "zone", "operator": "DoesNotExist"}]}),
                true,
            ),
            (
                serde_json::json!({"matchExpressions": [{"key": "app", "operator": "Gt", "values": ["1"]}]}),
                false,
            ),
            // Requirements are ANDed: one failing expression fails the selector.
            (
                serde_json::json!({"matchLabels": {"app": "web"}, "matchExpressions": [{"key": "tier", "operator": "DoesNotExist"}]}),
                false,
            ),
            (serde_json::json!({}), true),
        ];
        for (value, expected) in cases {
            let selector = selector(value.clone());
            assert_eq!(matches(&selector, &labels), expected, "{value}");
        }
        assert!(is_empty(&selector(serde_json::json!({"matchLabels": {}}))));
        assert!(!is_empty(&selector(
            serde_json::json!({"matchExpressions": [{"key": "app", "operator": "Exists"}]})
        )));
    }
}
