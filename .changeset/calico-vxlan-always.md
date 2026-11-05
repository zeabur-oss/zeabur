---
default: patch
---

# Pods on different K3s nodes reach each other on more networks

Calico on K3s now sends all traffic between nodes through VXLAN. Before, nodes in one subnet sent it with pod addresses, which networks that check addresses drop. A cluster installed earlier keeps the old setting; Add nodes → Common failures has the command that changes it.
