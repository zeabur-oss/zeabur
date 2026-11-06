---
default: patch
---

# A failed chart install or upgrade stops `apply` and says how to see why

`apply` used to wait 15 minutes and then only say the chart was not done. It now stops as soon as Helm marks the release failed. It says to see why with `helm history`, from a machine that has helm and can reach the cluster, since the installer doesn't install helm. Fix the cause, run `helm rollback`, or `helm uninstall` when the first install failed, then run `apply` again.
