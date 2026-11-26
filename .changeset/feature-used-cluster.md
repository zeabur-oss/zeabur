---
default: patch
---

# The experience program sends which cluster features are set up

`feature_used` is now also sent after backups are scheduled, a notification channel is added or changed, a Zeabur Catalog component is installed (with its catalog ID), the GitHub App is connected, a node is taken out for maintenance, and the sign-in setting changes.

Like the other features, each one is sent at most once a UTC day for each set of values, and names nobody.
