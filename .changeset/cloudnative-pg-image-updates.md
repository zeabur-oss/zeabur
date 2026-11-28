---
default: major
---

# Schedule image updates for CloudNativePG databases

Enable `databaseUpdater.enabled` in the Helm chart and choose an hourly, daily or weekly check in the database settings. When the existing PostgreSQL tag points to a new image, Zeabur updates the digest and CloudNativePG rolls it out.

The `autoUpdate.workload` API type is now `AutoUpdateTarget`, which accepts a service `WorkloadRef` or a CloudNativePG `Cluster`. Clients reading plans in rollout, timeline and trace responses must handle Cluster targets separately from service workloads.
