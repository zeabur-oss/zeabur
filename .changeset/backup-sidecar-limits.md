---
default: patch
---

# A PostgreSQL backup can no longer take all of a small server's CPU

New PostgreSQL backups give the Barman Cloud sidecar a CPU limit of half a CPU and compress with snappy instead of gzip, so the API server keeps answering on a 1 vCPU server while a backup runs. Backups set up before keep their settings.
