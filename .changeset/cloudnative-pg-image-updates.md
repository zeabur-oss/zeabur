---
default: major
---

# Reuse automatic image update settings for CloudNativePG databases

CloudNativePG databases can opt into scheduled refreshes of their existing PostgreSQL image tag, including tag references pinned to a digest. Enable the optional `databaseUpdater` component in the Zeabur Helm chart. It reuses the unmodified Keel registry poller in a separate Zeabur controller; no patched Keel installation or catalog update is required. The form checks that component's readiness and reuses the service policy, time controls and annotation-only auto-update operation. Saving preserves the database spec and unrelated annotations, including last-applied configuration, while normal controller status updates do not invalidate the form. The updater changes only the image digest and update metadata; CloudNativePG performs the rollout.

The `autoUpdate.workload` API type is now `AutoUpdateTarget`, which accepts a service `WorkloadRef` or a CloudNativePG `Cluster`. Clients reading plans in rollout, timeline and trace responses must handle Cluster targets separately from service workloads.
