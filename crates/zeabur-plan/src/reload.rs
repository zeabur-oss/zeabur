//! The planner for a workload's Reloader setting: whether Stakater Reloader replaces its pods
//! when a ConfigMap or Secret it reads changes [reloader].
//!
//! Turning it on adds the annotation `reloader.stakater.com/auto: "true"` to the workload's own
//! metadata, not its pod template, so the change itself restarts nothing; turning it off removes
//! that annotation. The annotations that name ConfigMaps or Secrets one by one
//! (`configmap.reloader.stakater.com/reload`, …) are left as they are. Reloader replaces the pods
//! of Deployments, StatefulSets and DaemonSets; Jobs and CronJobs are refused, as the catalog's
//! values have Reloader ignore them.
//!
//! [reloader]: https://github.com/stakater/Reloader#how-to-use-reloader

use serde_json::{Value, json};
use zeabur_model::{ConfigReload, WorkloadKind, WorkloadRef};

use crate::json::{pointer, remove, replace, test};
use crate::plan::{ErrorKind, PlanError, Planner, error, workload_json, workload_target};
use crate::project::{set_key, shown, steps};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "the planner's change kinds are split across modules by area"
)]
impl Planner<'_> {
    pub(crate) fn config_reload(
        &mut self,
        reference: &WorkloadRef,
        enabled: bool,
    ) -> Result<(), PlanError> {
        let found = self.workload(reference)?;
        match found.kind() {
            WorkloadKind::Deployment | WorkloadKind::StatefulSet | WorkloadKind::DaemonSet => {}
            WorkloadKind::Job | WorkloadKind::CronJob => {
                return Err(error(
                    ErrorKind::Invalid,
                    "not_reloadable",
                    format!(
                        "{} {} runs to completion; Reloader restarts only long-running workloads.",
                        reference.kind.as_str(),
                        reference.name
                    ),
                ));
            }
        }
        Self::refuse_managed(found.meta(), reference.kind.as_str(), &reference.name)?;
        let index = self.working(workload_target(found), workload_json(found)?);
        let current = self.current(index);
        let path = pointer(&["metadata", "annotations", ConfigReload::AUTO]);
        let before = current.pointer(&path).cloned();
        let on = before
            .as_ref()
            .and_then(Value::as_str)
            .is_some_and(|v| v.trim().eq_ignore_ascii_case("true"));
        if on == enabled && (enabled || before.is_none()) {
            return Err(error(
                ErrorKind::BadRequest,
                "unchanged",
                format!(
                    "{} {} is already {}.",
                    reference.kind.as_str(),
                    reference.name,
                    if enabled {
                        "restarted when its configuration changes"
                    } else {
                        "left running when its configuration changes"
                    }
                ),
            ));
        }
        self.claim(
            index,
            &steps("annotations", ConfigReload::AUTO),
            &shown("annotations", ConfigReload::AUTO),
        )?;
        self.impact.insert(reference.clone());
        let operations = match (before, enabled) {
            (Some(before), true) => {
                vec![test(&path, before), replace(&path, json!("true"))]
            }
            (None, true) => set_key(&current, "annotations", ConfigReload::AUTO, "true"),
            (Some(before), false) => vec![test(&path, before), remove(&path)],
            (None, false) => Vec::new(),
        };
        self.patch(index, operations)
    }
}
