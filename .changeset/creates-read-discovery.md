---
default: patch
---

# Every create checks the kinds the cluster serves

A plan that creates an object from a form, such as a database, a bucket or a backup, now reads the cluster's API discovery, as a create from YAML already did. A kind the cluster does not serve, for example a database whose operator is not installed, is refused when the plan is made instead of when it is applied. Zeabur no longer keeps its own list of kinds to fall back on. When discovery cannot be read, the plan is refused.
