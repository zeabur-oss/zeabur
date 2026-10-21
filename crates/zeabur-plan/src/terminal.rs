//! What a terminal needs written first: a debug container in a pod whose image has no shell, and
//! a pod with `kubectl` and `helm` that act as the person.
//!
//! # Debug containers
//!
//! A container built without a shell (a distroless image [distroless]) cannot run one, so a
//! terminal goes into an *ephemeral container* added to the same pod [k8s-ephemeral]: a shell
//! image, sharing the target container's process namespace (`targetContainerName`), started with
//! standard input and a TTY so its own shell waits instead of exiting, as `kubectl debug -it`
//! adds one [kubectl-debug]. The terminal then runs a new shell in it through `pods/exec`, which
//! kube-apiserver and the kubelet allow for ephemeral containers as for any other, so a second
//! terminal does not need a second container. Kubernetes never removes an ephemeral container: it
//! stays in the pod's spec until the pod is replaced, which the plan says.
//!
//! The container is Zeabur's when its name starts with [`DEBUG_PREFIX`] (an ephemeral container
//! has no labels). One that still runs, or is still starting, for the same target is used again,
//! and the plan then writes nothing.
//!
//! It runs as the target container's user where the pod or the container names one, so the
//! target's files (`/proc/1/root`) and processes are reachable without `CAP_SYS_PTRACE`
//! [proc-root]; with all capabilities dropped, no privilege escalation and the runtime's default
//! seccomp profile, it meets the `restricted` Pod Security Standard whenever the pod does
//! [k8s-pss].
//!
//! The write is a JSON Patch of the `ephemeralcontainers` subresource whose first operation tests
//! the pod's `uid`: the pod's `resourceVersion` moves with every status update and its spec cannot
//! change otherwise, so only a pod replaced since the plan makes it stale.
//!
//! # Cluster tools
//!
//! A pod in the project's namespace (or the one Zeabur's chart names) with an image that has
//! `kubectl` and `helm`, created as the person. It has no service account token
//! (`automountServiceAccountToken: false`), runs as user 65532 with a read-only root file system,
//! no capabilities and the runtime's default seccomp profile, stops at its
//! `activeDeadlineSeconds`, and is labelled as Zeabur's tools pod for one person
//! ([`TOOLS_LABEL`], [`TOOLS_PERSON_LABEL`]). Its home directory's `.kube` is a memory-backed
//! `emptyDir`; the credential is written there through the terminal's exec stream when the
//! terminal opens, never into the spec, an environment variable, a ConfigMap or a Secret (see
//! `zeabur_api::tools`). kube-apiserver's CA comes from the `kube-root-ca.crt` ConfigMap that
//! kube-controller-manager publishes in every namespace [k8s-root-ca].
//!
//! # References
//!
//! - [k8s-ephemeral] The Kubernetes Authors, "Ephemeral Containers," Kubernetes Documentation. <https://kubernetes.io/docs/concepts/workloads/pods/ephemeral-containers/>
//! - [kubectl-debug] The Kubernetes Authors, "Debugging with an ephemeral debug container," Kubernetes Documentation. <https://kubernetes.io/docs/tasks/debug/debug-application/debug-running-pod/#ephemeral-container>
//! - [distroless] Google, "Distroless container images." <https://github.com/GoogleContainerTools/distroless>
//! - [proc-root] M. Kerrisk, "`proc_pid_root(5)`," Linux man-pages. <https://man7.org/linux/man-pages/man5/proc_pid_root.5.html>
//! - [k8s-pss] The Kubernetes Authors, "Pod Security Standards," Kubernetes Documentation. <https://kubernetes.io/docs/concepts/security/pod-security-standards/>
//! - [k8s-root-ca] The Kubernetes Authors, "Root CA certificate ConfigMap," in "Managing Service Accounts," Kubernetes Documentation. <https://kubernetes.io/docs/reference/access-authn-authz/service-accounts-admin/#root-ca-certificate-configmap>
//!
//! [k8s-ephemeral]: https://kubernetes.io/docs/concepts/workloads/pods/ephemeral-containers/
//! [kubectl-debug]: https://kubernetes.io/docs/tasks/debug/debug-application/debug-running-pod/#ephemeral-container
//! [distroless]: https://github.com/GoogleContainerTools/distroless
//! [proc-root]: https://man7.org/linux/man-pages/man5/proc_pid_root.5.html
//! [k8s-pss]: https://kubernetes.io/docs/concepts/security/pod-security-standards/
//! [k8s-root-ca]: https://kubernetes.io/docs/reference/access-authn-authz/service-accounts-admin/#root-ca-certificate-configmap

