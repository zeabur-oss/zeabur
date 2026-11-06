---
default: patch
---

# plan warns when an upgrade leaves a node short of memory for its Pods

Zeabur 0.2 reserves memory for the system and K3s or RKE2, which shrinks what Pods can request. On a server that was nearly full, a component the upgrade adds or changes could stay `Pending` with `Insufficient memory`, while `plan` reported nothing. On an upgrade, `plan` now adds the memory requests of the components it adds or changes to what each node's Pods already request. When a node goes over what's left for Pods, it warns and names the node. To make room, delete a service on that node or lower its replicas, move a service to another node, or add a node.
