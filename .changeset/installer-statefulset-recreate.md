---
default: patch
---

# Switching profile no longer leaves zot's chart failed

When a chart upgrade changes a StatefulSet field Kubernetes can't update, such as zot's volume size after a `profile` change, `apply` deletes the StatefulSet without its Pods and disks and upgrades again; the disks keep their size. A re-run of `apply` also gets through a release an earlier run left failed this way. `plan` says which StatefulSets this may affect, and for a release that failed for another reason, which `kubectl` command lets helm-controller retry it.
