//! Events as the diagnoses read them.

use engawa_model::Origin;
use k8s_openapi::api::events::v1::Event as KubeEvent;
use k8s_openapi::jiff::Timestamp;

/// One Kubernetes event, reduced to what a diagnosis needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// The Event object.
    pub origin: Origin,
    /// The object it is about; `field_path` names the container, as `spec.containers{web}`.
    pub regarding: Option<Origin>,
    /// `Normal` or `Warning`.
    pub kind: String,
    /// The machine-readable reason, such as `Unhealthy` or `Failed`.
    pub reason: String,
    /// What happened, as the component reported it.
    pub note: String,
    /// How many times it happened.
    pub count: i32,
    /// When it last happened.
    pub last_seen: Option<Timestamp>,
}

impl Event {
    /// The event as the diagnoses read it. Events written through the core/v1 API keep their
    /// count and times in the deprecated fields.
    #[must_use]
    pub fn from_kube(event: &KubeEvent) -> Self {
        let name = event.metadata.name.as_deref().unwrap_or_default();
        Self {
            origin: Origin::object("Event", name),
            regarding: event.regarding.as_ref().and_then(|r| {
                Some(Origin {
                    field_path: r.field_path.clone(),
                    ..Origin::object(r.kind.as_deref()?, r.name.as_deref()?)
                })
            }),
            kind: event.type_.clone().unwrap_or_default(),
            reason: event.reason.clone().unwrap_or_default(),
            note: event.note.clone().unwrap_or_default(),
            count: event
                .series
                .as_ref()
                .map(|s| s.count)
                .or(event.deprecated_count)
                .unwrap_or(1),
            last_seen: event
                .series
                .as_ref()
                .map(|s| s.last_observed_time.0)
                .or_else(|| event.deprecated_last_timestamp.as_ref().map(|t| t.0))
                .or_else(|| event.event_time.as_ref().map(|t| t.0)),
        }
    }

    /// The container an event names in its `fieldPath`, written `spec.containers{name}` (or
    /// `spec.initContainers{name}`) by the kubelet.
    pub(crate) fn container(&self) -> Option<&str> {
        let path = self.regarding.as_ref()?.field_path.as_deref()?;
        let (_, rest) = path.split_once('{')?;
        rest.strip_suffix('}')
    }
}
