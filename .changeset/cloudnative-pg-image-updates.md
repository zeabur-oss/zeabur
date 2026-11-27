---
default: major
---

# Reuse automatic image update settings for CloudNativePG databases can now check their PostgreSQL image tag on an hourly, daily or weekly schedule. Enable `databaseUpdater.enabled` in the Helm chart, then choose a schedule in the database settings. The updater runs from the Zeabur image and CloudNativePG rolls out the new digest.

The `autoUpdate.workload` API type is now `AutoUpdateTarget`, which accepts a service `WorkloadRef` or a CloudNativePG `Cluster`. Clients reading plans in rollout, timeline and trace responses must handle Cluster targets separately from service workloads.
