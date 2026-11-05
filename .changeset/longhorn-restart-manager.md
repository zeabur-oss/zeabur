---
default: patch
---

# Doctor says how to refresh a Longhorn node's checks

Longhorn's manager reads a node's packages and kernel modules when it starts, so a node's conditions stay `False` after `zeabur-installer apply` fixed the node. `doctor` now tells the reader to restart that node's longhorn-manager Pod after `apply`, with the command.
