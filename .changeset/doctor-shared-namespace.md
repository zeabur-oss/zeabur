---
default: patch
---

# Doctor reports a broken workload once

`zeabur-installer doctor` lists an unavailable Deployment or DaemonSet once, under its namespace. Before, it was repeated under every component that installs into that namespace, such as the four in `observability`.
