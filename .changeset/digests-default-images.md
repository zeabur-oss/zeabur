---
default: patch
---

# digests.txt lists the images Zeabur runs by default

A release's `digests.txt` now also names the busybox image used for debug containers (`debug index`) and the `alpine/k8s` image used by terminals (`tools index`). Zeabur runs both without publishing them, so a registry mirror built from `digests.txt` alone left terminals stuck in `ImagePullBackOff`. The release checks that each is still served for `linux/amd64` and `linux/arm64`, and `verify` checks that `digests.txt` names the images the code pins. Zeabur does not sign these two.
