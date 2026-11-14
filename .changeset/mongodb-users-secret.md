---
default: patch
---

# A MongoDB database shows the user and password the operator wrote

A new MongoDB database names its users Secret `<name>-secrets`, and the database page and `zeaburctl connect` read the Secret the cluster names. A cluster that names none is read at the operator's own default, `percona-server-mongodb-users`, which is where its users really are.
