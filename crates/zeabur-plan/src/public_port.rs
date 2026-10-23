//! A port of a workload open to the internet, without a domain: a `LoadBalancer` Service
//! [k8s-lb].
//!
//! - **One Service per port**, named `<service>-public-<port>`, with the selector and the port of
//!   the workload's Service. On k3s and RKE2, `ServiceLB` gives it the nodes' addresses and listens
//!   on the port on each node [k3s-servicelb]. Another load balancer controller gives it its own
//!   address. When the port is already in use on the nodes, the Service waits without an address.
//! - **It goes with the service.** It has the label `app.kubernetes.io/name`, so deleting the
//!   service with what it made deletes it too.
//! - **Closing** deletes the Service, and only one this module would have made: a `LoadBalancer`
//!   Service with that name.
//!
//! [k8s-lb]: https://kubernetes.io/docs/concepts/services-networking/service/#loadbalancer
//! [k3s-servicelb]: https://docs.k3s.io/networking/networking-services#service-load-balancer

use k8s_openapi::api::core::v1::{Service as KubeService, ServicePort};
use serde_json::{Value, json};

use crate::change::{PublicPortChange, Target, Warning};
use crate::owned::SERVICE_LABEL;
use crate::plan::{ErrorKind, PlanError, Planner, Working, error, to_json};

fn service_target(name: &str) -> Target {
    Target {
        api_version: "v1".to_owned(),
        kind: "Service".to_owned(),
        plural: "services".to_owned(),
        name: name.to_owned(),
    }
}

/// The name of the `LoadBalancer` Service that opens `port` of Service `service`.
#[must_use]
pub fn public_service_name(service: &str, port: i32) -> String {
    format!("{service}-public-{port}")
}

fn is_load_balancer(object: &KubeService) -> bool {
    object
        .spec
        .as_ref()
        .and_then(|s| s.type_.as_deref())
        .is_some_and(|t| t == "LoadBalancer")
}

/// A copy of `port` without its node port, which Kubernetes assigns again.
fn port_entry(port: &ServicePort) -> Value {
    let mut entry = json!({"port": port.port});
    for (key, value) in [
        ("name", port.name.clone().map(Value::from)),
        ("protocol", port.protocol.clone().map(Value::from)),
        ("appProtocol", port.app_protocol.clone().map(Value::from)),
        ("targetPort", port.target_port.as_ref().map(|t| json!(t))),
    ] {
        if let (Some(value), Some(map)) = (value, entry.as_object_mut()) {
            map.insert(key.to_owned(), value);
        }
    }
    entry
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "the planner's change kinds are split across modules by area"
)]
impl<'a> Planner<'a> {
    pub(crate) fn public_port(&mut self, change: &PublicPortChange) -> Result<(), PlanError> {
        let reference = &change.workload;
        let found = self.workload(reference)?;
        Self::refuse_managed(found.meta(), reference.kind.as_str(), &reference.name)?;
        let name = public_service_name(&change.service, change.port);
        let existing = self.kube_service(&name);
        if change.remove {
            return self.close_public_port(change, &name, existing);
        }
        if existing.is_some() {
            return Err(error(
                ErrorKind::Conflict,
                "already_public",
                format!("A Service named {name} already exists."),
            ));
        }
        if name.len() > 63 {
            return Err(error(
                ErrorKind::BadRequest,
                "name_too_long",
                format!("{name} is longer than the 63 characters a Service name can have."),
            ));
        }
        let unknown = || {
            error(
                ErrorKind::NotFound,
                "unknown_port",
                format!("Service {} has no port {}.", change.service, change.port),
            )
        };
        let source = self.kube_service(&change.service).ok_or_else(unknown)?;
        let spec = source.spec.as_ref().ok_or_else(unknown)?;
        let port = spec
            .ports
            .iter()
            .flatten()
            .find(|p| p.port == change.port)
            .ok_or_else(unknown)?;
        let Some(selector) = spec.selector.as_ref().filter(|s| !s.is_empty()) else {
            return Err(error(
                ErrorKind::Invalid,
                "no_selector",
                format!(
                    "Service {} selects no pods, so a copy of it would reach nothing.",
                    change.service
                ),
            ));
        };
        let taken = self.input.snapshot.services.iter().any(|s| {
            is_load_balancer(s)
                && s.spec
                    .iter()
                    .flat_map(|spec| spec.ports.iter().flatten())
                    .any(|p| p.port == change.port && p.protocol == port.protocol)
        });
        if taken {
            return Err(error(
                ErrorKind::Conflict,
                "port_taken",
                format!(
                    "Another Service of this project is already open on port {}.",
                    change.port
                ),
            ));
        }
        let entry = port_entry(port);
        let body = json!({
            "apiVersion": "v1",
            "kind": "Service",
            "metadata": {
                "name": name,
                "namespace": self.input.snapshot.namespace,
                "labels": {SERVICE_LABEL: reference.name},
            },
            "spec": {"type": "LoadBalancer", "selector": selector, "ports": [entry]},
        });
        self.objects.push(Working {
            target: service_target(&name),
            original: None,
            current: body,
            operations: Vec::new(),
            owners: Vec::new(),
            deleted: false,
        });
        self.impact.insert(reference.clone());
        let data_store = self
            .project
            .services
            .iter()
            .find(|s| s.workload.kind == reference.kind && s.workload.name == reference.name)
            .and_then(|s| s.data_store);
        self.warnings.push(Warning::PublicPort {
            workload: reference.clone(),
            port: change.port,
            data_store,
        });
        Ok(())
    }

    fn close_public_port(
        &mut self,
        change: &PublicPortChange,
        name: &str,
        existing: Option<&KubeService>,
    ) -> Result<(), PlanError> {
        let Some(object) = existing.filter(|o| is_load_balancer(o)) else {
            return Err(error(
                ErrorKind::NotFound,
                "not_public",
                format!(
                    "Port {} of {} is not open to the internet.",
                    change.port, change.service
                ),
            ));
        };
        let object = to_json(object)?;
        self.objects.push(Working {
            target: service_target(name),
            original: Some(object.clone()),
            current: object,
            operations: Vec::new(),
            owners: Vec::new(),
            deleted: true,
        });
        Ok(())
    }

    fn kube_service(&self, name: &str) -> Option<&'a KubeService> {
        self.input
            .snapshot
            .services
            .iter()
            .find(|s| s.metadata.name.as_deref() == Some(name))
    }
}
