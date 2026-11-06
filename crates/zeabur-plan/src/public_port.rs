//! A port of a workload or a PostgreSQL database open to the internet, without a domain: a
//! `LoadBalancer` Service [k8s-lb].
//!
//! - **One Service per port**, named `<service>-public-<port>`, with the selector and the port of
//!   the workload's Service. On k3s and RKE2, `ServiceLB` gives it the nodes' addresses and listens
//!   on the port on each node [k3s-servicelb]. Another load balancer controller gives it its own
//!   address. When the port is already in use on the nodes, the Service waits without an address.
//! - **It goes with the service.** It has the label `app.kubernetes.io/name`, so deleting the
//!   service with what it made deletes it too.
//! - **A PostgreSQL database** (a CloudNativePG `Cluster`) gets `<database>-public-5432`, a copy of
//!   its read-write Service. CloudNativePG serves TLS, so only PostgreSQL is offered. The `Cluster`
//!   owns the Service (`ownerReferences`), so the garbage collector deletes it with the database
//!   [k8s-gc].
//! - **Closing** deletes the Service, and only one this module would have made: a `LoadBalancer`
//!   Service with that name.
//!
//! [k8s-lb]: https://kubernetes.io/docs/concepts/services-networking/service/#loadbalancer
//! [k3s-servicelb]: https://docs.k3s.io/networking/networking-services#service-load-balancer
//! [k8s-gc]: https://kubernetes.io/docs/concepts/architecture/garbage-collection/

use k8s_openapi::api::core::v1::{Service as KubeService, ServicePort};
use serde_json::{Value, json};
use zeabur_model::Message;

use crate::change::{DatabasePublicPortChange, PublicPortChange, Target, Warning};
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

/// The port CloudNativePG serves PostgreSQL on.
const POSTGRES_PORT: i32 = 5432;
const CNPG_GROUP: &str = "postgresql.cnpg.io";

/// The name of the `LoadBalancer` Service that opens PostgreSQL database `database`.
#[must_use]
pub fn database_public_service_name(database: &str) -> String {
    public_service_name(database, POSTGRES_PORT)
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
        if change.remove {
            return self.close_public_port(&change.service, change.port, &name);
        }
        self.open_public_port(
            &change.service,
            change.port,
            &name,
            json!({"labels": {SERVICE_LABEL: reference.name}}),
        )?;
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

    pub(crate) fn database_public_port(
        &mut self,
        change: &DatabasePublicPortChange,
    ) -> Result<(), PlanError> {
        let name = database_public_service_name(&change.database);
        if change.remove {
            return self.close_public_port(&change.database, POSTGRES_PORT, &name);
        }
        let cluster = self
            .input
            .objects
            .iter()
            .find(|f| {
                f.resource.group == CNPG_GROUP
                    && f.resource.resource == "clusters"
                    && f.name == change.database
            })
            .map(|f| &f.object)
            .ok_or_else(|| {
                refusal(
                    ErrorKind::NotFound,
                    "unknown_database",
                    format!("There is no PostgreSQL database {}.", change.database),
                    Message::new("problem.summary.publicPort.unknownDatabase")
                        .with("name", change.database.clone()),
                    Message::new("problem.publicPort.fix.unknownDatabase"),
                )
            })?;
        let text = |pointer: &str| cluster.pointer(pointer).and_then(Value::as_str);
        let service = text("/status/writeService")
            .map_or_else(|| format!("{}-rw", change.database), str::to_owned);
        let owner = json!({
            "apiVersion": text("/apiVersion").unwrap_or("postgresql.cnpg.io/v1"),
            "kind": "Cluster",
            "name": change.database,
            "uid": text("/metadata/uid").unwrap_or_default(),
        });
        self.open_public_port(
            &service,
            POSTGRES_PORT,
            &name,
            json!({"ownerReferences": [owner]}),
        )?;
        self.warnings.push(Warning::DatabasePublicPort {
            database: change.database.clone(),
            port: POSTGRES_PORT,
        });
        Ok(())
    }

    /// Plans a `LoadBalancer` Service `name` with the selector and port `port` of Service
    /// `service`, and with `metadata` added to its own.
    fn open_public_port(
        &mut self,
        service: &str,
        port: i32,
        name: &str,
        metadata: Value,
    ) -> Result<(), PlanError> {
        if self.kube_service(name).is_some() {
            return Err(refusal(
                ErrorKind::Conflict,
                "already_public",
                format!("A Service named {name} already exists."),
                Message::new("problem.summary.publicPort.alreadyPublic").with("name", name),
                Message::new("problem.publicPort.fix.alreadyPublic"),
            ));
        }
        if name.len() > 63 {
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
                format!("Service {service} has no port {port}."),
                Message::new("problem.summary.publicPort.unknownPort")
                    .with("port", port.to_string()),
                Message::new("problem.publicPort.fix.unknownPort"),
            )
        };
        let source = self.kube_service(service).ok_or_else(unknown)?;
        let spec = source.spec.as_ref().ok_or_else(unknown)?;
        let entry = spec
            .ports
            .iter()
            .flatten()
            .find(|p| p.port == port)
            .ok_or_else(unknown)?;
        let Some(selector) = spec.selector.as_ref().filter(|s| !s.is_empty()) else {
            return Err(refusal(
                ErrorKind::Invalid,
                "no_selector",
                format!("Service {service} selects no pods, so a copy of it would reach nothing."),
                Message::new("problem.summary.publicPort.noSelector").with("service", service),
                Message::new("problem.publicPort.fix.noSelector"),
            ));
        };
        self.refuse_taken(entry)?;
        let mut body = json!({
            "apiVersion": "v1",
            "kind": "Service",
            "metadata": {"name": name, "namespace": self.input.snapshot.namespace},
            "spec": {"type": "LoadBalancer", "selector": selector, "ports": [port_entry(entry)]},
        });
        if let (Some(target), Value::Object(extra)) = (
            body.get_mut("metadata").and_then(Value::as_object_mut),
            metadata,
        ) {
            target.extend(extra);
        }
        self.objects.push(Working {
            target: service_target(name),
            original: None,
            current: body,
            operations: Vec::new(),
            owners: Vec::new(),
            deleted: false,
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
                    .any(|p| p.port == port.port && p.protocol == port.protocol)
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

    fn close_public_port(&mut self, what: &str, port: i32, name: &str) -> Result<(), PlanError> {
        let Some(object) = self.kube_service(name).filter(|o| is_load_balancer(o)) else {
            return Err(refusal(
                ErrorKind::NotFound,
                "not_public",
                format!("Port {port} of {what} is not open to the internet."),
                Message::new("problem.summary.publicPort.notPublic").with("port", port.to_string()),
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
