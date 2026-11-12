---
default: patch
---

# A secret key change checks who owns each key it writes

Setting one key of a secret no longer fails because Helm, Argo CD or Flux owns another key. Removing a key and setting it again is refused when one of those tools owns it. A key removed in a later change of the same plan is removed.
