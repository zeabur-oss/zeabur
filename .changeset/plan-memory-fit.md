---
default: patch
---

# plan warns when the memory reservation leaves a node short for its Pods

Zeabur 0.2 reserves memory for the system and K3s or RKE2, which shrinks what Pods can request. On a server that was nearly full, Pods could stay `Pending` with `Insufficient memory`, while `plan` reported nothing. On an upgrade, `plan` now compares what each node's Pods already request with what's left for Pods once the reservation applies, and warns and names the node when they don't fit. To make room, delete a service on that node or lower its replicas, move a service to another node, or add a node.
