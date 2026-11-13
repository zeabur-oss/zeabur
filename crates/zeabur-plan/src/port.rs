//! The port a container listens on, moved from one number to another: what a person means by
//! "the app listens on 3000, not 8080".
//!
//! - **The container** declares `to` where it declared `from`, or adds it when it declares neither.
//!   When it declares `to` already, its entries are left as they are.
//! - **Its probes** (`httpGet`, `tcpSocket`, `grpc`) that check `from`, by number or by the name of
//!   an entry that declares it, check `to`.
//! - **`PORT`** is set to `to` in the container's own `env`, which wins over every import
//!   [k8s-env], so no import has to be found or changed.
//! - **Each Service** that sends traffic to `from` on its pods (`targetPort`, or `port` without
//!   one), or by the name of such an entry, sends it to `to`. The Service's own `port` stays, so
//!   its address and the routes to it do not change.
//!
//! [k8s-env]: https://kubernetes.io/docs/tasks/inject-data-application/define-environment-variable-container/#using-environment-variables-inside-of-your-config

use k8s_openapi::api::core::v1::{Container, Probe, Service as KubeService, ServicePort};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use serde_json::json;
use zeabur_model::WorkloadRef;

use crate::Target;
use crate::fields::Step;
use crate::json::{add, replace, test};
use crate::plan::{ErrorKind, PlanError, Planner, error, to_json};
use crate::public_port::service_target;
use crate::rules::EnvEdit;

/// Whether a port a probe or Service names is the one being moved: its number, or the name of an
/// entry that declares it.
fn is_moved(port: &IntOrString, from: i32, names: &[String]) -> bool {
    match port {
        IntOrString::Int(number) => *number == from,
        IntOrString::String(name) => names.contains(name),
    }
}

