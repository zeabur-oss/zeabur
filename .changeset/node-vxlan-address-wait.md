---
default: patch
---

# The baseline network policies use each node's VXLAN address from the first apply

On a new K3s install, `apply` could write the baseline network policies before Calico wrote the node's VXLAN address. The policies then listed the first address of the node's replica network range instead, and `zeabur-installer doctor` reported that they did not let the node's VXLAN address in yet. `apply` and `node sync` now wait up to two minutes for every node's VXLAN address, and on K3s the policies use only the address Calico gives the node.
