//! `CrowdSec` agents that stay down after their node restarts.
//!
//! The Pod keeps its name, its init container registers that name with the LAPI again, and the
//! LAPI refuses a machine that exists. Deleting the machine lets the next retry register.
//! Temporary: remove this once the chart reuses the Pod's own credentials
//! (`crowdsecurity/helm-charts#361` or an equivalent) and the catalog moves to that release.

use serde_json::Value;
use zeabur_installer_host::kubectl::Kubectl;
use zeabur_installer_host::preflight::Status;

use crate::{InstallError, out};

/// The catalog's component.
pub(crate) const COMPONENT: &str = "intrusion-prevention/crowdsec";
const NAMESPACE: &str = "crowdsec";
const LAPI: &str = "deploy/crowdsec-lapi";
const INIT_CONTAINER: &str = "wait-for-lapi-and-register";
const PODS: &str = "--selector=k8s-app=crowdsec,type in (agent,appsec)";

/// Whether the plan (as the install record keeps it) installs `CrowdSec`.
pub(crate) fn planned(plan: &Value) -> bool {
    plan.get("components")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|c| c.get("id").and_then(Value::as_str) == Some(COMPONENT))
}

/// The agent and `AppSec` Pods whose registration the LAPI refused because their machine exists.
fn refused(kubectl: &Kubectl) -> Result<Vec<String>, InstallError> {
    let pods = kubectl.run(
        &["get", "pods", "-n", NAMESPACE, PODS, "--output=json"],
        None,
    )?;
    let pods: Value =
        serde_json::from_str(&pods).map_err(|e| InstallError::Message(e.to_string()))?;
    let mut found = Vec::new();
    for pod in retrying(&pods) {
        let logs = |previous: &str| {
            kubectl
                .run(
                    &[
                        "logs",
                        "-n",
                        NAMESPACE,
                        &pod,
                        "-c",
                        INIT_CONTAINER,
                        previous,
                    ],
                    None,
                )
                .unwrap_or_default()
        };
        if already_registered(&pod, &logs("--previous=false"))
            || already_registered(&pod, &logs("--previous"))
        {
            found.push(pod);
        }
    }
    Ok(found)
}

/// The Pods in a list whose registering init container has failed and is not done.
fn retrying(pods: &Value) -> Vec<String> {
    pods.get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|pod| {
            pod.pointer("/status/initContainerStatuses")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|c| {
                    c.get("name").and_then(Value::as_str) == Some(INIT_CONTAINER)
                        && c.get("ready").and_then(Value::as_bool) != Some(true)
                        && c.get("restartCount").and_then(Value::as_u64).unwrap_or(0) > 0
                })
        })
        .filter_map(|pod| pod.pointer("/metadata/name").and_then(Value::as_str))
        .map(str::to_owned)
        .collect()
}

/// Whether the log has the LAPI's refusal of this Pod's own name, not of another machine.
fn already_registered(pod: &str, log: &str) -> bool {
    let refusal = format!("user '{pod}': user already exist");
    log.lines().any(|line| line.contains(&refusal))
}

fn delete_command(pod: &str) -> String {
    format!("kubectl -n {NAMESPACE} exec {LAPI} -- cscli machines delete {pod}")
}

/// Deletes the LAPI's machine of each refused agent, so its init container registers on the next
/// retry. A failure is a warning: the health checks and `doctor` still show the agent.
pub(crate) fn repair(kubectl: &Kubectl) {
    let pods = match refused(kubectl) {
        Ok(pods) => pods,
        Err(e) => {
            out::err(&format!("    warning: reading CrowdSec's agents: {e}"));
            return;
        }
    };
    for pod in pods {
        out::err(&format!(
            "    {pod} is refused by the LAPI as already registered; deleting its machine"
        ));
        let deleted = kubectl.run(
            &[
                "exec", "-n", NAMESPACE, LAPI, "--", "cscli", "machines", "delete", &pod,
            ],
            None,
        );
        if let Err(e) = deleted {
            out::err(&format!(
                "    warning: {e}; run `{}` on a server",
                delete_command(&pod)
            ));
        }
    }
}

/// `doctor`'s check of the agents.
pub(crate) fn check(kubectl: &Kubectl) -> Vec<(Status, String)> {
    match refused(kubectl) {
        Ok(pods) => pods
            .iter()
            .map(|pod| {
                (
                    Status::Fail,
                    format!(
                        "{COMPONENT}: the LAPI refuses {pod} because its machine is already \
                         registered. zeabur-installer apply deletes the machine, or run `{}`",
                        delete_command(pod)
                    ),
                )
            })
            .collect(),
        Err(e) => vec![(Status::Warn, format!("reading CrowdSec's agents: {e}"))],
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn only_the_refusal_of_the_pods_own_name_counts() {
        let log = "waiting for lapi to start\nError: cscli lapi register: api client register: \
                   http 403 Forbidden: API error: user 'crowdsec-agent-g22kh': user already exist";
        assert!(already_registered("crowdsec-agent-g22kh", log));
        assert!(!already_registered("crowdsec-agent-g22k", log));
        assert!(!already_registered("crowdsec-agent-x7p2q", log));
        assert!(!already_registered(
            "crowdsec-agent-g22kh",
            "waiting for lapi to start\nwaiting for lapi to start"
        ));
    }

    #[test]
    fn only_pods_whose_registration_failed_and_is_not_done_are_read() {
        let pod = |name: &str, init: Value| json!({"metadata": {"name": name}, "status": {"initContainerStatuses": [init]}});
        let pods = json!({"items": [
            pod("crowdsec-agent-down", json!({"name": INIT_CONTAINER, "ready": false, "restartCount": 7})),
            pod("crowdsec-agent-starting", json!({"name": INIT_CONTAINER, "ready": false, "restartCount": 0})),
            pod("crowdsec-agent-up", json!({"name": INIT_CONTAINER, "ready": true, "restartCount": 1})),
            pod("crowdsec-appsec-down", json!({"name": INIT_CONTAINER, "ready": false, "restartCount": 2})),
            json!({"metadata": {"name": "crowdsec-agent-pending"}, "status": {}}),
        ]});
        assert_eq!(
            retrying(&pods),
            ["crowdsec-agent-down", "crowdsec-appsec-down"]
        );
    }
}
