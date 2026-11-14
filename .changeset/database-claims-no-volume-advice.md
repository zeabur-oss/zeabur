---
default: patch
---

# A database operator's volumes no longer get volume-backup advice

Missing setup suggested a K8up or Velero volume backup for each volume of a database that an operator runs, such as a CloudNativePG `Cluster` or a `MariaDB`. Copying the files of a running database does not give a consistent backup, and the database has its own backups. These volumes now have no row of their own under Backups and no volume-backup advice. Other volumes keep their advice, and a CloudNativePG database without a `ScheduledBackup` still shows its warning.