use std::collections::BTreeMap;

use serde_json::{Value, json};
use zeabur_core::crypto::{hex, sha256};

use crate::change::{Plan, PlannedObject, Target, Warning, Write};
use crate::plan::{ErrorKind, PlanError, error, shown, yaml};
use crate::rules::diff;

/// The name of every debug container Zeabur adds starts with this.
pub const DEBUG_PREFIX: &str = "zeabur-debug-";
/// The name of every tools pod starts with this.
pub const TOOLS_PREFIX: &str = "zeabur-tools-";
/// The label every tools pod carries, `"true"`.
pub const TOOLS_LABEL: &str = "zeabur.com/tools";
/// The label naming whose tools pod it is: [`person_key`] of their user name.
pub const TOOLS_PERSON_LABEL: &str = "zeabur.com/tools-person";
/// The annotation with the person's user name, as the cluster gave it.
pub const TOOLS_PERSON_ANNOTATION: &str = "zeabur.com/tools-person";
/// The annotation with the project the pod was opened for.
pub const TOOLS_PROJECT_ANNOTATION: &str = "zeabur.com/tools-project";
/// The tools pod's one container.
pub const TOOLS_CONTAINER: &str = "tools";
/// The tools container's home directory.
pub const TOOLS_HOME: &str = "/home/user";
/// Where the tools container finds kube-apiserver's CA.
pub const TOOLS_CA: &str = "/var/run/zeabur/kube-api/ca.crt";
/// The user the tools container runs as.
pub const TOOLS_USER: i64 = 65532;
/// The user a debug container runs as when the pod must not run as root and names no user.
const NOBODY: i64 = 65534;

/// A label value for a user name, which may hold characters a label cannot: the first 16
/// hexadecimal digits of its SHA-256.
#[must_use]
pub fn person_key(username: &str) -> String {
    hex(sha256(username.as_bytes()).get(..8).unwrap_or_default())
}

/// The state of a container, from the pod's status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerState {
    /// Running.
    Running,
    /// Not started yet, with the kubelet's reason and message (`ErrImagePull`, …).
    Waiting {
        /// `state.waiting.reason`.
        reason: Option<String>,
        /// `state.waiting.message`.
        message: Option<String>,
    },
    /// Ended.
    Terminated {
        /// `state.terminated.reason`.
        reason: Option<String>,
        /// `state.terminated.exitCode`.
        exit_code: Option<i64>,
    },
    /// No status yet: the kubelet has not seen it.
    Unknown,
}

/// A debug container of Zeabur's in a pod.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugContainer {
    /// Its name.
    pub name: String,
    /// The container whose processes it shares.
    pub target: Option<String>,
    /// Its image.
    pub image: String,
    /// Its state.
    pub state: ContainerState,
}

impl DebugContainer {
    /// Whether a terminal can use it now or once it starts.
    #[must_use]
    pub const fn usable(&self) -> bool {
        match self.state {
            ContainerState::Running | ContainerState::Waiting { .. } | ContainerState::Unknown => {
                true
            }
            ContainerState::Terminated { .. } => false,
        }
    }
}

fn container_state(status: Option<&Value>) -> ContainerState {
    let Some(state) = status.and_then(|s| s.get("state")) else {
        return ContainerState::Unknown;
    };
    let text = |path: &str| {
        state
            .pointer(path)
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    if state.get("running").is_some() {
        ContainerState::Running
    } else if state.get("terminated").is_some() {
        ContainerState::Terminated {
            reason: text("/terminated/reason"),
            exit_code: state
                .pointer("/terminated/exitCode")
                .and_then(Value::as_i64),
        }
    } else if state.get("waiting").is_some() {
        ContainerState::Waiting {
            reason: text("/waiting/reason"),
            message: text("/waiting/message"),
        }
    } else {
        ContainerState::Unknown
    }
}

fn items<'a>(object: &'a Value, path: &str) -> impl Iterator<Item = &'a Value> {
    object
        .pointer(path)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn name_of(item: &Value) -> Option<&str> {
    item.get("name")?.as_str()
}

