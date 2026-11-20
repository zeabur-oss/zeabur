---
default: minor
---

# Install components from the command line

`zeabur components install <id>...` installs catalog components the same way **Cluster** → **Components** does, as the pod's service account. It waits until every step and health check is ready, or until `--timeout` passes. A component that something other than Zeabur installed stays as it is.
