---
default: patch
---

# New object storage sets CPU and memory for its SeaweedFS servers

Zeabur now writes CPU and memory requests and a memory limit for the master, volume, filer and S3 servers of a new object store, so the store adds no missing-requests suggestion to the project. A store made before keeps its settings.
