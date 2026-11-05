---
default: patch
---

# Plan checks the sources paths

`plan` and `apply` stop before changing the host when a `sources` path is missing or is a directory where a file belongs, naming the key, such as `sources.identityctl`.
