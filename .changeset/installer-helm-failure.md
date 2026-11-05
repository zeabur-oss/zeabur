---
default: patch
---

# A failed chart upgrade rolls back and says why

`apply` tries a failed chart upgrade once more, unless Kubernetes refused the chart's objects, and doesn't run a refused upgrade again with the same chart and values. If it fails again, it rolls the chart back to its last deployed revision and sets the `HelmChart` back to that revision's chart and values, so the previous version keeps running and helm-controller doesn't try the failed upgrade again. It then stops with Helm's reason, the `HelmChart` conditions and the end of the install Job's log, instead of waiting 15 minutes. `apply` counts a chart as installed only once Helm has deployed a new revision for it. helm-controller now tries an install Job 5 times instead of 1000.