/// Zeabur's debug containers in `pod`, in the order they were added.
#[must_use]
pub fn debug_containers(pod: &Value) -> Vec<DebugContainer> {
    items(pod, "/spec/ephemeralContainers")
        .filter_map(|container| {
            let name = name_of(container)?;
            name.starts_with(DEBUG_PREFIX).then(|| DebugContainer {
                name: name.to_owned(),
                target: container
                    .get("targetContainerName")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                image: container
                    .get("image")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                state: container_state(
                    items(pod, "/status/ephemeralContainerStatuses")
                        .find(|s| name_of(s) == Some(name)),
                ),
            })
        })
        .collect()
}

/// The container `asked` for in `pod`, or its default: the one its
/// `kubectl.kubernetes.io/default-container` annotation names, or its first.
///
/// # Errors
///
/// When the pod has no such container, or none.
pub fn target_container(pod: &Value, asked: Option<&str>) -> Result<String, PlanError> {
    let pod_name = pod
        .pointer("/metadata/name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let containers: Vec<&str> = items(pod, "/spec/containers").filter_map(name_of).collect();
    if let Some(asked) = asked {
        return if containers.contains(&asked) {
            Ok(asked.to_owned())
        } else {
            Err(error(
                ErrorKind::NotFound,
                "no_such_container",
                format!("Pod {pod_name} has no container {asked}."),
            ))
        };
    }
    pod.pointer("/metadata/annotations/kubectl.kubernetes.io~1default-container")
        .and_then(Value::as_str)
        .filter(|d| containers.contains(d))
        .or_else(|| containers.first().copied())
        .map(str::to_owned)
        .ok_or_else(|| {
            error(
                ErrorKind::NotFound,
                "no_such_container",
                format!("Pod {pod_name} has no containers."),
            )
        })
}

/// The security context of a debug container for `target` in `pod` (see the module
/// documentation).
fn debug_security_context(pod: &Value, target: &str) -> Value {
    let container = items(pod, "/spec/containers").find(|c| name_of(c) == Some(target));
    let field = |name: &str| {
        container
            .and_then(|c| c.pointer(&format!("/securityContext/{name}")))
            .or_else(|| pod.pointer(&format!("/spec/securityContext/{name}")))
    };
    let mut context = json!({
        "allowPrivilegeEscalation": false,
        "capabilities": {"drop": ["ALL"]},
        "seccompProfile": {"type": "RuntimeDefault"},
    });
    let non_root = field("runAsNonRoot").and_then(Value::as_bool) == Some(true);
    let user = field("runAsUser")
        .and_then(Value::as_i64)
        .or_else(|| non_root.then_some(NOBODY));
    if let Some(object) = context.as_object_mut() {
        if let Some(user) = user {
            object.insert("runAsUser".to_owned(), json!(user));
            if user != 0 {
                object.insert("runAsNonRoot".to_owned(), json!(true));
            }
        }
        if let Some(group) = field("runAsGroup").and_then(Value::as_i64) {
            object.insert("runAsGroup".to_owned(), json!(group));
        }
    }
    context
}

/// The plan that adds a debug container with `image` for `asked` (or the default container).
///
/// When one of Zeabur's still serves that container, the plan writes nothing and says so
/// ([`Warning::DebugContainerReused`]). `made_at` names the new container, so the plan and its
/// apply name it alike.
///
/// # Errors
///
/// When the pod has no such container, or has stopped.
pub fn debug_container(
    pod: &Value,
    asked: Option<&str>,
    image: &str,
    made_at: &str,
) -> Result<Plan, PlanError> {
    let pod_name = pod
        .pointer("/metadata/name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let target = target_container(pod, asked)?;
    if matches!(
        pod.pointer("/status/phase").and_then(Value::as_str),
        Some("Succeeded" | "Failed")
    ) {
        return Err(error(
            ErrorKind::Conflict,
            "pod_stopped",
            format!("Pod {pod_name} has stopped, so no container can start in it."),
        ));
    }
    if let Some(reused) = debug_containers(pod)
        .into_iter()
        .find(|c| c.usable() && c.target.as_deref() == Some(target.as_str()))
    {
        return Ok(Plan {
            objects: Vec::new(),
            impact: Vec::new(),
            warnings: vec![Warning::DebugContainerReused {
                pod: pod_name,
                container: reused.name,
                target,
            }],
        });
    }
    let uid = pod
        .pointer("/metadata/uid")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let digest = sha256(format!("{uid}\n{target}\n{made_at}").as_bytes());
    let name = format!("{DEBUG_PREFIX}{}", hex(digest.get(..3).unwrap_or_default()));
    let container = json!({
        "name": name,
        "image": image,
        "imagePullPolicy": "IfNotPresent",
        "command": ["sh"],
        "stdin": true,
        "tty": true,
        "targetContainerName": target,
        "terminationMessagePolicy": "File",
        "securityContext": debug_security_context(pod, &target),
    });
    let existing = pod.pointer("/spec/ephemeralContainers").cloned();
    let mut operations = vec![json!({"op": "test", "path": "/metadata/uid", "value": uid})];
    match &existing {
        Some(list) => {
            operations
                .push(json!({"op": "test", "path": "/spec/ephemeralContainers", "value": list}));
            operations.push(
                json!({"op": "add", "path": "/spec/ephemeralContainers/-", "value": container}),
            );
        }
        None => {
            operations.push(
                json!({"op": "add", "path": "/spec/ephemeralContainers", "value": [container]}),
            );
        }
    }
    let mut after = pod.clone();
    let mut list = existing
        .and_then(|l| l.as_array().cloned())
        .unwrap_or_default();
    list.push(container);
    if let Some(spec) = after.get_mut("spec").and_then(Value::as_object_mut) {
        spec.insert("ephemeralContainers".to_owned(), Value::Array(list));
    }
    let before = shown(pod);
    let shown_after = shown(&after);
    Ok(Plan {
        objects: vec![PlannedObject {
            target: Target::of::<k8s_openapi::api::core::v1::Pod>(&pod_name),
            before_yaml: yaml(Some(&before)),
            after_yaml: yaml(Some(&shown_after)),
            changes: diff(Some(pod), &after),
            before: Some(before),
            after: Some(shown_after),
            owners: Vec::new(),
            resource_version: pod
                .pointer("/metadata/resourceVersion")
                .and_then(Value::as_str)
                .map(str::to_owned),
            write: Write::EphemeralContainers { operations },
            raw_after: after,
        }],
        impact: Vec::new(),
        warnings: vec![Warning::DebugContainerStays {
            pod: pod_name,
            container: name,
            target,
            image: image.to_owned(),
        }],
    })
}

/// The requests and limits of the tools container, as quantities by resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolsResources {
    /// `resources.requests`.
    pub requests: BTreeMap<String, String>,
    /// `resources.limits`.
    pub limits: BTreeMap<String, String>,
}

impl Default for ToolsResources {
    /// Enough for `kubectl` and `helm` on a large cluster's discovery, little when idle.
    fn default() -> Self {
        let quantities = |pairs: [(&str, &str); 2]| {
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect()
        };
        Self {
            requests: quantities([("cpu", "10m"), ("memory", "64Mi")]),
            limits: quantities([("cpu", "1"), ("memory", "512Mi")]),
        }
    }
}

/// What a tools pod is made from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolsPod<'a> {
    /// The namespace it goes in.
    pub namespace: &'a str,
    /// The project it is opened from: the namespace `kubectl` starts in.
    pub project: &'a str,
    /// The person's user name.
    pub person: &'a str,
    /// Whether the credential in use now is an administrator's.
    pub admin: bool,
    /// When that credential expires, RFC 3339; `None` when Zeabur does not know.
    pub credential_expires_at: Option<String>,
    /// The image, with `kubectl` and `helm`.
    pub image: &'a str,
    /// `activeDeadlineSeconds`.
    pub deadline_seconds: i64,
    /// How long after its last terminal closes the pod is deleted.
    pub grace_seconds: i64,
    /// Its container's requests and limits.
    pub resources: &'a ToolsResources,
    /// When the plan was made; names the pod, so the plan and its apply name it alike.
    pub made_at: &'a str,
}

