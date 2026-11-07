//! The port a container listens on, moved from one number to another: what a person means by
//! "the app listens on 3000, not 8080".
//!
//! - **The container** declares `to` where it declared `from`, or adds it when it declares neither.
//!   A probe that names its port by name follows the entry.
//! - **Its probes** on `from` (`httpGet`, `tcpSocket`, `grpc`) check `to`.
//! - **`PORT`** is set to `to` where the container gets it: its own `env` when it sets `PORT`
//!   there, else the last Secret it imports (`envFrom`, no prefix) whose values hold `PORT`, as the
//!   last import wins [k8s-env]. Otherwise it goes in the container's `env`, which wins over any
//!   import. One `PORT` stays, so the variables show it once and editing it changes what the app
//!   reads. The Secret is changed first, so the pods the new template starts read the new value.
//! - **Each Service** that sends traffic to `from` on its pods (`targetPort`, or `port` without
//!   one) sends it to `to`. The Service's own `port` stays, so its address and the routes to it do
//!   not change.
//!
//! [k8s-env]: https://kubernetes.io/docs/tasks/inject-data-application/define-environment-variable-container/#using-environment-variables-inside-of-your-config

use k8s_openapi::api::core::v1::Container;
use serde_json::{Value, json};
use zeabur_model::WorkloadRef;

use crate::change::{DataEdit, DataKind, Warning};
use crate::fields::Step;
use crate::json::{add, replace, test};
use crate::plan::{ErrorKind, PlanError, Planner, error, to_json};
use crate::public_port::service_target;
use crate::rules::EnvEdit;

const PROBES: [&str; 3] = ["livenessProbe", "readinessProbe", "startupProbe"];
const HANDLERS: [&str; 3] = ["httpGet", "tcpSocket", "grpc"];

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
        let secret = self.port_secret(&target.spec);
        if let Some(secret) = &secret {
            let edit = DataEdit {
                op: "set".to_owned(),
                key: "PORT".to_owned(),
                value: Some(to.to_string()),
            };
            self.data(DataKind::Secret, secret, &[edit])?;
            // The template changes below, so its pods start again and read the new value.
            for warning in &mut self.warnings {
                if let Warning::RestartNeeded { workloads } = warning {
                    workloads.retain(|w| w != reference);
                }
            }
            self.warnings.retain(
                |w| !matches!(w, Warning::RestartNeeded { workloads } if workloads.is_empty()),
            );
        }
        let found = self.editable_workload(reference)?;
        let (index, at, steps) = self.container_slot(found, &target)?;
        self.move_declared(index, &at, &steps, &name, from, to)?;
        for probe in PROBES {
            for handler in HANDLERS {
                let field = format!("{at}/{probe}/{handler}/port");
                if self.current(index).pointer(&field) != Some(&json!(from)) {
                    continue;
                }
                let mut path = steps.clone();
                path.push(Step::field(probe));
                self.claim(index, &path, &format!("{probe} of container {name}"))?;
                self.patch(
                    index,
                    vec![test(&field, json!(from)), replace(&field, json!(to))],
                )?;
            }
        }
        if secret.is_none() {
            let port = EnvEdit {
                op: "set".to_owned(),
                name: "PORT".to_owned(),
                value: Some(to.to_string()),
                value_from: None,
            };
            self.env(reference, Some(&name), &[port])?;
        }
        self.move_target_ports(reference, from, to)
    }

    /// The last Secret `container` imports whole that holds `PORT`, unless it sets `PORT` itself.
    fn port_secret(&self, container: &Container) -> Option<String> {
        let own = container.env.iter().flatten().any(|e| e.name == "PORT");
        if own {
            return None;
        }
        for from in container.env_from.iter().flatten().rev() {
            if from.prefix.as_deref().is_some_and(|p| !p.is_empty()) {
                continue;
            }
            let Some(secret) = from.secret_ref.as_ref() else {
                continue;
            };
            let values = self.input.secret_values.get(&secret.name);
            if values.is_some_and(|v| v.contains_key("PORT")) {
                return Some(secret.name.clone());
            }
        }
        None
    }

    fn move_declared(
        &mut self,
        index: usize,
        at: &str,
        steps: &[Step],
        name: &str,
        from: i32,
        to: i32,
    ) -> Result<(), PlanError> {
        let declared = self
            .current(index)
            .pointer(&format!("{at}/ports"))
            .and_then(Value::as_array)
            .cloned();
        let mut moved = Vec::new();
        let mut has_to = false;
        for (position, entry) in declared.iter().flatten().enumerate() {
            let number = entry.get("containerPort");
            if number == Some(&json!(to)) {
                has_to = true;
            }
            if number == Some(&json!(from)) {
                moved.push(position);
            }
        }
        if has_to {
            return Ok(());
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
        self.patch(index, operations)
    }

    fn move_target_ports(
        &mut self,
        reference: &WorkloadRef,
        from: i32,
        to: i32,
    ) -> Result<(), PlanError> {
        let mut names = Vec::new();
        for service in &self.project.services {
            if service.workload.kind == reference.kind && service.workload.name == reference.name {
                names.extend(service.endpoint_names.iter().cloned());
            }
        }
        for name in names {
            let Some(service) = self.kube_service(&name) else {
                continue;
            };
            let ports = service
                .spec
                .as_ref()
                .and_then(|s| s.ports.clone())
                .unwrap_or_default();
            let mut operations = Vec::new();
            for (position, port) in ports.iter().enumerate() {
                let field = format!("/spec/ports/{position}/targetPort");
                match &port.target_port {
                    Some(target) if json!(target) == json!(from) => {
                        operations.push(test(&field, json!(from)));
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
