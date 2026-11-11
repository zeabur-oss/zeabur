---
default: patch
---

# Cluster actions are decided by a cluster-wide access review

The Cluster page now asks the cluster whether you may change nodes and set up backups with an access review that is not tied to a namespace. A role that is granted only in one namespace, such as `kube-system`, no longer opens these actions. Rules are matched as Kubernetes RBAC does, so a rule limited to object names never allows creating a new object, and `*/log` allows only the log of every resource.
