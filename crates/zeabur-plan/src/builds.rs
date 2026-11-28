//! The planner for a service's image and its Zeabur Builder `Build` [builder]: deploying an
//! image, building again, and handing new images to Keel [keel].
//!
//! - **Deploying** sets one container's `image`, which replaces the workload's pods like any
//!   template change.
//! - **Building again** sets the `Build`'s `builder.zeabur.com/rebuild` annotation to the time of
//!   the plan; the controller builds the ref's current commit when the value is new.
//! - **Uploading a folder** to a `Build` with an `archive` source takes two plans. Asking for an
//!   upload token sets the `builder.zeabur.com/upload-token-request` annotation to a value the
//!   client chooses; the controller answers in `status.uploadToken` and writes the token to a
//!   Secret. After the client has pushed the folder, naming its digest sets
//!   `spec.source.archive.digest`, which builds it.
//! - **Updating automatically** writes Keel's annotations on the workload's own metadata. For new
//!   builds, `keel.sh/policy: force`, so Keel moves it to whatever tag it is told of, and a `keel`
//!   target in the `Build`'s `spec.notify`, so the controller tells Keel of each image it pushes.
//!   For an image from a registry, a version policy (`patch`, `minor`, `major`, or `force` with
//!   `keel.sh/match-tag` for a new image under the same tag), `keel.sh/trigger: poll` and a
//!   `keel.sh/pollSchedule`, hourly or once a day or week. Turning it off removes the annotations
//!   and leaves the target: Keel ignores images no workload opts in for, and another service may
//!   still use it.
//! - **Build options** set the `Build`'s directory and Dockerfile, and edit `spec.env`. Zeabur
//!   Builder passes each variable to the build as a `BuildKit` secret, never as a build argument,
//!   so a value does not stay in the image. A value from a Secret names the Secret and the key.
//!
//! [builder]: https://github.com/zeabur-oss/builder
//! [keel]: https://keel.sh/docs/#policies

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde_json::{Value, json};
use zeabur_model::{
    ImagePolicy, KEEL_ANNOTATIONS, KEEL_POLL, UpdateWindow, WorkloadKind, WorkloadRef,
};
pub use zeabur_model::{KEEL_FORCE, KEEL_POLICY};

use crate::change::{AutoUpdateTarget, BuildOptionsChange, Target, Update};
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
/// The annotation whose value Zeabur Builder links the commit status of each build to
/// ([builder#273]): Zeabur sets it to the service's Builds tab.
///
/// [builder#273]: https://github.com/zeabur-oss/builder/pull/273
pub const DETAILS_URL: &str = "builder.zeabur.com/details-url";
/// The annotation whose new value asks for a token to upload an archive.
pub const UPLOAD_TOKEN_REQUEST: &str = "builder.zeabur.com/upload-token-request";
/// The label Zeabur Builder reads credentials by.
pub const GIT_CREDENTIALS_LABEL: &str = "builder.zeabur.com/git-credentials";

/// Longest upload token request the controller answers.
const MAX_UPLOAD_REQUEST: usize = 63;

/// Whether `digest` is a SHA-256 digest in the form the `Build` takes.
fn archive_digest_valid(digest: &str) -> bool {
    digest.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
}

/// The values of [`KEEL_ANNOTATIONS`] that `update` writes, in that order; `None` removes one.
fn keel_annotations(update: &Update) -> [Option<String>; 4] {
    match update {
        Update::Off => [None, None, None, None],
        Update::NewBuilds { .. } => [Some(KEEL_FORCE.to_owned()), None, None, None],
        Update::Image { policy, window } => {
            let (policy, match_tag) = policy.annotation();
            [
                Some(policy.to_owned()),
                Some(KEEL_POLL.to_owned()),
                Some(UpdateWindow::schedule(*window)),
                match_tag.then(|| "true".to_owned()),
            ]
        }
    }
}

fn unchanged_update(kind: &str, name: &str) -> PlanError {
    error(
        ErrorKind::BadRequest,
        "unchanged",
        format!("{kind} {name} is already updated this way."),
    )
}

