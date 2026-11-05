---
default: patch
---

# Doctor no longer asks a single node to run node sync

On a one-node cluster, `zeabur-installer doctor` warned that the network policies did not let the node's tunnel address in. A node's traffic to its own pods needs no rule, so the baseline policies now list node addresses only when the cluster has more than one node.
