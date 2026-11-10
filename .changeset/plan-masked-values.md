---
default: patch
---

# A plan never writes a value shown masked

A Secret value appears as `(hidden)` or `••••••••`. A plan that would write such a mask in place of a value is refused with `masked_value`. This covers a variable, a ConfigMap or Secret key, a created object, and a mask base64-encoded under a Secret's `data` in a YAML edit. A Secret key left masked in a YAML edit keeps its value, as before.
