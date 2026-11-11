---
default: patch
---

# The in-memory test cluster answers from RBAC rules

No change for users. The cluster that the API tests run against now decides reviews, writes, dry runs and commands in a pod from the same RBAC rules.
