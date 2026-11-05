---
default: patch
---

# A failed chart upgrade stops `apply` and says how to see why, and the next `apply` retries it

Charts now use helm-controller's `failurePolicy: retry` instead of `abort`, so the install Job upgrades over a failed release. Before, a release left failed stayed stuck until someone rolled it back by hand. When an upgrade fails, `apply` stops at that chart instead of waiting 15 minutes. It sets the chart to `abort` so helm-controller stops retrying, and says to see why with `helm history`. Fix the cause and run `apply` again. It sets `retry` again and upgrades over the failed release. `apply` also restarts a finished install Job when it expects an upgrade, since helm-controller sometimes starts none. helm-controller now tries an install Job 3 times instead of 1000.
