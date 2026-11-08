---
default: patch
---

# Missing setup no longer asks to back up the volumes an operator manages

A volume that an operator manages, such as one of a database or of a SeaweedFS storage cluster, got the advice to back it up on its own. A copy of each volume alone can be inconsistent, and the operator knows how to back up its data. These volumes now get no advice. A CloudNativePG database with no backup schedule keeps its own warning. The volumes are still listed under **Backups**.
