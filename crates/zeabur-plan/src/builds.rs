//! The planner for a service's image and its Zeabur Builder `Build` [builder]: deploying an
//! image, building again, and handing new images to Keel [keel].
//!
//! - **Deploying** sets one container's `image`, which replaces the workload's pods like any
//!   template change.
//! - **Building again** sets the `Build`'s `builder.zeabur.com/rebuild` annotation to the time of
//!   the plan; the controller builds the ref's current commit when the value is new.
//! - **Updating automatically** adds `keel.sh/policy: force` to the workload's own metadata, so
//!   Keel moves it to whatever tag it is told of, and adds a `keel` target to the `Build`'s
//!   `spec.notify`, so the controller tells Keel of each image it pushes. Turning it off removes
//!   the annotation and leaves the target: Keel ignores images no workload opts in for, and
//!   another service may still use it.
//!
//! [builder]: https://github.com/zeabur/builder
//! [keel]: https://keel.sh/docs/#policies

use serde_json::{Value, json};
use zeabur_model::{WorkloadKind, WorkloadRef};

use crate::change::Target;
use crate::checks::ResourceRef;
use crate::fields::Step;
use crate::json::{add, pointer, remove, replace, test};
use crate::plan::{ErrorKind, PlanError, Planner, error, workload_json, workload_target};
use crate::project::{set_key, shown, steps};

/// The API group of Zeabur Builder's resources.
pub const BUILDER_GROUP: &str = "builder.zeabur.com";
/// Their version.
pub const BUILDER_VERSION: &str = "v1alpha1";
/// The annotation whose new value builds a `Build` again.
pub const REBUILD: &str = "builder.zeabur.com/rebuild";
/// The workload annotation that has Keel update it, and to which tags.
pub const KEEL_POLICY: &str = "keel.sh/policy";
/// The policy Zeabur sets: any new tag of the image, since the builder's tags are not
/// versions.
pub const KEEL_FORCE: &str = "force";

