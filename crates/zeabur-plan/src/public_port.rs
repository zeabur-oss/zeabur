//! A port of a workload open to the internet, without a domain: a `LoadBalancer` Service
//! [k8s-lb].
//!
//! - **One Service per port**, named `<service>-public-<port>`, with the selector and the port of
//!   the workload's Service. Its `port` is the external port, the Service port unless the change
//!   names another; its `targetPort` is the one of the workload's Service. On k3s and RKE2,
//!   `ServiceLB` gives it the nodes' addresses and listens on the external port on each node
//!   [k3s-servicelb]. Another load balancer controller gives it its own address. When the port is
//!   already in use on the nodes, the Service waits without an address.
//! - **It goes with the service.** It has the label `app.kubernetes.io/name`, so deleting the
//!   service with what it made deletes it too.
//! - **Closing** deletes the Service, and only one this module would have made: a `LoadBalancer`
//!   Service with that name.
//!
//! [k8s-lb]: https://kubernetes.io/docs/concepts/services-networking/service/#loadbalancer
//! [k3s-servicelb]: https://docs.k3s.io/networking/networking-services#service-load-balancer

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{Service as KubeService, ServicePort, ServiceSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use zeabur_model::kernel::dns::is_dns_label;
pub use zeabur_model::kernel::names::public_service_name;
use zeabur_model::{Health, Message};

use crate::change::{PublicPortChange, Target, Warning};
use crate::image_service::refused;
use crate::owned::SERVICE_LABEL;
use crate::plan::{ErrorKind, PlanError, Planner, Words, Working, error, to_json};

/// A refusal a client words from `summary` and `fix`.
fn refusal(
    kind: ErrorKind,
    code: &str,
    detail: String,
    summary: Message,
    fix: Message,
) -> PlanError {
    PlanError {
        words: Some(Box::new(Words { summary, fix })),
        ..error(kind, code, detail)
    }
}

pub(crate) fn service_target(name: &str) -> Target {
    Target {
        api_version: "v1".to_owned(),
        kind: "Service".to_owned(),
        plural: "services".to_owned(),
        name: name.to_owned(),
    }
}

fn is_load_balancer(object: &KubeService) -> bool {
    object
        .spec
        .as_ref()
        .and_then(|s| s.type_.as_deref())
        .is_some_and(|t| t == "LoadBalancer")
}

/// The port's protocol; Kubernetes defaults it to `TCP`.
fn protocol(port: &ServicePort) -> &str {
    port.protocol.as_deref().unwrap_or("TCP")
}

/// A copy of `port` on `external`, without its node port, which Kubernetes assigns again.
fn port_entry(port: &ServicePort, external: i32) -> ServicePort {
    ServicePort {
        port: external,
        name: port.name.clone(),
        protocol: port.protocol.clone(),
        app_protocol: port.app_protocol.clone(),
        // Without `targetPort`, Kubernetes targets `port`, which is the external port here.
        target_port: port
            .target_port
            .clone()
            .or_else(|| (external != port.port).then_some(IntOrString::Int(port.port))),
        ..ServicePort::default()
    }
}

/// The `LoadBalancer` copy of a Service port, labelled with the workload it opens.
fn public_service(
    name: &str,
    namespace: &str,
    workload: &str,
    selector: &BTreeMap<String, String>,
    port: ServicePort,
) -> KubeService {
    KubeService {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            namespace: Some(namespace.to_owned()),
            labels: Some(BTreeMap::from([(
                SERVICE_LABEL.to_owned(),
                workload.to_owned(),
            )])),
            ..ObjectMeta::default()
        },
        spec: Some(ServiceSpec {
            type_: Some("LoadBalancer".to_owned()),
            selector: Some(selector.clone()),
            ports: Some(vec![port]),
            ..ServiceSpec::default()
        }),
        ..KubeService::default()
    }
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
        if change.external.is_some_and(|p| !(1..=65535).contains(&p)) {
            return Err(refused("invalid_port", "A port is 1 to 65535.", "external"));
        }
        if existing.is_some() {
            return Err(refusal(
                ErrorKind::Conflict,
                "already_public",
                format!("A Service named {name} already exists."),
                Message::new("problem.summary.publicPort.alreadyPublic").with("name", name),
                Message::new("problem.publicPort.fix.alreadyPublic"),
            ));
        }
        if !is_dns_label(&name) {
            return Err(refusal(
                ErrorKind::BadRequest,
                "name_too_long",
                format!("{name} is longer than the 63 characters a Service name can have."),
                Message::new("problem.summary.publicPort.nameTooLong").with("name", name),
                Message::new("problem.publicPort.fix.nameTooLong"),
            ));
        }
        let unknown = || {
            refusal(
                ErrorKind::NotFound,
                "unknown_port",
                format!("Service {} has no port {}.", change.service, change.port),
                Message::new("problem.summary.publicPort.unknownPort")
                    .with("port", change.port.to_string()),
                Message::new("problem.publicPort.fix.unknownPort"),
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
            return Err(refusal(
                ErrorKind::Invalid,
                "no_selector",
                format!(
                    "Service {} selects no pods, so a copy of it would reach nothing.",
                    change.service
                ),
                Message::new("problem.summary.publicPort.noSelector")
                    .with("service", change.service.clone()),
                Message::new("problem.publicPort.fix.noSelector"),
            ));
        };
        let external = change.external.unwrap_or(change.port);
        let entry = port_entry(port, external);
        self.refuse_taken(&entry)?;
        let body = public_service(
            &name,
            &self.input.snapshot.namespace,
            &reference.name,
            selector,
            entry,
        );
        self.objects.push(Working {
            target: service_target(&name),
            original: None,
            current: to_json(&body)?,
            operations: Vec::new(),
            owners: Vec::new(),
            deleted: false,
            foreground: false,
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
            port: external,
            data_store,
            health: if data_store.is_some() {
                Health::Fail
            } else {
                Health::Warn
            },
        });
        Ok(())
    }

    /// Refuses `port` when another `LoadBalancer` Service of the project is open on it.
    fn refuse_taken(&self, port: &ServicePort) -> Result<(), PlanError> {
        let taken = self.input.snapshot.services.iter().any(|s| {
            is_load_balancer(s)
                && s.spec
                    .iter()
                    .flat_map(|spec| spec.ports.iter().flatten())
                    .any(|p| p.port == port.port && protocol(p) == protocol(port))
        });
        if !taken {
            return Ok(());
        }
        Err(refusal(
            ErrorKind::Conflict,
            "port_taken",
            format!(
                "Another Service of this project is already open on port {}.",
                port.port
            ),
            Message::new("problem.summary.publicPort.portTaken")
                .with("port", port.port.to_string()),
            Message::new("problem.publicPort.fix.portTaken"),
        ))
    }

    fn close_public_port(
        &mut self,
        change: &PublicPortChange,
        name: &str,
        existing: Option<&KubeService>,
    ) -> Result<(), PlanError> {
        let Some(object) = existing.filter(|o| is_load_balancer(o)) else {
            return Err(refusal(
                ErrorKind::NotFound,
                "not_public",
                format!(
                    "Port {} of {} is not open to the internet.",
                    change.port, change.service
                ),
                Message::new("problem.summary.publicPort.notPublic")
                    .with("port", change.port.to_string()),
                Message::new("problem.publicPort.fix.notPublic"),
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
            foreground: false,
        });
        Ok(())
    }

    pub(crate) fn kube_service(&self, name: &str) -> Option<&'a KubeService> {
        self.input
            .snapshot
            .services
            .iter()
            .find(|s| s.metadata.name.as_deref() == Some(name))
    }
}
