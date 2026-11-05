---
default: patch
---

# Doctor names the component that owns a broken workload

When several components share a namespace, `zeabur-installer doctor` blames an unavailable Deployment or DaemonSet on the component whose Helm release made it, including the ones an operator creates from that release's objects, instead of on every component in the namespace.
