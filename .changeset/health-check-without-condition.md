---
default: patch
---

# Components read from Zeabur Catalog 0.5.6

Zeabur reads the components of Zeabur Catalog 0.5.6 again. Their health checks of a Deployment, StatefulSet or DaemonSet name no condition, because kstatus decides when a workload is ready, and Zeabur no longer requires one.
