//! The planner for rollbacks, storage, probes, compute and YAML edits.
//!
//! Every change here edits an object that exists (or creates a new PersistentVolumeClaim), so it
//! is written as a JSON Patch guarded by `test` operations, like the environment edits (see the
//! `plan` module): server-side apply could not remove or replace fields other managers own.
//!
//! - **Rollback** replaces the pod template with the release's, as `kubectl rollout undo`
//!   [kubectl-undo]; ConfigMap and Secret contents are not part of a template and stay.
//! - **A claim grows** by its `spec.resources.requests.storage`, when its `StorageClass` allows
//!   expansion; claims never shrink [k8s-expand].
//! - **A new volume** is a new claim, `ReadWriteOnce` and named `<workload>-<volume>` unless the
//!   person chooses otherwise, created before the workload's template mounts it; a claim that
//!   exists is refused rather than shared. A StatefulSet's `volumeClaimTemplates` cannot change
//!   after creation [k8s-sts-limitations], so a StatefulSet gets no new volume here.
//! - **A volume is removed** by unmounting it, and dropping it from the pod template once no
//!   container mounts it; the claim and its data stay until deleted on their own.
//! - **Probes** follow the container's declared ports [k8s-probes].
//! - **Scaling** sets `spec.replicas` of a Deployment or StatefulSet.
//! - **A YAML edit** becomes the patch between the object as read and as edited ([`crate::edit`]);
//!   a ConfigMap's or Secret's keys become key changes, so a Secret value shown masked is never
//!   written. What kube-apiserver does not let change (a Secret's `type`, the keys of an
//!   `immutable` one [k8s-secret-immutable]) is planned as edited and refused by its dry run.
//!
//! [kubectl-undo]: https://kubernetes.io/docs/reference/kubectl/generated/kubectl_rollout/kubectl_rollout_undo/
//! [k8s-expand]: https://kubernetes.io/docs/concepts/storage/persistent-volumes/#expanding-persistent-volumes-claims
//! [k8s-sts-limitations]: https://kubernetes.io/docs/concepts/workloads/controllers/statefulset/#limitations
//! [k8s-probes]: https://kubernetes.io/docs/tasks/configure-pod-container/configure-liveness-readiness-startup-probes/
//! [k8s-secret-immutable]: https://kubernetes.io/docs/concepts/configuration/secret/#secret-immutable

use std::collections::BTreeMap;

use base64::Engine as _;
use k8s_openapi::api::core::v1::{Container, PersistentVolumeClaim};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde_json::{Map, Value, json};
use zeabur_graph::Workload;
use zeabur_kernel::quantity;
use zeabur_model::{SourceKind, WorkloadKind, WorkloadRef};

use crate::change::{DataEdit, DataKind, ProbeKind, Target, Warning};
use crate::checks::{
    AccessMode, COMPUTE, ProbeRequest, ResourceRef, VolumeRequest, owned_by_stateful_set,
    probe_config, reconcile_identity, resource_allowed, validate_mount_path, validate_resources,
    validate_storage,
};
use crate::edit::{edit_operations, editable};
use crate::fields::Step;
use crate::json::{add, pointer, remove, replace, test};
use crate::plan::{
    ErrorKind, PlanError, Planner, Working, error, pod_spec_steps, to_json, workload_json,
    workload_target,
};
use crate::rules::{REDACTED, Refusal};

/// A container of a workload: its name, position and spec.
pub(crate) struct Found {
    pub(crate) name: String,
    pub(crate) position: usize,
    pub(crate) spec: Container,
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "called as `map_err(|r| bad(r, …))`"
)]
fn bad(refusal: Refusal, detail: &str) -> PlanError {
    error(ErrorKind::BadRequest, &refusal.0, detail)
}

/// Whether a Secret value in an edited copy is a mask a person was shown: the YAML view's
/// [`REDACTED`], the older `«redacted»`, or a run of `•`.
fn is_mask(value: &str) -> bool {
    value == REDACTED
        || value == "\u{ab}redacted\u{bb}"
        || (!value.is_empty() && value.chars().all(|c| c == '\u{2022}'))
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "the planner's change kinds are split across modules by area"
)]
impl<'a> Planner<'a> {
    // ── Shared lookups ───────────────────────────────────────────────────────────────────────