/// The probes of `container` with where each checks its port: the probe's field, the handler's
/// field and the port.
fn probe_ports(container: &Container) -> Vec<(&'static str, &'static str, IntOrString)> {
    let probes: [(&str, &Option<Probe>); 3] = [
        ("livenessProbe", &container.liveness_probe),
        ("readinessProbe", &container.readiness_probe),
        ("startupProbe", &container.startup_probe),
    ];
    let mut ports = Vec::new();
    for (field, probe) in probes {
        let Some(probe) = probe else { continue };
        if let Some(http) = &probe.http_get {
            ports.push((field, "httpGet", http.port.clone()));
        }
        if let Some(tcp) = &probe.tcp_socket {
            ports.push((field, "tcpSocket", tcp.port.clone()));
        }
        if let Some(grpc) = &probe.grpc {
            ports.push((field, "grpc", IntOrString::Int(grpc.port)));
        }
    }
    ports
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "the planner's change kinds are split across modules by area"
)]
impl Planner<'_> {
    pub(crate) fn port(
        &mut self,
        reference: &WorkloadRef,
        container: Option<&str>,
        from: i32,
        to: i32,
    ) -> Result<(), PlanError> {
        if !(1..=65535).contains(&to) || from == to {
            return Err(error(
                ErrorKind::BadRequest,
                "invalid_port",
                format!("{to} is not a port to move {from} to."),
            ));
        }
        let found = self.editable_workload(reference)?;
        let target = Self::container(found, reference, container)?;
        let name = target.name.clone();
        let (index, at, steps) = self.container_slot(found, &target)?;
        let names = self.move_declared(index, &at, &steps, &name, from, to)?;
        let current = self.container_now(index, &at)?;
        for (probe, handler, port) in probe_ports(&current) {
            if !is_moved(&port, from, &names) {
                continue;
            }
            let field = format!("{at}/{probe}/{handler}/port");
            let mut path = steps.clone();
            path.push(Step::field(probe));
            self.claim(index, &path, &format!("{probe} of container {name}"))?;
            self.patch(
                index,
                vec![test(&field, json!(port)), replace(&field, json!(to))],
            )?;
        }
        let port = EnvEdit {
            op: "set".to_owned(),
            name: "PORT".to_owned(),
            value: Some(to.to_string()),
            value_from: None,
        };
        self.env(reference, Some(&name), &[port])?;
        self.move_target_ports(reference, from, to, &names)
    }

    /// The container at `at` in object `index` as the earlier changes of this request left it.
    fn container_now(&self, index: usize, at: &str) -> Result<Container, PlanError> {
        let value = self.current(index).pointer(at).cloned().unwrap_or_default();
        serde_json::from_value(value).map_err(|e| {
            error(
                ErrorKind::BadRequest,
                "invalid_container",
                format!("The container is not valid: {e}."),
            )
        })
    }

    /// Declares `to` where the container declared `from`, or adds it; nothing changes when it
    /// declares `to` already. Returns the names of the entries that declared `from`, which a probe
    /// or Service may use instead of the number.
    fn move_declared(
        &mut self,
        index: usize,
        at: &str,
        steps: &[Step],
        name: &str,
        from: i32,
        to: i32,
    ) -> Result<Vec<String>, PlanError> {
        let declared = self.container_now(index, at)?.ports;
        let entries = declared.iter().flatten();
        let moved: Vec<usize> = entries
            .clone()
            .enumerate()
            .filter(|(_, entry)| entry.container_port == from)
            .map(|(position, _)| position)
            .collect();
        let names = entries
            .clone()
            .filter(|entry| entry.container_port == from)
            .filter_map(|entry| entry.name.clone())
            .collect();
        if entries.clone().any(|entry| entry.container_port == to) {
            return Ok(names);
        }
        let mut path = steps.to_vec();
        path.push(Step::field("ports"));
        self.claim(index, &path, &format!("ports of container {name}"))?;
        let mut operations = Vec::new();
        for position in &moved {
            let field = format!("{at}/ports/{position}/containerPort");
            operations.push(test(&field, json!(from)));
            operations.push(replace(&field, json!(to)));
        }
        if moved.is_empty() {
            operations.push(match declared {
                Some(_) => add(&format!("{at}/ports/-"), json!({"containerPort": to})),
                None => add(&format!("{at}/ports"), json!([{"containerPort": to}])),
            });
        }
        self.patch(index, operations)?;
        Ok(names)
    }

    /// The ports of `service` as the earlier changes of this request left them, so that a second
    /// port change moves the targets the first one set.
    fn service_ports(&self, service: &KubeService) -> Vec<ServicePort> {
        let name = service.metadata.name.as_deref().unwrap_or_default();
        let target: Target = service_target(name);
        let planned = self.objects.iter().find(|w| w.target == target);
        let planned = planned.and_then(|w| w.current.pointer("/spec/ports").cloned());
        planned
            .and_then(|ports| serde_json::from_value(ports).ok())
            .or_else(|| service.spec.as_ref()?.ports.clone())
            .unwrap_or_default()
    }

    fn move_target_ports(
        &mut self,
        reference: &WorkloadRef,
        from: i32,
        to: i32,
        port_names: &[String],
    ) -> Result<(), PlanError> {
        let names: Vec<String> = self
            .project
            .services
            .iter()
            .filter(|s| s.workload.kind == reference.kind && s.workload.name == reference.name)
            .flat_map(|s| s.endpoint_names.iter().cloned())
            .collect();
        for name in names {
            let Some(service) = self.kube_service(&name) else {
                continue;
            };
            let mut operations = Vec::new();
            for (position, port) in self.service_ports(service).iter().enumerate() {
                let field = format!("/spec/ports/{position}/targetPort");
                match &port.target_port {
                    Some(target) if is_moved(target, from, port_names) => {
                        operations.push(test(&field, json!(target)));
                        operations.push(replace(&field, json!(to)));
                    }
                    None if port.port == from => operations.push(add(&field, json!(to))),
                    _ => {}
                }
            }
            if operations.is_empty() {
                continue;
            }
            Self::refuse_managed(&service.metadata, "Service", &name)?;
            let object = to_json(service)?;
            let index = self.working(service_target(&name), object);
            self.claim(
                index,
                &[Step::field("spec"), Step::field("ports")],
                &format!("ports of Service {name}"),
            )?;
            self.patch(index, operations)?;
        }
        Ok(())
    }
}
