---
default: patch
---

# A team install needs 2700 MiB on K3s and 3500 MiB on RKE2

The minimums count what the scheduler counts: the memory requests of the distribution's own pods, of Zeabur, of Zeabur Identity and of the components, on top of the memory kept for the system and the eviction threshold. Below them pods stayed Pending after the install. The requests come from the catalog's measurements, Zeabur's chart and Zeabur Identity's release, not from numbers in the installer.

Components that need room for work, Zeabur Builder, VictoriaMetrics and VictoriaLogs, are left out on a server where they and their work would not fit, as a personal install already does. Zeabur Builder needs 3900 MiB on K3s and 4800 MiB on RKE2. So a 4 GiB server installs K3s or RKE2 without the builder.

An install that already runs is upgraded on a server below the new minimum, with a warning that suggests turning off the crash kernel reserve, and keeps the components it has.
