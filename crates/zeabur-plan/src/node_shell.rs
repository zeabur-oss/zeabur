//! A root shell on a node, as `kubectl debug node/<node> -it --profile=sysadmin` followed by
//! `nsenter` gives one [kubectl-debug-node].
//!
//! A pod is pinned to the node (`nodeName`, so the scheduler, a cordon and taints do not keep it
//! off) and tolerates every taint; it is privileged and shares the host's process, network and IPC
//! namespaces, as the `sysadmin` profile makes it [kubectl-profiles]. The terminal runs
//! [`command`] in it through `pods/exec`, which enters the mount, UTS, IPC, network and process
//! namespaces of the host's PID 1 [nsenter] and starts a login shell there, with the environment
//! variable [`MARKER`] naming the pod.
//!
//! # It ends with the session
//!
//! Zeabur deletes the pod when the terminal closes. In case Zeabur or the browser is gone first,
//! the container's own command ([`WATCH`]) ends it too: it waits [`FIRST_SESSION_SECONDS`] for a
//! process whose environment holds this pod's [`MARKER`] (`hostPID` shows it every process of the
//! host, a shell's children included), then exits once there is none. The value is the pod's name
//! between slashes, which no pod name holds, so another pod's shell or any other process cannot
//! keep it running. `restartPolicy` is `Never`: the pod completes, and nothing can enter it again.
//! `activeDeadlineSeconds` ([`DEADLINE_SECONDS`]) stops it in any case.
//!
//! # Only cluster administrators
//!
//! Such a pod is refused by the `baseline` and `restricted` Pod Security Standards [k8s-pss], so
//! it goes in a namespace of its own, [`NAMESPACE`], whose `enforce` level is `privileged`
//! [k8s-psa-labels]. Whoever may create pods there, or run commands in them, is root on every
//! node, so the namespace is never a project and no member role is bound in it. Zeabur reads no
//! other bindings: a subject that may run commands in pods of every namespace [k8s-rbac] can enter
//! it too. The pod's labels name the node and, as a [`person_key`], who opened it
//! ([`is_opened_by`]); the full user name is an annotation.
//!
//! Opening one also records an `events.k8s.io/v1` Event about the node [k8s-event], so
//! `kubectl describe node` shows who opened a shell on it and when.
//!
//! # References
//!
//! - [kubectl-debug-node] The Kubernetes Authors, "Debugging a Kubernetes Node with kubectl debug,"
//!   Kubernetes Documentation. <https://kubernetes.io/docs/tasks/debug/debug-cluster/kubectl-node-debug/>
//! - [kubectl-profiles] The Kubernetes Authors, "Debugging profiles," in "Debug Running Pods,"
//!   Kubernetes Documentation. <https://kubernetes.io/docs/tasks/debug/debug-application/debug-running-pod/#debugging-profiles>
//! - [nsenter] "`nsenter(1)`," Linux man-pages. <https://man7.org/linux/man-pages/man1/nsenter.1.html>
//! - [k8s-pss] The Kubernetes Authors, "Pod Security Standards," Kubernetes Documentation. <https://kubernetes.io/docs/concepts/security/pod-security-standards/>
//! - [k8s-psa-labels] The Kubernetes Authors, "Enforce Pod Security Standards with Namespace
//!   Labels," Kubernetes Documentation. <https://kubernetes.io/docs/tasks/configure-pod-container/enforce-standards-namespace-labels/>
//! - [k8s-rbac] The Kubernetes Authors, "Using RBAC Authorization," Kubernetes Documentation. <https://kubernetes.io/docs/reference/access-authn-authz/rbac/>
//! - [k8s-event] The Kubernetes Authors, "Event (events.k8s.io/v1)," Kubernetes API Reference. <https://kubernetes.io/docs/reference/kubernetes-api/cluster-resources/event-v1/>
//!
//! [kubectl-debug-node]: https://kubernetes.io/docs/tasks/debug/debug-cluster/kubectl-node-debug/
//! [kubectl-profiles]: https://kubernetes.io/docs/tasks/debug/debug-application/debug-running-pod/#debugging-profiles
//! [nsenter]: https://man7.org/linux/man-pages/man1/nsenter.1.html
//! [k8s-pss]: https://kubernetes.io/docs/concepts/security/pod-security-standards/
//! [k8s-psa-labels]: https://kubernetes.io/docs/tasks/configure-pod-container/enforce-standards-namespace-labels/
//! [k8s-rbac]: https://kubernetes.io/docs/reference/access-authn-authz/rbac/
//! [k8s-event]: https://kubernetes.io/docs/reference/kubernetes-api/cluster-resources/event-v1/

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{
    Container, Namespace, ObjectReference, Pod, PodSpec, ResourceRequirements, SecurityContext,
    Toleration,
};
use k8s_openapi::api::events::v1::Event;
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{MicroTime, ObjectMeta};
use k8s_openapi::jiff::{SignedDuration, Timestamp};
use serde_json::{Value, json};
use zeabur_model::kernel::digest::{hex, sha256};
use zeabur_model::kernel::names::FIELD_MANAGER;

