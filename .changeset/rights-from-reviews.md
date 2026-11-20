---
default: major
---

# Rights come from Kubernetes reviews, not from matching rules

A project is read-only when its rules review grants no write. Each action, the Admin and Viewer roles and the cluster actions now come from access reviews, and cluster actions are checked cluster-wide. A `cluster-admin` bound only by a RoleBinding can no longer change nodes or cluster backups in the interface. `GET /projects/{p}/namespace` drops `can` and `workloadPatch` and adds `readOnly`, `canReadLogs` and `canReadSecrets`. On `GET /projects/{p}/creatable`, a kind's `access` is now `unknown` unless the project is read-only. `GET /projects/{p}/objects` answers 403 only when the person can list no kind in the project. Clients of the old fields must read the new ones.
