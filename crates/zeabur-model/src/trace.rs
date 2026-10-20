//! The way traffic travels to a service, hop by hop, and where it stops.

use serde::{Deserialize, Serialize};

use crate::diagnosis::Message;
use crate::source::Origin;

/// What kind of address a trace follows.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    utoipa::ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum AddressKind {
    /// A public domain, routed by an Ingress or an `HTTPRoute`.
    Domain,
    /// The private address other services of the cluster use: a Service's DNS name.
    Private,
}

/// One hop on the way to a service, in the order traffic takes them.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    utoipa::ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum HopStep {
    /// The domain resolves to an address, looked up from where Zeabur runs.
    Dns,
    /// That address is the ingress controller's (the route's `status.loadBalancer`).
    Address,
    /// The ingress controller of the route's class has a ready replica.
    Controller,
    /// The certificate for the domain is issued (cert-manager's `Certificate`), or its Secret is
    /// there.
    Certificate,
    /// The route is taken up: an Ingress has an address in its status, an `HTTPRoute` is accepted
    /// by its Gateway.
    Route,
    /// The Service the route sends to exists, has the port the route names, and selects pods.
    Service,
    /// The Service's `targetPort` is a port of the containers.
    TargetPort,
    /// At least one of the pods is ready, so the Service sends to it.
    Ready,
}

/// How a hop fared.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    utoipa::ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum HopStatus {
    /// It passes.
    Ok,
    /// Traffic stops here.
    Failed,
    /// What Zeabur can read does not tell.
    Unknown,
}

/// One hop and what was seen there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Hop {
    /// Which hop.
    pub step: HopStep,
    /// How it fared.
    pub status: HopStatus,
    /// The value it should have, as Kubernetes or DNS writes it, when one is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    /// The value seen, the same way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saw: Option<String>,
    /// What was seen, in one sentence.
    pub message: Message,
    /// The objects and fields it rests on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<Origin>,
}

/// The way to one address of a service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RouteTrace {
    /// A domain or the private address.
    pub kind: AddressKind,
    /// The domain (`*` for a rule without a host) or the Service's DNS name, with its port.
    pub address: String,
    /// The object that makes the address: the Ingress, `HTTPRoute` or Service.
    pub via: Origin,
    /// The hops, in the order traffic takes them.
    pub hops: Vec<Hop>,
    /// `failed` when a hop failed, `unknown` when none failed and one is unknown, else `ok`.
    pub status: HopStatus,
}

impl RouteTrace {
    /// The first hop that failed.
    #[must_use]
    pub fn first_failure(&self) -> Option<&Hop> {
        self.hops.iter().find(|h| h.status == HopStatus::Failed)
    }
}