/// The `builds` resource.
#[must_use]
pub fn builds() -> ResourceRef {
    ResourceRef {
        group: BUILDER_GROUP.to_owned(),
        version: BUILDER_VERSION.to_owned(),
        resource: "builds".to_owned(),
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "the planner's change kinds are split across modules by area"
)]
impl Planner<'_> {
    pub(crate) fn image(
        &mut self,
        reference: &WorkloadRef,
        container: Option<&str>,
        image: &str,
    ) -> Result<(), PlanError> {
        let image = image.trim();
        if image.is_empty() || image.chars().any(char::is_whitespace) {
            return Err(error(
                ErrorKind::BadRequest,
                "invalid_image",
                "The image reference is not valid.",
            ));
        }
        let found = self.editable_workload(reference)?;
        let target = Self::container(found, reference, container)?;
        let (index, container_pointer, mut steps) = self.container_slot(found, &target)?;
        let path = format!("{container_pointer}/image");
        let before = self.current(index).pointer(&path).cloned();
        if before.as_ref().and_then(Value::as_str) == Some(image) {
            return Err(error(
                ErrorKind::BadRequest,
                "unchanged",
                format!("Container {} already runs {image}.", target.name),
            ));
        }
        steps.push(Step::field("image"));
        self.claim(index, &steps, &format!("image of container {}", target.name))?;
        let operations = before.map_or_else(
            || vec![add(&path, json!(image))],
            |before| vec![test(&path, before), replace(&path, json!(image))],
        );
        self.patch(index, operations)
    }

    /// The working copy of `Build` `name`, read for the plan.
    fn build(&mut self, name: &str) -> Result<usize, PlanError> {
        let resource = builds();
        let Some(object) = self
            .input
            .objects
            .iter()
            .find(|f| f.resource == resource && f.name == name)
            .map(|f| f.object.clone())
        else {
            return Err(error(
                ErrorKind::NotFound,
                "unknown_build",
                format!("There is no Build named {name}."),
            ));
        };
        Ok(self.working(
            Target {
                api_version: resource.api_version(),
                kind: "Build".to_owned(),
                plural: resource.resource,
                name: name.to_owned(),
            },
            object,
        ))
    }

    pub(crate) fn rebuild(&mut self, name: &str) -> Result<(), PlanError> {
        let index = self.build(name)?;
        self.claim(
            index,
            &steps("annotations", REBUILD),
            &shown("annotations", REBUILD),
        )?;
        let current = self.current(index);
        let path = pointer(&["metadata", "annotations", REBUILD]);
        let now = self.input.now;
        let operations = current.pointer(&path).cloned().map_or_else(
            || set_key(&current, "annotations", REBUILD, now),
            |before| vec![test(&path, before), replace(&path, json!(now))],
        );
        self.patch(index, operations)
    }

    pub(crate) fn auto_update(
        &mut self,
        reference: &WorkloadRef,
        enabled: bool,
        build: Option<&str>,
        webhook: Option<&str>,
    ) -> Result<(), PlanError> {
        let found = self.workload(reference)?;
        if matches!(found.kind(), WorkloadKind::Job | WorkloadKind::CronJob) {
            return Err(error(
                ErrorKind::Invalid,
                "not_updatable",
                format!(
                    "{} {} runs to completion; Keel updates only long-running workloads.",
                    reference.kind.as_str(),
                    reference.name
                ),
            ));
        }
        Self::refuse_managed(found.meta(), reference.kind.as_str(), &reference.name)?;
        let index = self.working(workload_target(found), workload_json(found)?);
        let current = self.current(index);
        let path = pointer(&["metadata", "annotations", KEEL_POLICY]);
        let before = current.pointer(&path).cloned();
        let on = before.as_ref().and_then(Value::as_str) == Some(KEEL_FORCE);
        if on == enabled && (enabled || before.is_none()) {
            return Err(error(
                ErrorKind::BadRequest,
                "unchanged",
                format!(
                    "{} {} is already {}.",
                    reference.kind.as_str(),
                    reference.name,
                    if enabled {
                        "updated automatically"
                    } else {
                        "updated by hand"
                    }
                ),
            ));
        }
        self.claim(
            index,
            &steps("annotations", KEEL_POLICY),
            &shown("annotations", KEEL_POLICY),
        )?;
        let operations = match (before, enabled) {
            (Some(before), true) => vec![test(&path, before), replace(&path, json!(KEEL_FORCE))],
            (None, true) => set_key(&current, "annotations", KEEL_POLICY, KEEL_FORCE),
            (Some(before), false) => vec![test(&path, before), remove(&path)],
            (None, false) => Vec::new(),
        };
        self.patch(index, operations)?;
        match (enabled, build) {
            (true, Some(build)) => self.notify_keel(build, webhook),
            _ => Ok(()),
        }
    }

    /// Adds a `keel` target for `webhook` to the `Build`'s `spec.notify`, unless one is there.
    fn notify_keel(&mut self, name: &str, webhook: Option<&str>) -> Result<(), PlanError> {
        let Some(url) = webhook.map(str::trim).filter(|u| !u.is_empty()) else {
            return Err(error(
                ErrorKind::BadRequest,
                "webhook_required",
                "Name Keel's webhook to tell it of new images.",
            ));
        };
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(error(
                ErrorKind::BadRequest,
                "invalid_webhook",
                "Keel's webhook must be an http or https URL.",
            ));
        }
        let index = self.build(name)?;
        let current = self.current(index);
        let targets = current
            .pointer("/spec/notify")
            .and_then(Value::as_array)
            .cloned();
        let entry = json!({"type": "keel", "url": url});
        let known = targets.as_ref().is_some_and(|list| {
            list.iter()
                .any(|t| t["type"] == "keel" && t["url"].as_str() == Some(url))
        });
        if known {
            return Ok(());
        }
        self.claim(
            index,
            &[Step::field("spec"), Step::field("notify")],
            "spec.notify",
        )?;
        let operation = match targets {
            Some(_) => add("/spec/notify/-", entry),
            None => add("/spec/notify", json!([entry])),
        };
        self.patch(index, vec![operation])
    }
}
