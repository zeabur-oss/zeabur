---
default: major
---

# The plan review no longer predicts whether new pods fit

The plan preview used to add up requests against quotas and the nodes' free room, and could warn "Not enough room" before you applied. That guess missed what the host uses, affinity, taints and LimitRange defaults, so it could be wrong either way. The preview now says only what is certain: which services restart or stop, the data that goes and the fields that are removed. If a pod can't start after you apply, the service shows the real reason from the cluster.

API: `Preview` loses `fit` and `unreadable`, and the `Fit`, `FitStatus`, `Blocker` and `BlockerKind` schemas are removed.
