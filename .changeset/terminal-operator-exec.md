---
default: patch
---

# Open the cluster kubectl terminal and node terminals when an operator may run commands everywhere

The cluster kubectl terminal and the node terminal no longer refuse to open when a role binding lets someone besides cluster administrators run commands in their replicas. Operators such as CloudNativePG grant their ServiceAccount that right in every namespace, so on those clusters the terminals never opened. Such an account can already run commands in every replica, so the check protected little.
