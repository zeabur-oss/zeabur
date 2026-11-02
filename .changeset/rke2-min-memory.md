---
default: patch
---

# RKE2 needs a server that reports at least 4200 MiB of memory

With the memory kept for the system, RKE2's control plane, the default components and one build no longer fit a 4 GiB server, so the installer refuses RKE2 below 4200 MiB and the Server step greys it out, instead of leaving pods Pending after the install. K3s still fits 4 GiB.
