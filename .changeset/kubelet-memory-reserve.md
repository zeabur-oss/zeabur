---
default: patch
---

# A K3s or RKE2 server that runs out of memory stops a pod, not the cluster

`zeabur-installer` keeps memory for the OS and the distribution from pods with the kubelet's `system-reserved` and `kube-reserved`, and evicts pods when less than 400 MiB stays free for 30 seconds or less than 200 MiB at once. A build that runs out of memory fails alone instead of taking down K3s and every pod with it. On RKE2, etcd and kube-controller-manager ask for less memory, so the components still fit a 4 GiB server. The Options step shows how much memory is kept for the system. Running `zeabur-installer apply` again writes the new settings to an existing server and restarts K3s or RKE2; on a worker, run `zeabur-installer join` again.
