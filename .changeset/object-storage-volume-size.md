---
default: patch
---

# Object storage fits its volumes to its disk

A new object storage sets the size and number of its SeaweedFS volumes from the disk size you choose, and growing the disk adjusts them. Before, SeaweedFS made volumes of up to 30 GB, so a store on a small disk had one volume. Every bucket needs volumes of its own, so each upload to a bucket failed with `InternalError`, and PostgreSQL backups to it failed.
