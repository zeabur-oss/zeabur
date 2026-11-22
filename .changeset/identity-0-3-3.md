---
default: patch
---

# The chart installs Zeabur Identity v0.3.3

With Zeabur Identity v0.3.3, a later `helm upgrade` of the chart no longer fails with a conflict on `app.kubernetes.io/managed-by` of the Secret `zeabur-identity-signing-keys`. Zeabur Identity now changes only the keys of that Secret and keeps the labels and annotations Helm set.
