---
default: minor
---

# `zeaburctl ui` joins the experience program

`zeaburctl ui` joins the experience program by default and says so each time it starts. It sends `cluster_connected`: whether it could connect to the cluster of your kubeconfig. It keeps a random ID in its configuration directory, made only when the first event is about to be sent; `zeaburctl telemetry reset` deletes it. `zeaburctl telemetry status` shows whether it sends and which setting decides it; `zeaburctl telemetry disable` (or `DO_NOT_TRACK=1`) turns it off and `zeaburctl telemetry enable` turns it back on. Sending never delays the link or the browser.
