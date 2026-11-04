---
default: patch
---

# The installer chooses components from the configuration file alone

`zeabur-installer plan` no longer reads the install record to choose components: a default the record lacks is installed. The record still shows what `apply` adds and removes.
