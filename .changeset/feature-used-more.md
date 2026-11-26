---
default: patch
---

# The experience program sends traces viewed and database exports

`feature_used` is now also sent after a service's trace or evidence is read and after a database dump finishes. Each one is sent at most once a UTC day and names nobody.
