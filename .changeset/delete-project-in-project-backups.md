---
default: patch
---

# Delete project tells which backups go with the project

The Delete project review now lists backups kept in object storage inside the project under what goes, with a warning to copy them or switch to another S3 destination first. Before, it always said "Backups in S3" stay, so deleting a project whose database backed up to its own object storage lost every backup. The review shows the backups that stay, and the restore line, only when such backups exist.
