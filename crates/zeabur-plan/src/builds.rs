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
//!   the annotation and leaves the target: Keel ignores images no workload opts in for, and another
//!   service may still use it.
//! - **Build options** set the `Build`'s directory and Dockerfile, and edit `spec.env`. Zeabur
//!   Builder passes each variable to the build as a `BuildKit` secret, never as a build argument,
//!   so a value does not stay in the image. A value from a Secret names the Secret and the key.
//!
//! [builder]: https://github.com/zeabur-oss/builder
//! [keel]: https://keel.sh/docs/#policies

use serde_json::{Value, json};
use zeabur_model::{WorkloadKind, WorkloadRef};

use crate::change::{BuildOptionsChange, Target};
use crate::checks::ResourceRef;
use crate::fields::Step;
use crate::json::{add, pointer, remove, replace, test};
use crate::plan::{ErrorKind, PlanError, Planner, error, workload_json, workload_target};
use crate::project::{set_key, shown, steps};
use crate::rules::{EnvEdit, env_entry, validate_env};

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

/// Most build variables a `Build` takes.
const MAX_BUILD_ENV: usize = 128;
/// Longest path the `Build` takes for its directory and Dockerfile.
const MAX_BUILD_PATH: usize = 1024;

/// Whether `path` is a relative path that the `Build` accepts: no segment starts with `.`, so
/// the path cannot climb out with `..`.
fn build_path(path: &str) -> bool {
    path.len() <= MAX_BUILD_PATH
        && path.split('/').all(|segment| {
            segment
                .bytes()
                .next()
                .is_some_and(|first| first != b'.' && path_byte(first))
                && segment.bytes().all(path_byte)
        })
}

const fn path_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'@' | b'+' | b'-')
}

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
        self.claim(
            index,
            &steps,
            &format!("image of container {}", target.name),
        )?;
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

    pub(crate) fn build_options(&mut self, change: &BuildOptionsChange) -> Result<(), PlanError> {
        if change.context_dir.is_none() && change.dockerfile.is_none() && change.env.is_empty() {
            return Err(error(
                ErrorKind::BadRequest,
                "no_changes",
                "Nothing to change.",
            ));
        }
        for path in [&change.context_dir, &change.dockerfile]
            .into_iter()
            .flatten()
        {
            let path = path.trim();
            if !path.is_empty() && !build_path(path) {
                return Err(error(
                    ErrorKind::BadRequest,
                    "invalid_path",
                    format!("{path} is not a relative path inside the repository."),
                ));
            }
        }
        if !change.env.is_empty() {
            validate_env(&change.env)
                .map_err(|r| error(ErrorKind::BadRequest, &r.0, "The variables are not valid."))?;
        }
        if change.env.iter().any(|e| {
            e.value_from.as_ref().is_some_and(|from| {
                from.secret_name.is_none()
                    || from.config_map_name.is_some()
                    || from.field_path.is_some()
            })
        }) {
            return Err(error(
                ErrorKind::BadRequest,
                "unsupported_env_source",
                "A build variable reads its value only from a Secret key.",
            ));
        }
        let index = self.build(&change.build)?;
        let before = self.current(index);
        for (field, value) in [
            ("contextDir", &change.context_dir),
            ("dockerfile", &change.dockerfile),
        ] {
            if let Some(value) = value {
                self.build_path_field(index, field, value.trim())?;
            }
        }
        for edit in &change.env {
            self.build_env(index, &change.build, edit)?;
        }
        let after = self.current(index);
        let count = after
            .pointer("/spec/env")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        if count > MAX_BUILD_ENV {
            return Err(error(
                ErrorKind::BadRequest,
                "too_many_variables",
                format!("A Build takes at most {MAX_BUILD_ENV} build variables."),
            ));
        }
        if after == before {
            return Err(error(
                ErrorKind::BadRequest,
                "unchanged",
                format!("Build {} already builds this way.", change.build),
            ));
        }
        Ok(())
    }

    /// Sets `spec.source.git.<field>` to `value`, or removes it when `value` is empty.
    fn build_path_field(
        &mut self,
        index: usize,
        field: &str,
        value: &str,
    ) -> Result<(), PlanError> {
        let path = pointer(&["spec", "source", "git", field]);
        let before = self.current(index).pointer(&path).cloned();
        let operations = match before {
            Some(before) if before.as_str() == Some(value) => return Ok(()),
            Some(before) if value.is_empty() => vec![test(&path, before), remove(&path)],
            Some(before) => vec![test(&path, before), replace(&path, json!(value))],
            None if value.is_empty() => return Ok(()),
            None => vec![add(&path, json!(value))],
        };
        self.claim(
            index,
            &[
                Step::field("spec"),
                Step::field("source"),
                Step::field("git"),
                Step::field(field),
            ],
            &format!("spec.source.git.{field}"),
        )?;
        self.patch(index, operations)
    }

    fn build_env(&mut self, index: usize, build: &str, edit: &EnvEdit) -> Result<(), PlanError> {
        let env = self
            .current(index)
            .pointer("/spec/env")
            .and_then(Value::as_array)
            .cloned();
        let existing = env.as_ref().and_then(|env| {
            env.iter()
                .position(|e| e.get("name").and_then(Value::as_str) == Some(edit.name.as_str()))
        });
        let path = [
            Step::field("spec"),
            Step::field("env"),
            Step::named(&edit.name),
        ];
        let shown = format!("build variable {}", edit.name);
        let operations = match (edit.op.as_str(), existing) {
            ("set", Some(at)) => {
                let entry = format!("/spec/env/{at}");
                vec![
                    test(&format!("{entry}/name"), json!(edit.name)),
                    replace(&entry, env_entry(edit)),
                ]
            }
            ("set", None) if env.is_some() => vec![add("/spec/env/-", env_entry(edit))],
            ("set", None) => vec![add("/spec/env", json!([env_entry(edit)]))],
            (_, Some(at)) => {
                let entry = format!("/spec/env/{at}");
                vec![
                    test(&format!("{entry}/name"), json!(edit.name)),
                    remove(&entry),
                ]
            }
            (_, None) => {
                return Err(error(
                    ErrorKind::NotFound,
                    "unknown_env_var",
                    format!("Build {build} has no variable {}.", edit.name),
                ));
            }
        };
        if existing.is_some() {
            self.claim(index, &path, &shown)?;
        }
        self.patch(index, operations)
    }
}
