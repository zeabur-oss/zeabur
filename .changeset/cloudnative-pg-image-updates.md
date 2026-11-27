---
default: major
---

# Reuse automatic image update settings for CloudNativePG databases

CloudNativePG databases can opt into scheduled refreshes of their existing PostgreSQL image tag, including tag references pinned to a digest. This requires Keel with CloudNativePG support and its Cluster permissions; the currently pinned Keel 0.22.4 does not have that support. The form reuses the service policy and time controls and its annotation-only auto-update operation. Saving preserves the database spec and unrelated annotations, including last-applied configuration, while normal controller status updates do not invalidate the form.

The `autoUpdate.workload` API type is now `AutoUpdateTarget`, which accepts a service `WorkloadRef` or a CloudNativePG `Cluster`. Clients reading plans in rollout, timeline and trace responses must handle Cluster targets separately from service workloads.