/// Whether `pod` is a tools pod of the person whose [`person_key`] is `key`.
#[must_use]
pub fn is_tools_pod_of(pod: &Value, key: &str) -> bool {
    let label = |name: &str| {
        pod.pointer(&format!("/metadata/labels/{}", name.replace('/', "~1")))
            .and_then(Value::as_str)
    };
    label(TOOLS_LABEL) == Some("true") && label(TOOLS_PERSON_LABEL) == Some(key)
}

/// The tools pod `request` describes, as it is created.
#[must_use]
pub fn tools_pod_body(request: &ToolsPod<'_>) -> Value {
    let key = person_key(request.person);
    let digest = sha256(format!("{}\n{}", request.person, request.made_at).as_bytes());
    let name = format!(
        "{TOOLS_PREFIX}{key}-{}",
        hex(digest.get(..3).unwrap_or_default())
    );
    let home = TOOLS_HOME;
    json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": name,
            "namespace": request.namespace,
            "labels": {
                "app.kubernetes.io/name": "zeabur-tools",
                "app.kubernetes.io/managed-by": "zeabur",
                TOOLS_LABEL: "true",
                TOOLS_PERSON_LABEL: key,
            },
            "annotations": {
                TOOLS_PERSON_ANNOTATION: request.person,
                TOOLS_PROJECT_ANNOTATION: request.project,
            },
        },
        "spec": {
            "automountServiceAccountToken": false,
            "enableServiceLinks": false,
            "restartPolicy": "Never",
            "activeDeadlineSeconds": request.deadline_seconds,
            "terminationGracePeriodSeconds": 5,
            "securityContext": {
                "runAsUser": TOOLS_USER,
                "runAsGroup": TOOLS_USER,
                "runAsNonRoot": true,
                "fsGroup": TOOLS_USER,
                "seccompProfile": {"type": "RuntimeDefault"},
            },
            "containers": [{
                "name": TOOLS_CONTAINER,
                "image": request.image,
                "imagePullPolicy": "IfNotPresent",
                "command": ["sleep", request.deadline_seconds.to_string()],
                "workingDir": home,
                "env": [
                    {"name": "HOME", "value": home},
                    {"name": "KUBECONFIG", "value": format!("{home}/.kube/config")},
                    {"name": "KUBECACHEDIR", "value": format!("{home}/.cache/kube")},
                    {"name": "PATH", "value": format!("{home}/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin")},
                ],
                "resources": {
                    "requests": request.resources.requests,
                    "limits": request.resources.limits,
                },
                "securityContext": {
                    "allowPrivilegeEscalation": false,
                    "readOnlyRootFilesystem": true,
                    "runAsNonRoot": true,
                    "capabilities": {"drop": ["ALL"]},
                },
                "volumeMounts": [
                    {"name": "home", "mountPath": home},
                    {"name": "credential", "mountPath": format!("{home}/.kube")},
                    {"name": "tmp", "mountPath": "/tmp"},
                    {"name": "kube-api", "mountPath": "/var/run/zeabur/kube-api", "readOnly": true},
                ],
            }],
            "volumes": [
                {"name": "home", "emptyDir": {"sizeLimit": "256Mi"}},
                {"name": "credential", "emptyDir": {"medium": "Memory", "sizeLimit": "1Mi"}},
                {"name": "tmp", "emptyDir": {"sizeLimit": "256Mi"}},
                {"name": "kube-api", "projected": {
                    "defaultMode": 0o444,
                    "sources": [{"configMap": {
                        "name": "kube-root-ca.crt",
                        "items": [{"key": "ca.crt", "path": "ca.crt"}],
                    }}],
                }},
            ],
        },
    })
}