use crate::json::value_of;
pub use crate::terminal::person_key;

/// The namespace node shells go in; never a project.
pub const NAMESPACE: &str = zeabur_model::kernel::names::NODE_SHELL_NAMESPACE;
/// The start of a node shell pod's name, followed by the node's.
pub const PREFIX: &str = "zeabur-node-shell-";
/// The label every node shell pod carries, `"true"`.
pub const LABEL: &str = "zeabur.com/node-shell";
/// The label naming the node, or a hash of its name when it is too long for a label.
pub const NODE_LABEL: &str = "zeabur.com/node-shell-node";
/// The label naming who opened it, as a [`person_key`].
pub const PERSON_LABEL: &str = "zeabur.com/node-shell-person";
/// The annotation with the user name of who opened it.
pub const PERSON_ANNOTATION: &str = "zeabur.com/node-shell-person";
/// The annotation a terminal sets when it enters, so a second one is refused.
pub const SESSION_ANNOTATION: &str = "zeabur.com/node-shell-session";
/// The pod's one container.
pub const CONTAINER: &str = "shell";
/// How long a node shell pod runs at most.
pub const DEADLINE_SECONDS: i64 = 1800;
/// How long a node shell pod waits for its first session.
pub const FIRST_SESSION_SECONDS: u32 = 120;
/// How often the pod looks for its sessions, in seconds.
pub const WATCH_INTERVAL_SECONDS: u32 = 2;
/// The environment variable that marks a session's processes with the pod's name.
pub const MARKER: &str = "ZEABUR_NODE_SHELL";
/// The namespace the Event about the node goes in, as the kubelet's own node events do.
pub const EVENT_NAMESPACE: &str = "default";
/// The Event's `reason`.
pub const EVENT_REASON: &str = "NodeShellOpened";
/// The login shell a session runs on the host: `bash` where the host has it.
pub const SHELL: &str = "exec bash -l 2>/dev/null || exec sh -l";
/// The container's command, run as `sh -c WATCH node-shell <pod> <first> <proc> <interval>`.
///
/// It waits up to `$2` seconds for a process whose environment under `$3` holds the pod's
/// [`MARKER`], then exits once none is left, looking every `$4` seconds.
pub const WATCH: &str = r#"trap 'exit 0' TERM INT
in_use() { grep -qsF "ZEABUR_NODE_SHELL=/$1/" "$3"/[0-9]*/environ; }
waited=0
until in_use "$@"; do
  [ "$waited" -ge "$2" ] && exit 0
  sleep "$4"
  waited=$((waited + $4))
done
while in_use "$@"; do sleep "$4"; done
"#;

const ENFORCE: &str = "pod-security.kubernetes.io/enforce";
const PRIVILEGED: &str = "privileged";
const MAX_NAME: usize = 253;
const MAX_LABEL: usize = 63;

/// What a node shell pod is made from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeShell<'a> {
    /// The node.
    pub node: &'a str,
    /// The node's `metadata.uid`, for the Event.
    pub node_uid: Option<&'a str>,
    /// The person's user name.
    pub person: &'a str,
    /// The image; it needs `sh`, `grep`, `sleep` and `nsenter`.
    pub image: &'a str,
    /// When the plan was made; names the pod, so the plan and its apply name it alike.
    pub made_at: &'a str,
}

/// `value` when it can be a label value, else the first 16 hexadecimal digits of its SHA-256.
fn label_value(value: &str) -> String {
    let fits = value.len() <= MAX_LABEL
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && value.starts_with(|c: char| c.is_ascii_alphanumeric())
        && value.ends_with(|c: char| c.is_ascii_alphanumeric());
    if fits {
        value.to_owned()
    } else {
        hex(sha256(value.as_bytes()).get(..8).unwrap_or_default())
    }
}

