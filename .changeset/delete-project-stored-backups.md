---
default: patch
---

# Delete project lists every stored backup of the project

The Delete project review now finds a volume's backups from K8up's snapshot list, so backups of a stopped schedule are listed too. A database or a project backup is listed when any of its runs stored a backup, or when the database's store says it holds one, not only when one of its recent runs did. Each is told as going with the project, staying, or unknown, as before.