fn validate_window(window: Option<UpdateWindow>) -> Result<(), PlanError> {
    if window.is_some_and(|w| w.hour > 23) {
        return Err(error(
            ErrorKind::BadRequest,
            "invalid_window",
            "The hour of the update window must be 0 to 23.",
        ));
    }
    Ok(())
}

/// The CloudNativePG resource whose image updates the database updater handles.
#[must_use]
pub fn cloudnative_pg_clusters() -> ResourceRef {
    ResourceRef {
        group: "postgresql.cnpg.io".to_owned(),
        version: "v1".to_owned(),
        resource: "clusters".to_owned(),
    }
}

/// Most build variables a `Build` takes.
const MAX_BUILD_ENV: usize = 128;
/// Longest path the `Build` takes for its directory and Dockerfile.
pub const MAX_BUILD_PATH: usize = 1024;

/// A build path as the `Build`'s schema writes it, empty allowed, for the API's description.
pub const BUILD_PATH_PATTERN: &str =
    "^([A-Za-z0-9_@+-][A-Za-z0-9._@+-]*(/[A-Za-z0-9_@+-][A-Za-z0-9._@+-]*)*)?$";

/// Whether `path` is a relative path that the `Build` accepts: no segment starts with `.`, so
/// the path cannot climb out with `..`.
pub(crate) fn build_path(path: &str) -> bool {
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
    pub(crate) fn build(&mut self, name: &str) -> Result<usize, PlanError> {
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

    /// The `Build` named `name` must take its source from an uploaded archive.
    fn archive_build(&mut self, name: &str) -> Result<usize, PlanError> {
        let index = self.build(name)?;
        if self
            .current(index)
            .pointer("/spec/source/type")
            .and_then(Value::as_str)
            != Some("archive")
        {
            return Err(error(
                ErrorKind::BadRequest,
                "not_an_upload",
                format!("Build {name} does not build an uploaded folder."),
            ));
        }
        Ok(index)
    }

    pub(crate) fn upload_token(&mut self, name: &str, request: &str) -> Result<(), PlanError> {
        if request.is_empty()
            || request.len() > MAX_UPLOAD_REQUEST
            || !request
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        {
            return Err(error(
                ErrorKind::BadRequest,
                "invalid_request",
                "The upload request must be 1 to 63 letters, digits, dots, dashes or underscores.",
            ));
        }
        let index = self.archive_build(name)?;
        let path = pointer(&["metadata", "annotations", UPLOAD_TOKEN_REQUEST]);
        let current = self.current(index);
        let before = current.pointer(&path).cloned();
        if before.as_ref().and_then(Value::as_str) == Some(request) {
            return Err(error(
                ErrorKind::BadRequest,
                "unchanged",
                format!("Build {name} was already asked for this upload."),
            ));
        }
        self.claim(
            index,
            &steps("annotations", UPLOAD_TOKEN_REQUEST),
            &shown("annotations", UPLOAD_TOKEN_REQUEST),
        )?;
        let operations = before.map_or_else(
            || set_key(&current, "annotations", UPLOAD_TOKEN_REQUEST, request),
            |before| vec![test(&path, before), replace(&path, json!(request))],
        );
        self.patch(index, operations)
    }

    pub(crate) fn archive_digest(&mut self, name: &str, digest: &str) -> Result<(), PlanError> {
        if !archive_digest_valid(digest) {
            return Err(error(
                ErrorKind::BadRequest,
                "invalid_digest",
                "The digest must be sha256: and 64 lower-case hexadecimal digits.",
            ));
        }
        let index = self.archive_build(name)?;
        let path = pointer(&["spec", "source", "archive", "digest"]);
        let before = self.current(index).pointer(&path).cloned();
        if before.as_ref().and_then(Value::as_str) == Some(digest) {
            return Err(error(
                ErrorKind::BadRequest,
                "unchanged",
                format!("Build {name} already builds this upload."),
            ));
        }
        self.claim(
            index,
            &[
                Step::field("spec"),
                Step::field("source"),
                Step::field("archive"),
                Step::field("digest"),
            ],
            "spec.source.archive.digest",
        )?;
        // `archive` is optional in the CRD: `{type: archive}` alone waits for an upload.
        let operations = match before {
            Some(before) => vec![test(&path, before), replace(&path, json!(digest))],
            None if self
                .current(index)
                .pointer("/spec/source/archive")
                .is_some() =>
            {
                vec![add(&path, json!(digest))]
            }
            None => vec![add("/spec/source/archive", json!({ "digest": digest }))],
        };
        self.patch(index, operations)
    }

    pub(crate) fn auto_update(
        &mut self,
        target: &AutoUpdateTarget,
        update: &Update,
    ) -> Result<(), PlanError> {
        let reference = match target {
            AutoUpdateTarget::Workload(reference) => reference,
            AutoUpdateTarget::Cluster { name, .. } => {
                return self.cluster_auto_update(name, update);
            }
        };
        let changed = self.write_keel(reference, update)?;
        if let Update::NewBuilds { build, webhook } = update {
            return self.notify_keel(build, webhook, changed, reference);
        }
        if !changed {
            return Err(unchanged_update(reference.kind.as_str(), &reference.name));
        }
        Ok(())
    }

    fn cluster_auto_update(&mut self, name: &str, update: &Update) -> Result<(), PlanError> {
        let resource = cloudnative_pg_clusters();
        let object = self
            .input
            .objects
            .iter()
            .find(|f| f.resource == resource && f.name == name)
            .map(|f| f.object.clone())
            .ok_or_else(|| {
                error(
                    ErrorKind::NotFound,
                    "unknown_object",
                    format!("There is no Cluster named {name}."),
                )
            })?;
        let metadata: ObjectMeta = serde_json::from_value(
            object.get("metadata").cloned().unwrap_or_default(),
        )
        .map_err(|e| {
            error(
                ErrorKind::Invalid,
                "invalid_object",
                format!("Cannot read Cluster {name}: {e}"),
            )
        })?;
        Self::refuse_managed(&metadata, "Cluster", name)?;
        let choices = zeabur_model::database_image_updates(
            &metadata,
            object.get("spec").unwrap_or(&Value::Null),
        )
        .ok_or_else(|| {
            error(
                ErrorKind::Conflict,
                "database_updates_read_only",
                "The Cluster is being deleted or its update settings are managed by another tool.",
            )
        })?;
        if let Update::Image { policy, window } = update {
            validate_window(*window)?;
            if !choices.allowed.contains(policy) {
                return Err(error(
                    ErrorKind::BadRequest,
                    "policy_not_allowed",
                    "CloudNativePG updates require an explicit PostgreSQL major tag, no image catalog, controlling owner or approval requirement, and the same-tag policy.",
                ));
            }
        } else if !matches!(update, Update::Off) {
            return Err(error(
                ErrorKind::BadRequest,
                "policy_not_allowed",
                "CloudNativePG updates use the same-tag image policy.",
            ));
        }
        let index = self.working(
            Target {
                api_version: "postgresql.cnpg.io/v1".to_owned(),
                kind: "Cluster".to_owned(),
                plural: "clusters".to_owned(),
                name: name.to_owned(),
            },
            object,
        );
        let normalized = self.clear_cluster_keel_overrides(index)?;
        if !self.write_keel_annotations(index, update)? && !normalized {
            return Err(unchanged_update("Cluster", name));
        }
        Ok(())
    }

    fn clear_cluster_keel_overrides(&mut self, index: usize) -> Result<bool, PlanError> {
        let mut changed = false;
        for map in ["labels", "annotations"] {
            for key in KEEL_ANNOTATIONS
                .into_iter()
                .chain(std::iter::once("keel.sh/matchTag"))
            {
                if map == "annotations" && key != "keel.sh/matchTag" {
                    continue;
                }
                let path = pointer(&["metadata", map, key]);
                let Some(before) = self.current(index).pointer(&path).cloned() else {
                    continue;
                };
                self.claim(index, &steps(map, key), &shown(map, key))?;
                self.patch(index, vec![test(&path, before), remove(&path)])?;
                changed = true;
            }
        }
        Ok(changed)
    }

    /// Writes the Keel annotations `update` asks for on the workload; returns whether any
    /// changed. A `Build`'s notify target is the caller's.
    pub(crate) fn write_keel(
        &mut self,
        reference: &WorkloadRef,
        update: &Update,
    ) -> Result<bool, PlanError> {
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
        if let Update::Image { policy, window } = update {
            validate_window(*window)?;
            let images = found
                .pod_spec()
                .into_iter()
                .flat_map(|spec| &spec.containers)
                .filter_map(|c| c.image.as_deref());
            if !ImagePolicy::allowed(images).contains(policy) {
                return Err(error(
                    ErrorKind::BadRequest,
                    "policy_not_allowed",
                    format!(
                        "The image tags of {} {} do not allow this update policy.",
                        reference.kind.as_str(),
                        reference.name
                    ),
                ));
            }
        }
        let index = self.working(workload_target(found), workload_json(found)?);
        self.write_keel_annotations(index, update)
    }

    fn write_keel_annotations(&mut self, index: usize, update: &Update) -> Result<bool, PlanError> {
        let wanted = keel_annotations(update);
        let mut changed = false;
        for (key, value) in KEEL_ANNOTATIONS.into_iter().zip(wanted) {
            let current = self.current(index);
            let path = pointer(&["metadata", "annotations", key]);
            let before = current.pointer(&path).cloned();
            let operations = match (before, value) {
                (Some(before), Some(value)) if before.as_str() == Some(value.as_str()) => continue,
                (Some(before), Some(value)) => {
                    vec![test(&path, before), replace(&path, json!(value))]
                }
                (None, Some(value)) => set_key(&current, "annotations", key, &value),
                (Some(before), None) => vec![test(&path, before), remove(&path)],
                (None, None) => continue,
            };
            self.claim(
                index,
                &steps("annotations", key),
                &shown("annotations", key),
            )?;
            self.patch(index, operations)?;
            changed = true;
        }
        Ok(changed)
    }

    /// Adds a `keel` target for `webhook` to the `Build`'s `spec.notify`, unless one is there.
    fn notify_keel(
        &mut self,
        name: &str,
        webhook: &str,
        changed: bool,
        reference: &WorkloadRef,
    ) -> Result<(), PlanError> {
        let url = webhook.trim();
        if url.is_empty() {
            return Err(error(
                ErrorKind::BadRequest,
                "webhook_required",
                "Name Keel's webhook to tell it of new images.",
            ));
        }
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
            return if changed {
                Ok(())
            } else {
                Err(unchanged_update(reference.kind.as_str(), &reference.name))
            };
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

    /// Sets `spec.source.<field>` to `value`, or removes it when `value` is empty.
    fn build_path_field(
        &mut self,
        index: usize,
        field: &str,
        value: &str,
    ) -> Result<(), PlanError> {
        let path = pointer(&["spec", "source", field]);
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
                Step::field(field),
            ],
            &format!("spec.source.{field}"),
        )?;
        self.patch(index, operations)
    }

    pub(crate) fn build_env(
        &mut self,
        index: usize,
        build: &str,
        edit: &EnvEdit,
    ) -> Result<(), PlanError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_path_pattern_says_what_the_check_does() {
        let pattern = regex::Regex::new(BUILD_PATH_PATTERN).unwrap();
        let paths = [
            "apps/web",
            "Dockerfile",
            "a@b+c-d_e.f",
            "../etc",
            "apps/.git",
            ".env",
            "apps//web",
            "apps/web/",
            "a b",
        ];
        for path in paths {
            assert_eq!(pattern.is_match(path), build_path(path), "{path:?}");
        }
        assert!(pattern.is_match(""));
    }
}
