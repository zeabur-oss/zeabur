//! Variables a container reads from a ConfigMap or Secret that several services share.
//!
//! A single key is an `env[].valueFrom` entry, which [`Change::Env`] already writes. Every key is
//! an `envFrom` entry [env-from], which this module adds or removes. `envFrom` is an atomic list
//! for server-side apply, so the whole list is claimed. Kubernetes reads the keys when a container
//! starts, so the pods are replaced.
//!
//! [`Change::Env`]: crate::Change::Env
//! [env-from]: https://kubernetes.io/docs/tasks/inject-data-application/distribute-credentials-secure/#configure-all-key-value-pairs-in-a-secret-as-container-environment-variables

use serde_json::{Value, json};

use crate::change::{DataKind, EnvImportChange};
use crate::fields::Step;
use crate::json::{add, pointer, remove, test};
use crate::plan::{
    ErrorKind, PlanError, Planner, container_at, error, pod_spec_steps, workload_json,
    workload_target,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "the planner's change kinds are split across modules by area"
)]
impl Planner<'_> {
    pub(crate) fn env_import(&mut self, change: &EnvImportChange) -> Result<(), PlanError> {
        let reference = &change.workload;
        let found = self.workload(reference)?;
        Self::refuse_managed(found.meta(), reference.kind.as_str(), &reference.name)?;
        let (container, position) = container_at(found, reference, change.container.as_deref())?;
        let (field, kind) = match change.kind {
            DataKind::ConfigMap => ("configMapRef", "ConfigMap"),
            DataKind::Secret => ("secretRef", "Secret"),
        };
        if !change.remove && !self.data_exists(change.kind, &change.name) {
            return Err(error(
                ErrorKind::NotFound,
                "unknown_object",
                format!("There is no {kind} named {}.", change.name),
            ));
        }
        let index = self.working(workload_target(found), workload_json(found)?);
        self.impact.insert(reference.clone());
        let mut base: Vec<&str> = reference.kind.pod_template_path().to_vec();
        let position_text = position.to_string();
        base.extend(["spec", "containers", &position_text, "envFrom"]);
        let list = pointer(&base);
        let mut steps = pod_spec_steps(reference.kind);
        steps.extend([
            Step::field("containers"),
            Step::named(&container),
            Step::field("envFrom"),
        ]);
        let shown = format!("envFrom of container {container}");
        let current = self.current(index).pointer(&list).cloned();
        let entries = current.as_ref().and_then(Value::as_array);
        let at = entries.and_then(|e| {
            e.iter().position(|entry| {
                entry
                    .pointer(&format!("/{field}/name"))
                    .and_then(Value::as_str)
                    == Some(change.name.as_str())
            })
        });
        let operations = match (change.remove, at, entries.is_some()) {
            (false, Some(_), _) => {
                return Err(error(
                    ErrorKind::Conflict,
                    "already_imported",
                    format!(
                        "Container {container} already reads every key of {kind} {}.",
                        change.name
                    ),
                ));
            }
            (false, None, true) => {
                vec![add(
                    &format!("{list}/-"),
                    json!({field: {"name": change.name}}),
                )]
            }
            (false, None, false) => vec![add(&list, json!([{field: {"name": change.name}}]))],
            (true, Some(at), _) => vec![
                test(&format!("{list}/{at}/{field}/name"), json!(change.name)),
                remove(&format!("{list}/{at}")),
            ],
            (true, None, _) => {
                return Err(error(
                    ErrorKind::NotFound,
                    "not_imported",
                    format!(
                        "Container {container} does not read every key of {kind} {}.",
                        change.name
                    ),
                ));
            }
        };
        if entries.is_some() {
            self.claim(index, &steps, &shown)?;
        }
        self.patch(index, operations)
    }

    fn data_exists(&self, kind: DataKind, name: &str) -> bool {
        let named = |meta: &k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta| {
            meta.name.as_deref() == Some(name)
        };
        match kind {
            DataKind::ConfigMap => self
                .input
                .snapshot
                .config_maps
                .iter()
                .any(|c| named(&c.metadata)),
            DataKind::Secret => self
                .input
                .snapshot
                .secrets
                .iter()
                .any(|s| named(&s.metadata)),
        }
    }
}
