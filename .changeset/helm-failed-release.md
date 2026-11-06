---
default: patch
---

# A failed chart install or upgrade stops `apply` and says how to see why

`apply` used to wait 15 minutes and then only report that the chart wasn't ready. It now stops as soon as Helm marks the release failed and tells you to run `helm history` on a machine that has Helm installed and can reach the cluster. The installer doesn't install Helm. Fix the cause, run `helm rollback` (or `helm uninstall` if the first install failed), then run `apply` again.
