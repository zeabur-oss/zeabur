---
default: minor
---

# `zeabur-installer` installs only components the catalog tested on the server's distribution

`plan` leaves out a default component that was not tested on K3s, RKE2 or KubeSolo, and installs one the configuration asks for with a warning. `doctor` and `apply` run the health checks the catalog ships with each component, and `doctor` prints the catalog's advice when one fails.
