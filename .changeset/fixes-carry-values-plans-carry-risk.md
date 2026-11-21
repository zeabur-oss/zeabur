---
default: major
---

# Plans say their risk and fixes carry their values

A plan now says how much its changes can hurt. `POST /projects/{project}/plan` returns `risk`, with a `level` (`low`, `normal` or `high`) and the `reasons` for it, each with the workloads it is about. A restart reads the ready pods when the plan is made, so every client confirms the same changes the same way.

A gap's fix names the `workload` it changes and, where Zeabur OSS can choose them, the `values` the plan sets: the requests and limits, the port of a readiness check, or the path and size of a new disk. The `service`, `container` and `mountPath` parameters of these fixes are gone; clients must read `workload` and `values` instead. A volume's backup fix names the workload that mounts the volume.

A diagnosis carries a `group`. Services whose diagnoses have the same group key fail the same way, and `sharedCause` says whether their cause can be said once for all of them.
