---
default: patch
---

# The Backups tab says when WAL archiving fails

A PostgreSQL database whose CloudNativePG `ContinuousArchiving` condition is false used to look like one that was still starting. The Backups tab now shows the operator's message and when archiving began to fail, and the disabled Back up now button says why.
