---
default: patch
---

# Longhorn's node check passes after apply

Longhorn reports a node whose `dm_crypt` module is not loaded, so `apply` now loads it with `iscsi_tcp` and at every boot.
