---
default: patch
---

# Components pick a fixed ClusterIP

Installing a component with an input that takes a fixed ClusterIP, such as `clusterIp` of Zeabur Builder, fills it with the lowest free address in the Service CIDR's static band when it is left empty. The install review shows the address, and a later install keeps it. Before, the input stayed empty, zot got a random address, and nodes could not pull built images.

If you installed Zeabur Builder before this release, its zot Service already has an assigned address that cannot change, so installing it again fails. Run `kubectl -n zot delete service zot` first: the install then creates the Service at the picked address. zot's pod and its images stay.