/// The pod's name: [`PREFIX`], the node's name, and six hexadecimal digits for this plan.
#[must_use]
pub fn pod_name(request: &NodeShell<'_>) -> String {
    let digest =
        sha256(format!("{}\n{}\n{}", request.node, request.person, request.made_at).as_bytes());
    let id = hex(digest.get(..3).unwrap_or_default());
    let room = MAX_NAME - PREFIX.len() - id.len() - 1;
    let node: String = request.node.chars().take(room).collect();
    let node = node.trim_end_matches(['-', '.']);
    format!("{PREFIX}{node}-{id}")
}

/// The value of [`MARKER`] for `pod`'s sessions.
#[must_use]
pub fn marker_value(pod: &str) -> String {
    format!("/{pod}/")
}

/// What a session runs in `pod`'s container: [`SHELL`] in the host's namespaces, marked as
/// `pod`'s.
#[must_use]
pub fn command(pod: &str) -> Vec<String> {
    let marker = format!("{MARKER}={}", marker_value(pod));
    [
        "nsenter", "-t", "1", "-m", "-u", "-i", "-n", "-p", "--", "env", &marker, "sh", "-c", SHELL,
    ]
    .map(str::to_owned)
    .to_vec()
}

fn watch_command(pod: &str, proc: &str, interval: u32) -> Vec<String> {
    vec![
        "sh".to_owned(),
        "-c".to_owned(),
        WATCH.to_owned(),
        "node-shell".to_owned(),
        pod.to_owned(),
        FIRST_SESSION_SECONDS.to_string(),
        proc.to_owned(),
        interval.to_string(),
    ]
}

/// The node shell pod `request` describes, as it is created.
#[must_use]
pub fn pod_body(request: &NodeShell<'_>) -> Value {
    let pod = pod_name(request);
    value_of(&Pod {
        metadata: ObjectMeta {
            name: Some(pod.clone()),
            namespace: Some(NAMESPACE.to_owned()),
            labels: Some(BTreeMap::from([
                (
                    "app.kubernetes.io/name".to_owned(),
                    PREFIX.trim_end_matches('-').to_owned(),
                ),
                (
                    "app.kubernetes.io/managed-by".to_owned(),
                    FIELD_MANAGER.to_owned(),
                ),
                (LABEL.to_owned(), "true".to_owned()),
                (NODE_LABEL.to_owned(), label_value(request.node)),
                (PERSON_LABEL.to_owned(), person_key(request.person)),
            ])),
            annotations: Some(BTreeMap::from([(
                PERSON_ANNOTATION.to_owned(),
                request.person.to_owned(),
            )])),
            ..ObjectMeta::default()
        },
        spec: Some(PodSpec {
            node_name: Some(request.node.to_owned()),
            host_pid: Some(true),
            host_network: Some(true),
            host_ipc: Some(true),
            dns_policy: Some("ClusterFirstWithHostNet".to_owned()),
            tolerations: Some(vec![Toleration {
                operator: Some("Exists".to_owned()),
                ..Toleration::default()
            }]),
            restart_policy: Some("Never".to_owned()),
            active_deadline_seconds: Some(DEADLINE_SECONDS),
            termination_grace_period_seconds: Some(5),
            automount_service_account_token: Some(false),
            enable_service_links: Some(false),
            containers: vec![Container {
                name: CONTAINER.to_owned(),
                image: Some(request.image.to_owned()),
                image_pull_policy: Some("IfNotPresent".to_owned()),
                command: Some(watch_command(&pod, "/proc", WATCH_INTERVAL_SECONDS)),
                security_context: Some(SecurityContext {
                    privileged: Some(true),
                    ..SecurityContext::default()
                }),
                resources: Some(ResourceRequirements {
                    requests: Some(BTreeMap::from([
                        ("cpu".to_owned(), Quantity("10m".to_owned())),
                        ("memory".to_owned(), Quantity("16Mi".to_owned())),
                    ])),
                    ..ResourceRequirements::default()
                }),
                ..Container::default()
            }],
            ..PodSpec::default()
        }),
        ..Pod::default()
    })
}