    /// The workload, refused when its template cannot change or a tool manages it.
    pub(crate) fn editable_workload(
        &self,
        reference: &WorkloadRef,
    ) -> Result<Workload<'a>, PlanError> {
        let found = self.workload(reference)?;
        if found.kind() == WorkloadKind::Job {
            return Err(error(
                ErrorKind::Invalid,
                "immutable_field",
                "A Job's pod template cannot change after it is created.",
            ));
        }
        Self::refuse_managed(found.meta(), reference.kind.as_str(), &reference.name)?;
        Ok(found)
    }

    pub(crate) fn container(
        found: Workload<'_>,
        reference: &WorkloadRef,
        container: Option<&str>,
    ) -> Result<Found, PlanError> {
        let containers = found
            .pod_spec()
            .map_or_else(Vec::new, |s| s.containers.clone());
        let position = match container {
            Some(name) => containers.iter().position(|c| c.name == name),
            None if containers.len() == 1 => Some(0),
            None => {
                return Err(error(
                    ErrorKind::BadRequest,
                    "container_required",
                    "The workload has several containers; name one.",
                ));
            }
        };
        let Some((position, spec)) =
            position.and_then(|p| containers.get(p).cloned().map(|c| (p, c)))
        else {
            return Err(error(
                ErrorKind::NotFound,
                "unknown_container",
                format!(
                    "{} has no container {}.",
                    reference.name,
                    container.unwrap_or_default()
                ),
            ));
        };
        Ok(Found {
            name: spec.name.clone(),
            position,
            spec,
        })
    }

    /// The working copy of the workload, its container's JSON pointer and `managedFields` steps.
    pub(crate) fn container_slot(
        &mut self,
        found: Workload<'_>,
        container: &Found,
    ) -> Result<(usize, String, Vec<Step>), PlanError> {
        let object = workload_json(found)?;
        let index = self.working(workload_target(found), object);
        let mut tokens: Vec<String> = found
            .kind()
            .pod_template_path()
            .iter()
            .map(|t| (*t).to_owned())
            .collect();
        tokens.extend([
            "spec".to_owned(),
            "containers".to_owned(),
            container.position.to_string(),
        ]);
        let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
        let mut steps = pod_spec_steps(found.kind());
        steps.extend([Step::field("containers"), Step::named(&container.name)]);
        self.impact.insert(found.reference());
        Ok((index, pointer(&refs), steps))
    }

    /// Whether `reference` reads a ConfigMap or Secret, into its environment or as files.
    fn reads_configuration(&self, reference: &WorkloadRef) -> bool {
        self.project
            .services
            .iter()
            .filter(|s| s.workload.kind == reference.kind && s.workload.name == reference.name)
            .flat_map(|s| &s.containers)
            .any(|c| {
                let configuration =
                    |kind: SourceKind| matches!(kind, SourceKind::ConfigMap | SourceKind::Secret);
                c.env.iter().any(|e| configuration(e.source.kind))
                    || c.imported_env.iter().any(|e| configuration(e.source.kind))
                    || c.mounts.iter().any(|m| {
                        m.object
                            .as_ref()
                            .is_some_and(|o| matches!(o.kind.as_str(), "ConfigMap" | "Secret"))
                    })
            })
    }

    // ── Rollback ─────────────────────────────────────────────────────────────────────────────

    pub(crate) fn rollback(
        &mut self,
        reference: &WorkloadRef,
        revision: i64,
    ) -> Result<(), PlanError> {
        let found = self.workload(reference)?;
        if !matches!(
            found.kind(),
            WorkloadKind::Deployment | WorkloadKind::StatefulSet | WorkloadKind::DaemonSet
        ) {
            return Err(error(
                ErrorKind::Invalid,
                "not_rollbackable",
                format!(
                    "{} {} keeps no releases to return to.",
                    reference.kind.as_str(),
                    reference.name
                ),
            ));
        }
        Self::refuse_managed(found.meta(), reference.kind.as_str(), &reference.name)?;
        let release = self
            .input
            .releases
            .get(reference)
            .and_then(|list| list.iter().find(|r| r.revision == revision))
            .ok_or_else(|| {
                error(
                    ErrorKind::NotFound,
                    "unknown_revision",
                    format!("{} has no release {revision}.", reference.name),
                )
            })?;
        if release.current {
            return Err(error(
                ErrorKind::Invalid,
                "already_current",
                format!("{} already runs release {revision}.", reference.name),
            ));
        }
        let template = release.template.clone();
        let object = workload_json(found)?;
        let index = self.working(workload_target(found), object);
        self.claim(
            index,
            &[Step::field("spec"), Step::field("template")],
            "the pod template",
        )?;
        self.patch(index, vec![replace("/spec/template", template)])?;
        self.impact.insert(reference.clone());
        if self.reads_configuration(reference) {
            self.warnings.push(Warning::ConfigNotRolledBack {
                workload: reference.clone(),
            });
        }
        Ok(())
    }

    // ── Storage ──────────────────────────────────────────────────────────────────────────────

    #[expect(clippy::too_many_lines, reason = "one change kind, read top to bottom")]
    pub(crate) fn expand_volume(&mut self, name: &str, size: &str) -> Result<(), PlanError> {
        let Some(claim) = self.input.snapshot.persistent_volume_claim(name).cloned() else {
            return Err(error(
                ErrorKind::NotFound,
                "unknown_claim",
                format!("There is no PersistentVolumeClaim named {name}."),
            ));
        };
        Self::refuse_managed(&claim.metadata, "PersistentVolumeClaim", name)?;
        let wanted = quantity::bytes(size).ok_or_else(|| {
            error(
                ErrorKind::BadRequest,
                "invalid_quantity",
                format!("{size:?} is not a size such as 20Gi."),
            )
        })?;
        let requested = claim
            .spec
            .as_ref()
            .and_then(|s| s.resources.as_ref())
            .and_then(|r| r.requests.as_ref())
            .and_then(|r| r.get("storage"))
            .map(|q| q.0.clone());
        if requested
            .as_deref()
            .and_then(quantity::bytes)
            .is_some_and(|now| wanted <= now)
        {
            return Err(error(
                ErrorKind::Invalid,
                "size_not_larger",
                format!(
                    "{name} has {} now; a claim can only grow.",
                    requested.unwrap_or_default()
                ),
            ));
        }
        let class_name = claim
            .spec
            .as_ref()
            .and_then(|s| s.storage_class_name.clone());
        match self.input.storage_classes {
            Some(classes) => {
                let class = class_name.as_ref().map_or_else(
                    || classes.iter().find(|c| c.default),
                    |wanted| classes.iter().find(|c| &c.name == wanted),
                );
                match class {
                    Some(class) if !class.allow_expansion => {
                        return Err(error(
                            ErrorKind::Invalid,
                            "expansion_not_allowed",
                            format!(
                                "StorageClass {} does not allow its claims to grow \
                                 (allowVolumeExpansion).",
                                class.name
                            ),
                        ));
                    }
                    Some(_) => {}
                    None => self.warnings.push(Warning::StorageClassUnknown {
                        claim: name.to_owned(),
                    }),
                }
            }
            None => self.warnings.push(Warning::StorageClassUnknown {
                claim: name.to_owned(),
            }),
        }
        let index = self.working(Target::of::<PersistentVolumeClaim>(name), to_json(&claim)?);
        self.claim(
            index,
            &[
                Step::field("spec"),
                Step::field("resources"),
                Step::field("requests"),
                Step::field("storage"),
            ],
            "the requested size",
        )?;
        let operation = if requested.is_some() {
            replace("/spec/resources/requests/storage", json!(size))
        } else {
            add("/spec/resources", json!({"requests": {"storage": size}}))
        };
        self.patch(index, vec![operation])?;
        // Who mounts it.
        let readers: Vec<WorkloadRef> = self
            .project
            .services
            .iter()
            .filter(|s| {
                s.containers.iter().any(|c| {
                    c.mounts.iter().any(|m| {
                        m.claim_name.as_deref() == Some(name)
                            || m.object.as_ref().is_some_and(|o| {
                                o.kind == "PersistentVolumeClaim" && o.name == name
                            })
                    })
                }) || (s.workload.kind == WorkloadKind::StatefulSet
                    && owned_by_stateful_set(name, &s.workload.name))
            })
            .map(|s| WorkloadRef {
                kind: s.workload.kind,
                name: s.workload.name.clone(),
            })
            .collect();
        self.impact.extend(readers);
        Ok(())
    }

    #[expect(clippy::too_many_lines, reason = "one change kind, read top to bottom")]
    pub(crate) fn add_volume(
        &mut self,
        reference: &WorkloadRef,
        container: Option<&str>,
        volume: &VolumeRequest,
    ) -> Result<(), PlanError> {
        let found = self.editable_workload(reference)?;
        if found.kind() == WorkloadKind::StatefulSet {
            return Err(error(
                ErrorKind::Invalid,
                "stateful_set_volumes_fixed",
                "A StatefulSet's volumeClaimTemplates cannot change after it is created; add \
                 the volume to its manifest and recreate it.",
            ));
        }
        let target = Self::container(found, reference, container)?;
        // As this plan has left the template so far, so two new volumes cannot collide.
        let (index, container_pointer, _) = self.container_slot(found, &target)?;
        let working = self.current(index);
        let mounts = array_at(&working, &format!("{container_pointer}/volumeMounts"));
        let paths: Vec<&str> = mounts
            .iter()
            .filter_map(|m| m["mountPath"].as_str())
            .collect();
        validate_storage(volume, &paths).map_err(|r| bad(r, "The volume is not valid."))?;
        let volume_in_use = array_at(
            &working,
            &format!("{}/volumes", spec_of(&container_pointer)),
        )
        .iter()
        .any(|v| v["name"].as_str() == Some(volume.name.as_str()));
        if volume_in_use {
            return Err(error(
                ErrorKind::Conflict,
                "volume_name_in_use",
                format!(
                    "{} already has a volume named {}.",
                    reference.name, volume.name
                ),
            ));
        }
        let claim_name = volume
            .claim_name
            .clone()
            .unwrap_or_else(|| format!("{}-{}", reference.name, volume.name));
        let claim_target = Target::of::<PersistentVolumeClaim>(&claim_name);
        if self
            .input
            .snapshot
            .persistent_volume_claim(&claim_name)
            .is_some()
            || self.objects.iter().any(|w| w.target == claim_target)
        {
            return Err(error(
                ErrorKind::Conflict,
                "claim_name_taken",
                format!("A PersistentVolumeClaim named {claim_name} already exists."),
            ));
        }
        if let (Some(class), Some(classes)) = (&volume.storage_class, self.input.storage_classes)
            && !classes.iter().any(|c| &c.name == class)
        {
            return Err(error(
                ErrorKind::BadRequest,
                "unknown_storage_class",
                format!("There is no StorageClass named {class}."),
            ));
        }
        let access_mode = volume.access_mode.unwrap_or_default();
        let mut spec = json!({
            "accessModes": [access_mode],
            "resources": {"requests": {"storage": volume.size}},
        });
        if let (Some(class), Some(map)) = (&volume.storage_class, spec.as_object_mut()) {
            map.insert("storageClassName".to_owned(), json!(class));
        }
        let body = json!({
            "apiVersion": "v1",
            "kind": "PersistentVolumeClaim",
            "metadata": {"name": claim_name, "namespace": self.input.snapshot.namespace},
            "spec": spec,
        });
        // The claim first, so the pods find it.
        self.objects.insert(
            0,
            Working {
                target: claim_target,
                original: None,
                current: body,
                operations: Vec::new(),
                owners: Vec::new(),
                deleted: false,
            },
        );
        let (index, container_pointer, _) = self.container_slot(found, &target)?;
        let spec_pointer = spec_of(&container_pointer);
        let current = self.current(index);
        let volume_entry =
            json!({"name": volume.name, "persistentVolumeClaim": {"claimName": claim_name}});
        let mount_entry = json!({"name": volume.name, "mountPath": volume.mount_path});
        let mut operations = vec![test(
            &format!("{container_pointer}/name"),
            json!(target.name),
        )];
        operations.push(
            if current
                .pointer(&format!("{spec_pointer}/volumes"))
                .is_some()
            {
                add(&format!("{spec_pointer}/volumes/-"), volume_entry)
            } else {
                add(&format!("{spec_pointer}/volumes"), json!([volume_entry]))
            },
        );
        operations.push(
            if current
                .pointer(&format!("{container_pointer}/volumeMounts"))
                .is_some()
            {
                add(&format!("{container_pointer}/volumeMounts/-"), mount_entry)
            } else {
                add(
                    &format!("{container_pointer}/volumeMounts"),
                    json!([mount_entry]),
                )
            },
        );
        self.patch(index, operations)?;
        let replicas = current
            .pointer("/spec/replicas")
            .and_then(Value::as_i64)
            .unwrap_or(1);
        if found.kind() == WorkloadKind::DaemonSet || replicas > 1 {
            let workload = reference.clone();
            match access_mode {
                AccessMode::ReadWriteOnce => {
                    self.warnings
                        .push(Warning::ReadWriteOnceShared { workload });
                }
                AccessMode::ReadWriteOncePod => {
                    self.warnings
                        .push(Warning::ReadWriteOncePodShared { workload });
                }
                AccessMode::ReadWriteMany | AccessMode::ReadOnlyMany => {}
            }
        }
        Ok(())
    }

    pub(crate) fn edit_mount(
        &mut self,
        reference: &WorkloadRef,
        container: Option<&str>,
        mount_path: &str,
        new_mount_path: Option<&str>,
        read_only: Option<bool>,
    ) -> Result<(), PlanError> {
        if new_mount_path.is_none() && read_only.is_none() {
            return Err(error(
                ErrorKind::BadRequest,
                "no_changes",
                "Nothing to change on the mount.",
            ));
        }
        let found = self.editable_workload(reference)?;
        let target = Self::container(found, reference, container)?;
        let (index, container_pointer, mut steps) = self.container_slot(found, &target)?;
        let mounts = array_at(
            &self.current(index),
            &format!("{container_pointer}/volumeMounts"),
        );
        let Some(at) = mounts
            .iter()
            .position(|m| m["mountPath"].as_str() == Some(mount_path))
        else {
            return Err(error(
                ErrorKind::NotFound,
                "unknown_mount",
                format!("Container {} mounts nothing at {mount_path}.", target.name),
            ));
        };
        if let Some(path) = new_mount_path {
            let others: Vec<&str> = mounts
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != at)
                .filter_map(|(_, m)| m["mountPath"].as_str())
                .collect();
            validate_mount_path(path, &others)
                .map_err(|r| bad(r, "The mount path is not valid."))?;
        }
        steps.extend([
            Step::field("volumeMounts"),
            Step::Key(json!({ "mountPath": mount_path })),
        ]);
        self.claim(index, &steps, &format!("the mount at {mount_path}"))?;
        let entry = format!("{container_pointer}/volumeMounts/{at}");
        let mut operations = vec![test(&format!("{entry}/mountPath"), json!(mount_path))];
        if let Some(path) = new_mount_path {
            operations.push(replace(&format!("{entry}/mountPath"), json!(path)));
        }
        if let Some(read_only) = read_only {
            operations.push(add(&format!("{entry}/readOnly"), json!(read_only)));
        }
        self.patch(index, operations)
    }

    /// Unmounts a volume, and drops it from the pod template when no container mounts it any
    /// more; the claim it names is left alone.
    pub(crate) fn remove_volume(
        &mut self,
        reference: &WorkloadRef,
        container: Option<&str>,
        mount_path: &str,
    ) -> Result<(), PlanError> {
        let found = self.editable_workload(reference)?;
        let target = Self::container(found, reference, container)?;
        let (index, container_pointer, mut steps) = self.container_slot(found, &target)?;
        let working = self.current(index);
        let mounts = array_at(&working, &format!("{container_pointer}/volumeMounts"));
        let Some(at) = mounts
            .iter()
            .position(|m| m["mountPath"].as_str() == Some(mount_path))
        else {
            return Err(error(
                ErrorKind::NotFound,
                "unknown_mount",
                format!("Container {} mounts nothing at {mount_path}.", target.name),
            ));
        };
        let name = mounts
            .get(at)
            .and_then(|m| m["name"].as_str())
            .unwrap_or_default()
            .to_owned();
        let spec_pointer = spec_of(&container_pointer);
        let volumes = array_at(&working, &format!("{spec_pointer}/volumes"));
        let Some(volume_at) = volumes
            .iter()
            .position(|v| v["name"].as_str() == Some(name.as_str()))
        else {
            return Err(error(
                ErrorKind::Invalid,
                "stateful_set_volumes_fixed",
                "A StatefulSet's volumeClaimTemplates cannot change after it is created; remove \
                 the volume from its manifest and recreate it.",
            ));
        };
        steps.extend([
            Step::field("volumeMounts"),
            Step::Key(json!({ "mountPath": mount_path })),
        ]);
        self.claim(index, &steps, &format!("the mount at {mount_path}"))?;
        let entry = format!("{container_pointer}/volumeMounts/{at}");
        let mut operations = vec![
            test(&format!("{entry}/mountPath"), json!(mount_path)),
            remove(&entry),
        ];
        let mounted_elsewhere = ["containers", "initContainers"].iter().any(|list| {
            array_at(&working, &format!("{spec_pointer}/{list}"))
                .iter()
                .enumerate()
                .any(|(position, c)| {
                    let this = *list == "containers" && position == target.position;
                    c["volumeMounts"].as_array().is_some_and(|ms| {
                        ms.iter().enumerate().any(|(i, m)| {
                            m["name"].as_str() == Some(name.as_str()) && !(this && i == at)
                        })
                    })
                })
        });
        if !mounted_elsewhere {
            let volume = format!("{spec_pointer}/volumes/{volume_at}");
            operations.push(test(&format!("{volume}/name"), json!(name)));
            operations.push(remove(&volume));
        }
        self.patch(index, operations)
    }

    // ── Probes and compute ───────────────────────────────────────────────────────────────────

    pub(crate) fn probe(
        &mut self,
        reference: &WorkloadRef,
        container: Option<&str>,
        kind: ProbeKind,
        set: Option<&ProbeRequest>,
    ) -> Result<(), PlanError> {
        let found = self.editable_workload(reference)?;
        let target = Self::container(found, reference, container)?;
        let ports: Vec<i32> = target
            .spec
            .ports
            .iter()
            .flatten()
            .map(|p| p.container_port)
            .collect();
        let probe = set
            .map(|request| probe_config(request, &ports))
            .transpose()
            .map_err(|r| bad(r, "The probe is not valid."))?;
        let (index, container_pointer, mut steps) = self.container_slot(found, &target)?;
        steps.push(Step::field(kind.field()));
        self.claim(
            index,
            &steps,
            &format!("{} of container {}", kind.field(), target.name),
        )?;
        let field = format!("{container_pointer}/{}", kind.field());
        let exists = self.current(index).pointer(&field).is_some();
        let change = match (probe, exists) {
            (Some(probe), true) => replace(&field, probe),
            (Some(probe), false) => add(&field, probe),
            (None, true) => remove(&field),
            (None, false) => {
                return Err(error(
                    ErrorKind::NotFound,
                    "unknown_probe",
                    format!("Container {} has no {}.", target.name, kind.field()),
                ));
            }
        };
        self.patch(
            index,
            vec![
                test(&format!("{container_pointer}/name"), json!(target.name)),
                change,
            ],
        )
    }

    pub(crate) fn scale(
        &mut self,
        reference: &WorkloadRef,
        replicas: i32,
    ) -> Result<(), PlanError> {
        let found = self.workload(reference)?;
        if !matches!(
            found.kind(),
            WorkloadKind::Deployment | WorkloadKind::StatefulSet
        ) {
            return Err(error(
                ErrorKind::Invalid,
                "not_scalable",
                format!(
                    "A {} runs as many pods as it needs; it has no replica count.",
                    reference.kind.as_str()
                ),
            ));
        }
        if replicas < 0 {
            return Err(error(
                ErrorKind::BadRequest,
                "invalid_replicas",
                "Replicas cannot be negative.",
            ));
        }
        Self::refuse_managed(found.meta(), reference.kind.as_str(), &reference.name)?;
        let object = workload_json(found)?;
        let index = self.working(workload_target(found), object);
        self.claim(
            index,
            &[Step::field("spec"), Step::field("replicas")],
            "replicas",
        )?;
        self.impact.insert(reference.clone());
        let operation = if self.current(index).pointer("/spec/replicas").is_some() {
            replace("/spec/replicas", json!(replicas))
        } else {
            add("/spec/replicas", json!(replicas))
        };
        self.patch(index, vec![operation])
    }

    pub(crate) fn resources(
        &mut self,
        reference: &WorkloadRef,
        container: Option<&str>,
        requests: &BTreeMap<String, String>,
        limits: &BTreeMap<String, String>,
    ) -> Result<(), PlanError> {
        if let Some(name) = requests
            .keys()
            .chain(limits.keys())
            .find(|name| !COMPUTE.contains(&name.as_str()))
        {
            return Err(error(
                ErrorKind::BadRequest,
                "unknown_resource",
                format!("{name} is not cpu, memory or ephemeral-storage; edit it as YAML."),
            ));
        }
        let found = self.editable_workload(reference)?;
        let target = Self::container(found, reference, container)?;
        let current = serde_json::to_value(&target.spec.resources)
            .ok()
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({}));
        let mut desired = current.clone();
        for (section, given) in [("requests", requests), ("limits", limits)] {
            let mut map: Map<String, Value> = current
                .get(section)
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            map.retain(|name, _| !COMPUTE.contains(&name.as_str()));
            for (name, value) in given {
                map.insert(name.clone(), json!(value));
            }
            if let Some(object) = desired.as_object_mut() {
                if map.is_empty() {
                    object.remove(section);
                } else {
                    object.insert(section.to_owned(), Value::Object(map));
                }
            }
        }
        validate_resources(&desired).map_err(|r| bad(r, "The resources are not valid."))?;
        if desired == current {
            return Err(error(
                ErrorKind::BadRequest,
                "no_changes",
                "The resources already have these values.",
            ));
        }
        let (index, container_pointer, mut steps) = self.container_slot(found, &target)?;
        steps.push(Step::field("resources"));
        self.claim(
            index,
            &steps,
            &format!("resources of container {}", target.name),
        )?;
        let field = format!("{container_pointer}/resources");
        let exists = self.current(index).pointer(&field).is_some();
        let empty = desired.as_object().is_none_or(Map::is_empty);
        let change = match (exists, empty) {
            (true, true) => remove(&field),
            (true, false) => replace(&field, desired),
            (false, _) => add(&field, desired),
        };
        self.patch(
            index,
            vec![
                test(&format!("{container_pointer}/name"), json!(target.name)),
                change,
            ],
        )
    }

    // ── YAML edits ───────────────────────────────────────────────────────────────────────────

    #[expect(clippy::too_many_lines, reason = "one change kind, read top to bottom")]
    pub(crate) fn edit(
        &mut self,
        resource: &ResourceRef,
        name: &str,
        yaml: &str,
    ) -> Result<(), PlanError> {
        let edited: Value = serde_saphyr::from_str(yaml).map_err(|e| {
            error(
                ErrorKind::BadRequest,
                "invalid_yaml",
                format!("Unable to read the YAML: {e}"),
            )
        })?;
        let allowed = resource_allowed(resource);
        if let Some(reason) = allowed.reason {
            return Err(error(
                ErrorKind::Invalid,
                &reason,
                format!("{} cannot be edited here ({reason}).", resource.resource),
            ));
        }
        let keys = match (resource.group.as_str(), resource.resource.as_str()) {
            ("", "configmaps") => Some(DataKind::ConfigMap),
            ("", "secrets") => Some(DataKind::Secret),
            _ => None,
        };
        let fetched = self
            .input
            .objects
            .iter()
            .find(|f| &f.resource == resource && f.name == name)
            .map(|f| f.object.clone());
        // A ConfigMap or Secret the person may list but not read comes from the list (a
        // Secret's metadata and type, without its values).
        let fetched = match (fetched, keys) {
            (Some(object), _) => Some(object),
            (None, Some(DataKind::ConfigMap)) => self
                .input
                .snapshot
                .config_map(name)
                .map(to_json)
                .transpose()?,
            (None, Some(DataKind::Secret)) => {
                self.input.snapshot.secret(name).map(to_json).transpose()?
            }
            (None, None) => None,
        };
        let Some(mut object) = fetched else {
            return Err(error(
                ErrorKind::NotFound,
                "unknown_object",
                format!("There is no {} named {name}.", resource.resource),
            ));
        };
        let kind = object
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let namespace = self.input.snapshot.namespace.clone();
        reconcile_identity(resource, &kind, &namespace, name, &edited)
            .map_err(|r| bad(r, "The YAML names another object."))?;
        let meta: ObjectMeta = object
            .get("metadata")
            .cloned()
            .and_then(|m| serde_json::from_value(m).ok())
            .unwrap_or_default();
        Self::refuse_managed(&meta, &kind, name)?;
        if keys == Some(DataKind::Secret) {
            self.with_secret_values(name, &mut object);
        }
        let index = self.working(
            Target {
                api_version: resource.api_version(),
                kind: kind.clone(),
                plural: resource.resource.clone(),
                name: name.to_owned(),
            },
            object.clone(),
        );
        let mut current = editable(&self.current(index));
        let mut wanted = editable(&edited);
        // The namespace is the project's, said or not.
        if let Some(metadata) = wanted.get_mut("metadata").and_then(Value::as_object_mut) {
            metadata.insert("namespace".to_owned(), json!(namespace));
        }
        // A ConfigMap's or Secret's keys change one by one, without the values left as they
        // are or shown masked; the rest of it as any other object, and kube-apiserver's dry run
        // refuses what it does not allow to change. The keys compare with the object as read.
        let key_edits = match keys {
            Some(data_kind) => {
                let edits = self.key_edits(data_kind, name, &object, &edited)?;
                for side in [&mut current, &mut wanted] {
                    if let Some(map) = side.as_object_mut() {
                        map.remove("data");
                        map.remove("stringData");
                    }
                }
                edits.map(|edits| (data_kind, edits))
            }
            None => None,
        };
        let operations = edit_operations(&current, &wanted);
        if operations.is_empty() && key_edits.is_none() {
            return Err(error(
                ErrorKind::BadRequest,
                "no_changes",
                "The edited copy is the same as the object.",
            ));
        }
        for operation in operations {
            self.claim(index, &operation.field, &operation.shown)?;
            self.patch(index, operation.operations)?;
        }
        if let Some((data_kind, edits)) = key_edits {
            self.data(data_kind, name, &edits)?;
        }
        if let Some(workload) = WorkloadKind::parse(&kind) {
            self.impact.insert(WorkloadRef {
                kind: workload,
                name: name.to_owned(),
            });
        }
        Ok(())
    }

    /// The key changes of a ConfigMap's or Secret's YAML edit, `None` when there are none.
    ///
    /// A Secret value shown masked ([`is_mask`]) is left as it is, and so is one equal to the
    /// value the person could read; a Secret's keys are removed only when its values were read,
    /// so a key the edited copy leaves out is known to exist.
    fn key_edits(
        &self,
        kind: DataKind,
        name: &str,
        current: &Value,
        edited: &Value,
    ) -> Result<Option<Vec<DataEdit>>, PlanError> {
        let strings = |object: &Value, field: &str| -> BTreeMap<String, String> {
            object
                .get(field)
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned()))
                .collect()
        };
        let set = |key: &str, value: String| DataEdit {
            op: "set".to_owned(),
            key: key.to_owned(),
            value: Some(value),
        };
        let mut edits = Vec::new();
        match kind {
            DataKind::ConfigMap => {
                let before = strings(current, "data");
                let after = strings(edited, "data");
                for (key, value) in &after {
                    if before.get(key) != Some(value) {
                        edits.push(set(key, value.clone()));
                    }
                }
                for key in before.keys().filter(|k| !after.contains_key(*k)) {
                    edits.push(DataEdit {
                        op: "remove".to_owned(),
                        key: key.clone(),
                        value: None,
                    });
                }
            }
            DataKind::Secret => {
                let known = self.input.secret_values.get(name);
                let data = strings(edited, "data");
                let plain = strings(edited, "stringData");
                for (key, encoded) in &data {
                    if is_mask(encoded) {
                        continue;
                    }
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(encoded)
                        .map_err(|_decode| {
                            error(
                                ErrorKind::BadRequest,
                                "invalid_base64",
                                format!(
                                    "The value of {key} under data is not base64; put plain \
                                     text under stringData instead."
                                ),
                            )
                        })?;
                    if known.and_then(|k| k.get(key)) == Some(&bytes) {
                        continue;
                    }
                    let text = String::from_utf8(bytes).map_err(|_utf8| {
                        error(
                            ErrorKind::Invalid,
                            "binary_secret_value",
                            format!("{key} is binary; it cannot be edited here."),
                        )
                    })?;
                    edits.push(set(key, text));
                }
                for (key, text) in plain.iter().filter(|(_, text)| !is_mask(text)) {
                    edits.push(set(key, text.clone()));
                }
                if let Some(known) = known {
                    for key in known.keys() {
                        if !data.contains_key(key) && !plain.contains_key(key) {
                            edits.push(DataEdit {
                                op: "remove".to_owned(),
                                key: key.clone(),
                                value: None,
                            });
                        }
                    }
                }
            }
        }
        Ok((!edits.is_empty()).then_some(edits))
    }
}

/// The array at `pointer`, or none.
fn array_at(object: &Value, pointer: &str) -> Vec<Value> {
    object
        .pointer(pointer)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The pod spec's pointer, from one of its containers'.
fn spec_of(container_pointer: &str) -> String {
    container_pointer
        .rsplit_once("/containers/")
        .map_or_else(String::new, |(spec, _)| spec.to_owned())
}
