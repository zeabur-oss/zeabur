---
default: patch
---

# Components ask for the memory they were measured to use

On a single node, each default component's memory request follows what it used on a 4 GB K3s server. VictoriaLogs asks for less memory, leaves out Kubernetes' own namespaces and keeps kube-apiserver's audit log.
