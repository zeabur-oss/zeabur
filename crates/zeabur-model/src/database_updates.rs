//! The image settings that Zeabur can change on a CloudNativePG Cluster.

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde_json::Value;

use crate::adapters::image_reference::Reference;
use crate::adapters::provenance::{detect_tool, reapplying_tool};
use crate::{
    ImagePolicy, ImageUpdates, KEEL_ANNOTATIONS, KEEL_MATCH_TAG, KEEL_POLICY, KEEL_POLL_SCHEDULE,
    KEEL_TRIGGER, UpdateSetting,
};

/// The settings of a CloudNativePG Cluster. Its owner controls a managed Cluster. On a
/// directly managed Cluster, Off remains available even if its image can no longer update.
#[must_use]
pub fn database_image_updates(meta: &ObjectMeta, spec: &Value) -> Option<ImageUpdates> {
    if meta.deletion_timestamp.is_some() || detect_tool(meta).is_some() || owns_settings(meta) {
        return None;
    }
    let annotations = meta.annotations.as_ref();
    let labels = meta.labels.as_ref();
    let annotation = |key: &str| annotations.and_then(|a| a.get(key)).map(String::as_str);
    let value = |key: &str| {
        labels
            .and_then(|l| l.get(key))
            .map(String::as_str)
            .or_else(|| annotation(key))
    };
    let approved = [annotations, labels].into_iter().flatten().all(|values| {
        values
            .get("keel.sh/approvals")
            .is_none_or(|v| v.is_empty() || v == "0")
    });
    let allowed = if approved
        && meta.owner_references.as_ref().is_none_or(Vec::is_empty)
        && spec.get("imageCatalogRef").is_none_or(Value::is_null)
        && !owned(meta, |fields| {
            fields.pointer("/f:spec/f:imageName").is_some()
        })
        && spec
            .get("imageName")
            .and_then(Value::as_str)
            .is_some_and(major_tag)
    {
        vec![ImagePolicy::SameTag]
    } else {
        Vec::new()
    };
    let policy = value(KEEL_POLICY);
    let schedule = annotation(KEEL_POLL_SCHEDULE);
    let setting = policy
        .filter(|p| !p.is_empty() && *p != "never")
        .map(|policy| {
            let other = || UpdateSetting::Other {
                policy: policy.to_owned(),
                schedule: schedule.map(str::to_owned),
            };
            if database_poll_schedule(meta).is_none() {
                return other();
            }
            match crate::image_updates::setting(policy, value(KEEL_TRIGGER), schedule, Some("true"))
            {
                Some(
                    setting @ UpdateSetting::Image {
                        policy: ImagePolicy::SameTag,
                        ..
                    },
                ) => setting,
                _ => other(),
            }
        });
    Some(ImageUpdates { setting, allowed })
}

/// The configured poll schedule for a normalized, same-tag database setting. Legacy labels
/// are presented as external settings until a reviewed edit clears them.
#[must_use]
pub fn database_poll_schedule(meta: &ObjectMeta) -> Option<&str> {
    let annotations = meta.annotations.as_ref()?;
    let value = |key: &str| annotations.get(key).map(String::as_str);
    let legacy = meta.labels.as_ref().is_some_and(|labels| {
        KEEL_ANNOTATIONS
            .into_iter()
            .chain(["keel.sh/matchTag"])
            .any(|key| labels.contains_key(key))
    });
    let same_tag = value(KEEL_MATCH_TAG)
        .or_else(|| value("keel.sh/matchTag"))
        .is_some_and(|v| v.eq_ignore_ascii_case("true"));
    if legacy
        || !same_tag
        || value(KEEL_POLICY) != Some("force")
        || value(KEEL_TRIGGER) != Some("poll")
    {
        return None;
    }
    value(KEEL_POLL_SCHEDULE).filter(|s| !s.is_empty())
}

fn major_tag(image: &str) -> bool {
    if image.trim() != image || Reference::parse(image).is_none() {
        return false;
    }
    let tagged = image.split('@').next().unwrap_or(image);
    let Some((_, tag)) = tagged
        .rsplit('/')
        .next()
        .and_then(|last| last.split_once(':'))
    else {
        return false;
    };
    let major = tag
        .strip_prefix('v')
        .unwrap_or(tag)
        .split(['.', '-'])
        .next()
        .unwrap_or_default();
    !major.starts_with('0')
        && major.bytes().all(|b| b.is_ascii_digit())
        && major.parse::<u32>().is_ok_and(|n| n >= 10)
}

fn owned(meta: &ObjectMeta, field: impl Fn(&Value) -> bool) -> bool {
    meta.managed_fields.iter().flatten().any(|entry| {
        entry.subresource.as_deref().unwrap_or_default().is_empty()
            && entry.manager.as_deref().and_then(reapplying_tool).is_some()
            && entry
                .fields_v1
                .as_ref()
                .is_some_and(|fields| field(&fields.0))
    })
}

fn owns_settings(meta: &ObjectMeta) -> bool {
    owned(meta, |fields| {
        ["/f:metadata/f:annotations", "/f:metadata/f:labels"]
            .into_iter()
            .filter_map(|p| fields.pointer(p).and_then(Value::as_object))
            .flat_map(serde_json::Map::keys)
            .filter_map(|key| key.strip_prefix("f:"))
            .any(|key| KEEL_ANNOTATIONS.contains(&key) || key == "keel.sh/matchTag")
    })
}
