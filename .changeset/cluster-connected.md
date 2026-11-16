---
default: minor
---

# `zeaburctl ui` joins the experience program

`zeaburctl ui` joins the experience program by default and says so each time it starts. It sends `cluster_connected`: whether it could connect to the cluster of your kubeconfig. It keeps a random ID in its configuration directory, made only when the first event is about to be sent; `zeaburctl telemetry reset` deletes it. `DO_NOT_TRACK=1` or `telemetry: false` in `config.yaml` next to the ID turns it off.