/// The namespace node shells go in, as it is created.
#[must_use]
pub fn namespace_body() -> Value {
    value_of(&Namespace {
        metadata: ObjectMeta {
            name: Some(NAMESPACE.to_owned()),
            labels: Some(BTreeMap::from([
                (
                    "app.kubernetes.io/managed-by".to_owned(),
                    FIELD_MANAGER.to_owned(),
                ),
                (ENFORCE.to_owned(), PRIVILEGED.to_owned()),
            ])),
            ..ObjectMeta::default()
        },
        ..Namespace::default()
    })
}

/// The JSON Patch that lets privileged pods into `namespace`, and the namespace as it will be;
/// `None` when they are let in already.
#[must_use]
pub fn namespace_patch(namespace: &Value) -> Option<(Vec<Value>, Value)> {
    let labels = namespace.pointer("/metadata/labels");
    if labels.and_then(|l| l.get(ENFORCE)).and_then(Value::as_str) == Some(PRIVILEGED) {
        return None;
    }
    let mut operations = Vec::new();
    if let Some(version) = namespace
        .pointer("/metadata/resourceVersion")
        .and_then(Value::as_str)
    {
        operations
            .push(json!({"op": "test", "path": "/metadata/resourceVersion", "value": version}));
    }
    let mut after = namespace.clone();
    if labels.and_then(Value::as_object).is_some() {
        operations.push(json!({"op": "add", "path": "/metadata/labels/pod-security.kubernetes.io~1enforce", "value": PRIVILEGED}));
        if let Some(labels) = after
            .pointer_mut("/metadata/labels")
            .and_then(Value::as_object_mut)
        {
            labels.insert(ENFORCE.to_owned(), json!(PRIVILEGED));
        }
    } else {
        let labels = json!({ENFORCE: PRIVILEGED});
        operations.push(json!({"op": "add", "path": "/metadata/labels", "value": labels}));
        if let Some(metadata) = after.get_mut("metadata").and_then(Value::as_object_mut) {
            metadata.insert("labels".to_owned(), labels);
        }
    }
    Some((operations, after))
}

/// The Event that records `request`'s shell on its node, opened `at` (RFC 3339). An `at` that is
/// not a time leaves `eventTime` out, which kube-apiserver refuses.
#[must_use]
pub fn event_body(request: &NodeShell<'_>, at: &str) -> Value {
    let pod = pod_name(request);
    value_of(&Event {
        metadata: ObjectMeta {
            name: Some(pod.clone()),
            namespace: Some(EVENT_NAMESPACE.to_owned()),
            labels: Some(BTreeMap::from([(LABEL.to_owned(), "true".to_owned())])),
            ..ObjectMeta::default()
        },
        event_time: at.parse().ok().map(MicroTime),
        reporting_controller: Some("zeabur.com/node-shell".to_owned()),
        reporting_instance: Some(FIELD_MANAGER.to_owned()),
        action: Some("OpenShell".to_owned()),
        reason: Some(EVENT_REASON.to_owned()),
        type_: Some("Normal".to_owned()),
        regarding: Some(ObjectReference {
            api_version: Some("v1".to_owned()),
            kind: Some("Node".to_owned()),
            name: Some(request.node.to_owned()),
            uid: request.node_uid.map(str::to_owned),
            ..ObjectReference::default()
        }),
        related: Some(ObjectReference {
            api_version: Some("v1".to_owned()),
            kind: Some("Pod".to_owned()),
            namespace: Some(NAMESPACE.to_owned()),
            name: Some(pod.clone()),
            ..ObjectReference::default()
        }),
        note: Some(format!(
            "{} opened a root shell on this node in pod {NAMESPACE}/{pod}.",
            request.person
        )),
        ..Event::default()
    })
}

/// The `kubectl` command that opens the same shell.
#[must_use]
pub fn kubectl_command(node: &str, image: &str) -> String {
    format!(
        "kubectl debug node/{node} --namespace {NAMESPACE} -it --profile=sysadmin --image={image} \
         -- nsenter -t 1 -m -u -i -n -p -- sh -c '{SHELL}'"
    )
}

/// Whether `pod` is a node shell pod of Zeabur's.
#[must_use]
pub fn is_node_shell(pod: &Value) -> bool {
    pod.pointer("/metadata/labels")
        .and_then(|l| l.get(LABEL))
        .and_then(Value::as_str)
        == Some("true")
}

