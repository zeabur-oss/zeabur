//! The kubelet's answers when it cannot give a container's log.
//!
//! kube-apiserver does not read logs itself: it forwards `pods/log` to the kubelet of the pod's
//! node and passes a refusal on as `400 BadRequest` with the kubelet's text as the message
//! [apiserver-checker]. The kubelet refuses when the container has no log to give
//! [kubelet-pods]: the previous run was asked for but the container has not restarted
//! ("previous terminated container … not found"), the container is still waiting ("is waiting to
//! start"), or the container runtime no longer has that run's container ("unable to retrieve
//! container logs for containerd://…") [kuberuntime]. The kubelet keeps only the latest dead
//! containers of a pod [kubelet-gc], so the run before a long crash loop's last restart is often
//! gone. The kubelet's handler answers a refusal with `400` [kubelet-server].
//!
//! [apiserver-checker]: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/apiserver/pkg/registry/generic/rest/response_checker.go
//! [kubelet-pods]: https://github.com/kubernetes/kubernetes/blob/master/pkg/kubelet/kubelet_pods.go
//! [kuberuntime]: https://github.com/kubernetes/kubernetes/blob/master/pkg/kubelet/kuberuntime/kuberuntime_container.go
//! [kubelet-gc]: https://kubernetes.io/docs/concepts/architecture/garbage-collection/#container-image-garbage-collection
//! [kubelet-server]: https://github.com/kubernetes/kubernetes/blob/master/pkg/kubelet/server/server.go

use zeabur_core::problem::UnavailableLog;

/// Why the kubelet could not give a log, when `text`, the message of a `400` answer, is one of
/// its refusals. Anything else is `None`.
#[must_use]
pub fn unavailable_log(text: &str) -> Option<UnavailableLog> {
    let text = text.trim();
    if text.starts_with("unable to retrieve container logs for ") && text.contains("://") {
        Some(UnavailableLog::Removed)
    } else if text.starts_with("previous terminated container ") && text.ends_with(" not found") {
        Some(UnavailableLog::NoPreviousRun)
    } else if text.starts_with("container ") {
        if text.contains(" is waiting to start") || text.ends_with(" is not available") {
            Some(UnavailableLog::NotStarted)
        } else if text.ends_with(" is terminated") {
            Some(UnavailableLog::Removed)
        } else {
            None
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use zeabur_core::problem::UnavailableLog;

    use super::unavailable_log;

    #[test]
    fn the_kubelets_refusals_are_told_apart_from_log_lines() {
        let cases = [
            (
                "unable to retrieve container logs for containerd://4b1c0e9f",
                Some(UnavailableLog::Removed),
            ),
            (
                "previous terminated container \"web\" in pod \"web-1\" not found",
                Some(UnavailableLog::NoPreviousRun),
            ),
            (
                "container \"web\" in pod \"web-1\" is waiting to start: ContainerCreating",
                Some(UnavailableLog::NotStarted),
            ),
            (
                "container \"web\" in pod \"web-1\" is waiting to start: image can't be pulled",
                Some(UnavailableLog::NotStarted),
            ),
            (
                "container \"web\" in pod \"web-1\" is terminated",
                Some(UnavailableLog::Removed),
            ),
            ("listening on :8080", None),
            ("container web started", None),
        ];
        for (text, expected) in cases {
            assert_eq!(unavailable_log(text), expected, "{text}");
        }
    }
}
