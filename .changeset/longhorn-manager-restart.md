---
default: patch
---

# apply restarts Longhorn's manager on a node it prepares

Longhorn's manager checks a node's packages and kernel modules only when it starts. When `apply` installed them on a node where Longhorn already ran, `doctor` kept reporting the node as not prepared until someone deleted its `longhorn-manager` Pod. `apply` now restarts that Pod after it prepares the node, on the server and on joined nodes. When the upgrade adds Longhorn to a cluster with other nodes, run the new installer's `apply` on each of them too.
