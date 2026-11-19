---
default: major
---

# The server says which component installs a missing tool

`BackupProvider`, `Notifications`, `DatabaseOperator`, `ProjectObjectStorage` and `ProjectBuilds` add `component`: the catalogue component that installs the tool, with its id and name. `GET /cluster/backups` adds `providers` with K8up and Velero, each with its state and component. A missing capability on `GET /cluster/capabilities` adds `install`: a recommended component that runs on the cluster, else the first that does. A component incident names the component as Zeabur Catalog does, so a down ingress controller is now `ingress` instead of `ingress-controller`. The web no longer keeps its own component ids or picks a component for a capability.