/// The JSON Patch that marks `pod` as entered at `at`, or `None` when a terminal already did.
///
/// One terminal per shell: closing a terminal deletes the pod, which would end any other. The
/// patch tests the pod's `resourceVersion`, so of two terminals entering at once only one wins.
#[must_use]
pub fn claim(pod: &Value, at: &str) -> Option<Vec<Value>> {
    let annotations = pod.pointer("/metadata/annotations");
    if annotations
        .and_then(|a| a.get(SESSION_ANNOTATION))
        .is_some()
    {
        return None;
    }
    let mut operations = Vec::new();
    if let Some(version) = pod.pointer("/metadata/resourceVersion") {
        operations
            .push(json!({"op": "test", "path": "/metadata/resourceVersion", "value": version}));
    }
    if annotations.is_none() {
        operations.push(json!({"op": "add", "path": "/metadata/annotations", "value": {}}));
    }
    operations.push(json!({
        "op": "add",
        "path": format!("/metadata/annotations/{}", SESSION_ANNOTATION.replace('/', "~1")),
        "value": at,
    }));
    Some(operations)
}

/// Whether `pod` is a node shell pod `username` opened.
#[must_use]
pub fn is_opened_by(pod: &Value, username: &str) -> bool {
    is_node_shell(pod)
        && pod
            .pointer("/metadata/labels")
            .and_then(|l| l.get(PERSON_LABEL))
            .and_then(Value::as_str)
            == Some(person_key(username).as_str())
}

/// How a node shell pod is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellState {
    /// Not running yet; the kubelet's reason and message while its container waits.
    Starting {
        /// `state.waiting.reason` (`ContainerCreating`, `ErrImagePull`, …).
        reason: Option<String>,
        /// `state.waiting.message`.
        message: Option<String>,
    },
    /// A terminal can open in it.
    Running,
    /// It stopped, or is being deleted; `DeadlineExceeded` when its hour ran out.
    Ended {
        /// `status.reason`, or the container's `state.terminated.reason`.
        reason: Option<String>,
    },
}

/// A node shell pod, as the node's page lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellPod {
    /// Its name.
    pub name: String,
    /// The node it is pinned to.
    pub node: String,
    /// Who opened it.
    pub person: Option<String>,
    /// When it was created.
    pub created_at: Option<Timestamp>,
    /// When its deadline stops it: from its start, or its creation until then.
    pub stops_at: Option<Timestamp>,
    /// How it is doing.
    pub state: ShellState,
}

