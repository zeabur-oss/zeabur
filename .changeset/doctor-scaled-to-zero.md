---
default: patch
---

# Doctor reports a Deployment scaled to 0

`zeabur-installer doctor` counted a Deployment with no Pods wanted as available, so a component whose Deployments were scaled to 0 passed. It now reports `deployment/<name> is scaled to 0`.
