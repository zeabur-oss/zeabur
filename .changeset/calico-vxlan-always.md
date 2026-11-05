---
default: patch
---

# Webhooks reach pods on other K3s nodes

Calico on K3s now sends all traffic between nodes through VXLAN. Before, a node reached a pod on another node in the same subnet from its own address, which the baseline network policy blocks, so webhook calls from the API server timed out and `apply` failed. A cluster installed earlier keeps the old setting; Add nodes → Common failures has the command that changes it.
