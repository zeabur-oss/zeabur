---
default: minor
---

# `zeaburctl ui` can join the experience program

With `ZEABUR_TELEMETRY_ENABLED=true`, `zeaburctl ui` sends `cluster_connected`: whether it could connect to the cluster of your kubeconfig. It keeps a random ID in its configuration directory, made only when the first event is about to be sent; `zeaburctl telemetry reset` deletes it.
