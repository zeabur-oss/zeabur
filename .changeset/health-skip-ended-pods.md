---
default: patch
---

# Doctor and apply skip pods that have ended

After a full disk the kubelet leaves Completed and Evicted pods behind. `zeabur-installer doctor` and `apply` no longer wait for them to become Ready, so a component whose live pods are Running passes once the disk is free again.
