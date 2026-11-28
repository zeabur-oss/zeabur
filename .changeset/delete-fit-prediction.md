---
default: patch
---

# Zeabur no longer predicts before apply whether new pods fit

The template form, the Error pages review and the restore plan no longer say whether the cluster has room. `GET /api/v1/cluster/template-rooms` and the `room` field of a restore plan are removed. After apply, the service shows the scheduler's reason when a pod cannot be scheduled. The nodes page still shows the room each node has left.
