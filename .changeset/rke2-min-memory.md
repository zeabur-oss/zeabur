---
default: patch
---

# A team install needs 3500 MiB on K3s and 4300 MiB on RKE2, and Zeabur Builder 500 MiB more

The minimums now count what the scheduler counts: the memory requests of the distribution's own pods and the default components, on top of the memory kept for the system and the eviction threshold. Below them pods stayed Pending after the install. A 4 GiB server fits K3s once its crash kernel reserve is off, and the Server step greys RKE2 out on it.

On K3s and RKE2 the installer adds the catalog's `workMemoryMiB` of Zeabur Builder, the memory of one build, to that minimum, and leaves the builder out on a server below it, as a personal install already does. So a 4 GiB server installs K3s without the builder.

An install that already runs is upgraded on a server below the new minimum, with a warning that suggests turning off the crash kernel reserve, and keeps the components it has.
