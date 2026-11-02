---
default: minor
---

# Add a private address from the Networking tab

A Deployment, StatefulSet or DaemonSet with no Service gets an **Add a private address** form on its Networking tab. It plans a ClusterIP Service named like the service, selecting its pods by `matchLabels` and carrying every port its containers declare, with the port filled in from the readiness probe or a declared port. The `kubectl` command is under **Use kubectl instead**.