/// The plan that creates the tools pod `request` describes.
#[must_use]
pub fn tools_pod(request: &ToolsPod<'_>) -> Plan {
    let body = tools_pod_body(request);
    let name = body
        .pointer("/metadata/name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let after = shown(&body);
    Plan {
        objects: vec![PlannedObject {
            target: Target::of::<k8s_openapi::api::core::v1::Pod>(&name),
            before: None,
            before_yaml: None,
            after_yaml: yaml(Some(&after)),
            after: Some(after),
            changes: diff(None, &body),
            owners: Vec::new(),
            resource_version: None,
            write: Write::Create { body: body.clone() },
            raw_after: body,
        }],
        impact: Vec::new(),
        warnings: vec![
            Warning::ToolsPod {
                namespace: request.namespace.to_owned(),
                pod: name,
                image: request.image.to_owned(),
                person: request.person.to_owned(),
                admin: request.admin,
                credential_expires_at: request.credential_expires_at.clone(),
                deadline_seconds: request.deadline_seconds,
                grace_seconds: request.grace_seconds,
            },
            Warning::ToolsCredentialExposed {
                namespace: request.namespace.to_owned(),
            },
        ],
    }
}

/// The plan when the person's tools pod `pod` in `namespace` is still there: nothing to write.
#[must_use]
pub fn tools_pod_reused(namespace: &str, pod: &str) -> Plan {
    Plan {
        objects: Vec::new(),
        impact: Vec::new(),
        warnings: vec![Warning::ToolsPodReused {
            namespace: namespace.to_owned(),
            pod: pod.to_owned(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pod() -> Value {
        json!({
            "apiVersion": "v1", "kind": "Pod",
            "metadata": {"name": "ksm-1", "namespace": "monitoring", "uid": "u-1",
                         "resourceVersion": "7"},
            "spec": {
                "securityContext": {"runAsNonRoot": true},
                "containers": [
                    {"name": "kube-state-metrics", "image": "ksm:2.15",
                     "securityContext": {"runAsUser": 65534, "readOnlyRootFilesystem": true}},
                    {"name": "sidecar", "image": "side:1"}
                ]
            },
            "status": {"phase": "Running"}
        })
    }

    #[test]
    fn a_debug_container_shares_the_targets_processes_and_runs_as_its_user() {
        let plan = debug_container(&pod(), None, "busybox:1.37.0", "2026-10-21T00:00:00Z").unwrap();
        let [object] = plan.objects.as_slice() else {
            panic!("one write: {plan:?}")
        };
        let Write::EphemeralContainers { operations } = &object.write else {
            panic!("{:?}", object.write)
        };
        assert_eq!(
            operations[0],
            json!({"op": "test", "path": "/metadata/uid", "value": "u-1"})
        );
        let added = &operations[1]["value"][0];
        assert!(added["name"].as_str().unwrap().starts_with(DEBUG_PREFIX));
        assert_eq!(added["targetContainerName"], "kube-state-metrics");
        assert_eq!(added["stdin"], true);
        assert_eq!(added["tty"], true);
        assert_eq!(added["image"], "busybox:1.37.0");
        let context = &added["securityContext"];
        assert_eq!(context["runAsUser"], 65534);
        assert_eq!(context["runAsNonRoot"], true);
        assert_eq!(context["allowPrivilegeEscalation"], false);
        assert_eq!(context["capabilities"]["drop"], json!(["ALL"]));
        assert!(matches!(
            plan.warnings.as_slice(),
            [Warning::DebugContainerStays { target, .. }] if target == "kube-state-metrics"
        ));
    }

    #[test]
    fn a_pod_that_must_not_run_as_root_and_names_no_user_gets_nobody() {
        let mut pod = pod();
        pod["spec"]["containers"][1]["name"] = json!("only");
        let plan = debug_container(&pod, Some("only"), "busybox:1.37.0", "t").unwrap();
        let Write::EphemeralContainers { operations } = &plan.objects[0].write else {
            panic!()
        };
        assert_eq!(
            operations[1]["value"][0]["securityContext"]["runAsUser"],
            NOBODY
        );
    }

    #[test]
    fn the_plan_and_its_apply_name_the_container_alike_and_another_plan_does_not() {
        let name = |made_at| {
            let plan = debug_container(&pod(), None, "b", made_at).unwrap();
            plan.objects[0].raw_after["spec"]["ephemeralContainers"][0]["name"].clone()
        };
        assert_eq!(name("t1"), name("t1"));
        assert_ne!(name("t1"), name("t2"));
    }

    #[test]
    fn a_running_or_starting_debug_container_for_the_target_is_used_again() {
        let mut pod = pod();
        pod["spec"]["ephemeralContainers"] = json!([
            {"name": "zeabur-debug-aaaaaa", "image": "b", "targetContainerName": "sidecar"},
            {"name": "zeabur-debug-bbbbbb", "image": "b", "targetContainerName": "kube-state-metrics"},
            {"name": "debugger-x", "image": "b", "targetContainerName": "kube-state-metrics"}
        ]);
        pod["status"]["ephemeralContainerStatuses"] = json!([
            {"name": "zeabur-debug-bbbbbb", "state": {"waiting": {"reason": "ContainerCreating"}}}
        ]);
        let plan = debug_container(&pod, None, "b", "t").unwrap();
        assert_eq!(plan.objects, []);
        assert!(matches!(
            plan.warnings.as_slice(),
            [Warning::DebugContainerReused { container, .. }] if container == "zeabur-debug-bbbbbb"
        ));

        pod["status"]["ephemeralContainerStatuses"] = json!([
            {"name": "zeabur-debug-bbbbbb", "state": {"terminated": {"reason": "Completed", "exitCode": 0}}}
        ]);
        let plan = debug_container(&pod, None, "b", "t").unwrap();
        let Write::EphemeralContainers { operations } = &plan.objects[0].write else {
            panic!()
        };
        assert_eq!(operations[1]["op"], "test");
        assert_eq!(operations[2]["path"], "/spec/ephemeralContainers/-");
    }

    #[test]
    fn a_stopped_pod_or_an_unknown_container_is_refused() {
        let mut stopped = pod();
        stopped["status"]["phase"] = json!("Succeeded");
        assert_eq!(
            debug_container(&stopped, None, "b", "t").unwrap_err().code,
            "pod_stopped"
        );
        assert_eq!(
            debug_container(&pod(), Some("nope"), "b", "t")
                .unwrap_err()
                .code,
            "no_such_container"
        );
    }

    fn tools(person: &str, admin: bool) -> ToolsPod<'static> {
        ToolsPod {
            namespace: "shop",
            project: "shop",
            person: Box::leak(person.to_owned().into_boxed_str()),
            admin,
            credential_expires_at: Some("2026-10-21T00:10:00Z".to_owned()),
            image: "alpine/k8s:1.36.5",
            deadline_seconds: 1800,
            grace_seconds: 60,
            resources: Box::leak(Box::new(ToolsResources::default())),
            made_at: "2026-10-21T00:00:00Z",
        }
    }

    #[test]
    fn a_tools_pod_has_no_service_account_token_and_no_privileges() {
        let body = tools_pod_body(&tools("alice@example.com", false));
        let spec = &body["spec"];
        assert_eq!(spec["automountServiceAccountToken"], false);
        assert_eq!(spec["activeDeadlineSeconds"], 1800);
        assert_eq!(spec["securityContext"]["runAsNonRoot"], true);
        assert_eq!(
            spec["securityContext"]["seccompProfile"]["type"],
            "RuntimeDefault"
        );
        let container = &spec["containers"][0];
        assert_eq!(container["securityContext"]["readOnlyRootFilesystem"], true);
        assert_eq!(
            container["securityContext"]["allowPrivilegeEscalation"],
            false
        );
        assert_eq!(
            container["securityContext"]["capabilities"]["drop"],
            json!(["ALL"])
        );
        let credential = spec["volumes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == "credential")
            .unwrap();
        assert_eq!(credential["emptyDir"]["medium"], "Memory");
        assert!(
            container["volumeMounts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m["name"] == "credential" && m["mountPath"] == "/home/user/.kube")
        );
        assert!(is_tools_pod_of(&body, &person_key("alice@example.com")));
        assert!(!is_tools_pod_of(&body, &person_key("bob@example.com")));
    }

    #[test]
    fn a_tools_plan_says_who_it_acts_as_and_that_the_token_can_be_used_from_the_pod() {
        let plan = tools_pod(&tools("alice", true));
        assert!(matches!(&plan.objects[0].write, Write::Create { .. }));
        assert!(matches!(
            plan.warnings.as_slice(),
            [
                Warning::ToolsPod { person, admin: true, deadline_seconds: 1800, .. },
                Warning::ToolsCredentialExposed { namespace },
            ] if person == "alice" && namespace == "shop"
        ));
    }
}
