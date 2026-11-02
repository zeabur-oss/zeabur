---
default: patch
---

# The installer chooses components from the configuration file alone

`zeabur-installer plan` no longer reads the install record to choose components: a default the record lacks is installed, and an old component ID in `enable` or `disable` stops the plan and names the new ID. The record still shows what `apply` adds and removes; `container-registry/zot` shows as removed and `builds/zeabur-builder` as added.
