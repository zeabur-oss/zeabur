---
default: patch
---

# A restore copy fits only when each pod fits on one node

A restored copy used to fit when the nodes' free room added up to what its pods request. Each pod is now placed on one ready node that has its CPU, memory and a free pod slot, the largest pods first. Three nodes with 1 GiB free each no longer report room for a pod that requests 2 GiB.
