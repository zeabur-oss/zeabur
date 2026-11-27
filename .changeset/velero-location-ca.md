---
default: patch
---

# Cluster backups trust the CA of a self-hosted S3 store

S3 destinations and the cluster backup location take the CA certificates of a store whose certificate a private CA signs, such as a self-hosted MinIO. Velero trusts that CA through the Secret `zeabur-backup-ca` instead of the deprecated inline `caCert`, and writing the location no longer drops a CA reference set by hand.