fn text(object: &Value, path: &str) -> Option<String> {
    object
        .pointer(path)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn state_of(pod: &Value) -> ShellState {
    let container = pod
        .pointer("/status/containerStatuses")
        .and_then(Value::as_array)
        .and_then(|list| {
            list.iter()
                .find(|c| c.get("name").and_then(Value::as_str) == Some(CONTAINER))
        });
    let phase = pod.pointer("/status/phase").and_then(Value::as_str);
    if pod.pointer("/metadata/deletionTimestamp").is_some()
        || matches!(phase, Some("Succeeded" | "Failed"))
    {
        let reason = text(pod, "/status/reason")
            .or_else(|| container.and_then(|c| text(c, "/state/terminated/reason")));
        return ShellState::Ended { reason };
    }
    if let Some(container) = container {
        if container.pointer("/state/running").is_some() {
            return ShellState::Running;
        }
        if container.pointer("/state/terminated").is_some() {
            return ShellState::Ended {
                reason: text(container, "/state/terminated/reason"),
            };
        }
        return ShellState::Starting {
            reason: text(container, "/state/waiting/reason"),
            message: text(container, "/state/waiting/message"),
        };
    }
    ShellState::Starting {
        reason: None,
        message: None,
    }
}

/// A time field of `object`, by JSON pointer; `None` when missing or not an RFC 3339 time.
fn time(object: &Value, path: &str) -> Option<Timestamp> {
    object
        .pointer(path)
        .and_then(Value::as_str)
        .and_then(|t| t.parse().ok())
}

/// The node shell pods among `pods`, oldest first.
#[must_use]
pub fn shells(pods: &[Value]) -> Vec<ShellPod> {
    let mut found: Vec<ShellPod> = pods
        .iter()
        .filter(|p| is_node_shell(p))
        .filter_map(|pod| {
            let created_at = time(pod, "/metadata/creationTimestamp");
            let deadline = pod
                .pointer("/spec/activeDeadlineSeconds")
                .and_then(Value::as_i64)
                .unwrap_or(DEADLINE_SECONDS);
            let stops_at = time(pod, "/status/startTime")
                .or(created_at)
                .and_then(|at| at.checked_add(SignedDuration::from_secs(deadline)).ok());
            Some(ShellPod {
                name: text(pod, "/metadata/name")?,
                node: text(pod, "/spec/nodeName")?,
                person: pod
                    .pointer("/metadata/annotations")
                    .and_then(|a| a.get(PERSON_ANNOTATION))
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                created_at,
                stops_at,
                state: state_of(pod),
            })
        })
        .collect();
    found.sort_by(|a, b| (a.created_at, &a.name).cmp(&(b.created_at, &b.name)));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(node: &'static str) -> NodeShell<'static> {
        NodeShell {
            node,
            node_uid: Some("node-uid-1"),
            person: "zeabur:alice@example.com",
            image: "busybox:1.37.0",
            made_at: "2026-10-23T05:00:00Z",
        }
    }

    #[test]
    fn the_pod_is_pinned_to_the_node_privileged_in_the_hosts_namespaces_and_stops_in_an_hour() {
        let body = pod_body(&request("vps-1"));
        let spec = &body["spec"];
        assert_eq!(spec["nodeName"], "vps-1");
        assert_eq!(spec["hostPID"], true);
        assert_eq!(spec["hostNetwork"], true);
        assert_eq!(spec["hostIPC"], true);
        assert_eq!(spec["activeDeadlineSeconds"], 1800);
        assert_eq!(spec["restartPolicy"], "Never");
        assert_eq!(spec["tolerations"], json!([{"operator": "Exists"}]));
        assert_eq!(spec["automountServiceAccountToken"], false);
        let container = &spec["containers"][0];
        assert_eq!(container["name"], CONTAINER);
        assert_eq!(container["securityContext"]["privileged"], true);
        assert_eq!(container["image"], "busybox:1.37.0");
        let name = pod_name(&request("vps-1"));
        assert_eq!(
            container["command"],
            json!(["sh", "-c", WATCH, "node-shell", name, "120", "/proc", "2"])
        );
        assert_eq!(body["metadata"]["namespace"], NAMESPACE);
    }

    #[test]
    fn the_pod_is_labelled_with_its_node_and_who_opened_it() {
        let body = pod_body(&request("vps-1"));
        let labels = &body["metadata"]["labels"];
        assert_eq!(labels[LABEL], "true");
        assert_eq!(labels[NODE_LABEL], "vps-1");
        assert_eq!(labels[PERSON_LABEL], person_key("zeabur:alice@example.com"));
        assert_eq!(
            body["metadata"]["annotations"][PERSON_ANNOTATION],
            "zeabur:alice@example.com"
        );
        let name = body["metadata"]["name"].as_str().unwrap();
        assert!(name.starts_with("zeabur-node-shell-vps-1-"), "{name}");
        assert_eq!(name.len(), "zeabur-node-shell-vps-1-".len() + 6);
    }

    #[test]
    fn a_long_node_name_keeps_the_pod_name_and_labels_within_their_limits() {
        let node: &'static str =
            Box::leak(format!("{}.example.com", "n".repeat(240)).into_boxed_str());
        let body = pod_body(&request(node));
        let name = body["metadata"]["name"].as_str().unwrap();
        assert!(name.len() <= 253, "{}", name.len());
        let label = body["metadata"]["labels"][NODE_LABEL].as_str().unwrap();
        assert_eq!(label.len(), 16);
    }

    #[test]
    fn the_plan_and_its_apply_name_the_pod_alike_and_another_plan_does_not() {
        let mut later = request("vps-1");
        later.made_at = "2026-10-23T05:00:01Z";
        assert_eq!(pod_name(&request("vps-1")), pod_name(&request("vps-1")));
        assert_ne!(pod_name(&request("vps-1")), pod_name(&later));
    }

    #[test]
    fn the_namespace_lets_privileged_pods_in() {
        let body = namespace_body();
        assert_eq!(body["metadata"]["name"], NAMESPACE);
        assert_eq!(body["metadata"]["labels"][ENFORCE], "privileged");
        assert!(namespace_patch(&body).is_none());

        let restricted = json!({"metadata": {"name": NAMESPACE, "resourceVersion": "4",
            "labels": {ENFORCE: "baseline", "keep": "me"}}});
        let (operations, after) = namespace_patch(&restricted).unwrap();
        assert_eq!(
            operations[0],
            json!({"op": "test", "path": "/metadata/resourceVersion", "value": "4"})
        );
        assert_eq!(after["metadata"]["labels"][ENFORCE], "privileged");
        assert_eq!(after["metadata"]["labels"]["keep"], "me");

        let bare = json!({"metadata": {"name": NAMESPACE}});
        let (operations, after) = namespace_patch(&bare).unwrap();
        assert_eq!(operations[0]["path"], "/metadata/labels");
        assert_eq!(after["metadata"]["labels"][ENFORCE], "privileged");
    }

    #[test]
    fn the_event_names_the_node_who_opened_the_shell_and_when() {
        let event = event_body(&request("vps-1"), "2026-10-23T05:00:07Z");
        assert_eq!(event["regarding"]["kind"], "Node");
        assert_eq!(event["regarding"]["name"], "vps-1");
        assert_eq!(event["regarding"]["uid"], "node-uid-1");
        assert_eq!(event["eventTime"], "2026-10-23T05:00:07.000000Z");
        assert_eq!(event["reason"], EVENT_REASON);
        assert!(
            event["note"]
                .as_str()
                .unwrap()
                .starts_with("zeabur:alice@example.com opened a root shell")
        );
        assert_eq!(event["related"]["name"], pod_name(&request("vps-1")));
    }

    #[test]
    fn the_kubectl_command_opens_the_same_shell() {
        assert_eq!(
            kubectl_command("vps-1", "busybox:1.37.0"),
            "kubectl debug node/vps-1 --namespace zeabur-node-shell -it --profile=sysadmin \
             --image=busybox:1.37.0 -- nsenter -t 1 -m -u -i -n -p -- sh -c \
             'exec bash -l 2>/dev/null || exec sh -l'"
        );
    }

    #[test]
    fn a_session_runs_a_login_shell_on_the_host_marked_as_the_pods() {
        assert_eq!(
            command("zeabur-node-shell-vps-1-abc123"),
            [
                "nsenter",
                "-t",
                "1",
                "-m",
                "-u",
                "-i",
                "-n",
                "-p",
                "--",
                "env",
                "ZEABUR_NODE_SHELL=/zeabur-node-shell-vps-1-abc123/",
                "sh",
                "-c",
                "exec bash -l 2>/dev/null || exec sh -l",
            ]
        );
        assert!(WATCH.contains(&format!("{MARKER}=/$1/")));
    }

    /// A process table of `environ` files, as `/proc` shows it.
    struct FakeProc(std::path::PathBuf);

    impl FakeProc {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!("zeabur-node-shell-{name}-{}", std::process::id()));
            let _fresh = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            Self(root)
        }

        fn process(&self, pid: u32, environment: &[&str]) {
            let dir = self.0.join(pid.to_string());
            std::fs::create_dir_all(&dir).unwrap();
            let mut bytes = Vec::new();
            for variable in environment {
                bytes.extend_from_slice(variable.as_bytes());
                bytes.push(0);
            }
            std::fs::write(dir.join("environ"), bytes).unwrap();
        }

        fn end(&self, pid: u32) {
            std::fs::remove_dir_all(self.0.join(pid.to_string())).unwrap();
        }

        fn watch(&self, pod: &str, first_session: u32) -> std::process::Child {
            let mut command = watch_command(pod, self.0.to_str().unwrap(), 1);
            command[5] = first_session.to_string();
            std::process::Command::new(&command[0])
                .args(&command[1..])
                .spawn()
                .unwrap()
        }
    }

    impl Drop for FakeProc {
        fn drop(&mut self) {
            let _gone = std::fs::remove_dir_all(&self.0);
        }
    }

    fn exited_within(child: &mut std::process::Child, seconds: u64) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
        while std::time::Instant::now() < deadline {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let _stopped = child.kill();
        let _reaped = child.wait();
        false
    }

    #[test]
    fn the_pod_ends_when_no_session_came_or_the_last_one_left() {
        let proc = FakeProc::new("ends");
        proc.process(1, &["PATH=/usr/bin", "HOME=/root"]);
        let mut unused = proc.watch("shell-abc123", 0);
        assert!(exited_within(&mut unused, 3));

        proc.process(
            40,
            &[
                "TERM=xterm",
                "ZEABUR_NODE_SHELL=/shell-abc123/",
                "HOME=/root",
            ],
        );
        let mut used = proc.watch("shell-abc123", 0);
        assert!(!exited_within_quietly(&mut used, 2));
        proc.end(40);
        assert!(exited_within(&mut used, 4));
    }

    #[test]
    fn only_this_pods_sessions_keep_it_running() {
        let proc = FakeProc::new("others");
        proc.process(7, &["ZEABUR_NODE_SHELL=/shell-abc1234/"]);
        proc.process(8, &["ZEABUR_NODE_SHELL=shell-abc123"]);
        proc.process(9, &["X=ZEABUR_NODE_SHELL=/other/"]);
        let mut watch = proc.watch("shell-abc123", 1);
        assert!(exited_within(&mut watch, 4));
    }

    /// Whether `child` exited within `seconds`, leaving it running when it did not.
    fn exited_within_quietly(child: &mut std::process::Child, seconds: u64) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
        while std::time::Instant::now() < deadline {
            if child.try_wait().unwrap().is_some() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        false
    }

    #[test]
    fn only_the_person_who_opened_a_shell_may_enter_it() {
        let body = pod_body(&request("vps-1"));
        assert!(is_opened_by(&body, "zeabur:alice@example.com"));
        assert!(!is_opened_by(&body, "zeabur:bob@example.com"));
        let mut unlabelled = body;
        unlabelled["metadata"]["labels"]
            .as_object_mut()
            .unwrap()
            .remove(LABEL);
        assert!(!is_opened_by(&unlabelled, "zeabur:alice@example.com"));
    }

    #[test]
    fn a_shell_is_claimed_once_and_only_at_the_version_read() {
        let mut body = pod_body(&request("vps-1"));
        body["metadata"]["resourceVersion"] = json!("7");
        let operations = claim(&body, "2026-10-23T05:00:00Z").unwrap();
        assert_eq!(
            operations.first(),
            Some(&json!({"op": "test", "path": "/metadata/resourceVersion", "value": "7"}))
        );
        let mut claimed = body.clone();
        claimed["metadata"]["annotations"][SESSION_ANNOTATION] = json!("2026-10-23T05:00:00Z");
        assert_eq!(
            crate::json::apply_patch(&body, &operations).unwrap(),
            claimed
        );
        assert_eq!(claim(&claimed, "later"), None);
        let mut moved = body.clone();
        moved["metadata"]["resourceVersion"] = json!("8");
        assert!(crate::json::apply_patch(&moved, &operations).is_err());
        let mut bare = body;
        bare["metadata"]
            .as_object_mut()
            .unwrap()
            .remove("annotations");
        let applied = crate::json::apply_patch(&bare, &claim(&bare, "now").unwrap()).unwrap();
        assert_eq!(
            applied["metadata"]["annotations"][SESSION_ANNOTATION],
            "now"
        );
    }

    fn pod(status: &Value) -> Value {
        let mut body = pod_body(&request("vps-1"));
        body["metadata"]["creationTimestamp"] = json!("2026-10-23T05:00:00Z");
        body["status"] = status.clone();
        body
    }

    #[test]
    fn a_shell_pod_starts_runs_and_ends_at_its_deadline_without_failing() {
        let starting = pod(&json!({"phase": "Pending", "containerStatuses": [
            {"name": "shell", "state": {"waiting": {"reason": "ErrImagePull", "message": "no"}}}]}));
        let running = pod(
            &json!({"phase": "Running", "startTime": "2026-10-23T05:00:03Z",
            "containerStatuses": [{"name": "shell", "state": {"running": {}}}]}),
        );
        let expired = pod(&json!({"phase": "Failed", "reason": "DeadlineExceeded",
            "startTime": "2026-10-23T05:00:03Z"}));
        let found = shells(&[
            starting,
            running,
            expired,
            json!({"metadata": {"name": "other"}}),
        ]);
        assert_eq!(found.len(), 3);
        assert_eq!(
            found[0].state,
            ShellState::Starting {
                reason: Some("ErrImagePull".to_owned()),
                message: Some("no".to_owned())
            }
        );
        assert_eq!(
            found[0].stops_at,
            Some("2026-10-23T05:30:00Z".parse().unwrap())
        );
        assert_eq!(found[1].state, ShellState::Running);
        assert_eq!(
            found[1].stops_at,
            Some("2026-10-23T05:30:03Z".parse().unwrap())
        );
        assert_eq!(
            found[2].state,
            ShellState::Ended {
                reason: Some("DeadlineExceeded".to_owned())
            }
        );
        assert_eq!(found[2].node, "vps-1");
        assert_eq!(found[2].person.as_deref(), Some("zeabur:alice@example.com"));
    }
}
