---
default: minor
---

# `zeabur-installer` marks components the catalog did not test on the server's distribution

`plan`, `doctor` and the installer's pages mark a component that was not tested on K3s, RKE2 or KubeSolo as untested, and `plan` installs it with a warning. `doctor` and `apply` run the health checks the catalog ships with each component, and `doctor` prints the catalog's advice when one fails.
