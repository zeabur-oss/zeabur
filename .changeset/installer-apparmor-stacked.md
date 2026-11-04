---
default: patch
---

# Containers can signal their own processes on Ubuntu 26.04

On Ubuntu 26.04 with kernel 7.0.0-41 or later, containers on K3s, RKE2 and KubeSolo could not send signals to their own processes, so PostgreSQL and other images that rely on them hung at start. Where `kernel.apparmor_restrict_unprivileged_unconfined` is 1, the installer now writes and loads `/etc/apparmor.d/cri-containerd.apparmor.d` before the cluster starts: containerd's default profile with the rules that let such containers signal and trace each other. On a cluster that is already running, the next `apply` restarts K3s, RKE2 or KubeSolo once. `uninstall --purge` removes the file.
