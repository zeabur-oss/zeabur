---
default: patch
---

# Doctor reports a broken workload once

`zeabur-installer doctor` lists an unavailable Deployment or DaemonSet once, under its namespace. Before, it was repeated under every component that installs into that namespace, such as the four in `observability`. A component's passing line now says `charts installed` (and `health checks pass` when the catalog has checks), since its workloads are reported under their namespace.
