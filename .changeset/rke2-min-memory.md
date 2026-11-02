---
default: patch
---

# RKE2 needs a server that reports at least 4000 MiB of memory, and Zeabur Builder 500 MiB more

With the memory kept for the system, RKE2's control plane and the default components no longer fit a 4 GiB server, so the installer refuses RKE2 below 4000 MiB and the Server step greys it out, instead of leaving pods Pending after the install. K3s still fits 4 GiB.

On K3s and RKE2 the installer adds the catalog's `workMemoryMiB` of Zeabur Builder, the memory of one build, to that minimum, and leaves the builder out on a server below it, as a personal install already does.
