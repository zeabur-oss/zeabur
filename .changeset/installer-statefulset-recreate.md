---
default: patch
---

# Switching profile no longer leaves zot's chart failed

The catalog lists the objects an upgrade must recreate, such as zot's StatefulSet when `profile` changes its volume size. `plan` names them, and `apply` deletes each one without its Pods and volumes before the upgrade, so Helm creates it again. The volumes keep their size. An upgrade that fails anyway stops `apply` with Helm's reason instead of waiting 15 minutes.
