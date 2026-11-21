---
default: patch
---

# Components read from Zeabur Catalog 0.5.6

Zeabur OSS reads the components of Zeabur Catalog 0.5.6 again. Their health checks of a Deployment, StatefulSet or DaemonSet name no condition, because kstatus decides when a workload is ready, and Zeabur OSS no longer requires one.
