---
default: patch
---

# The experience program sends which project features are used

`feature_used` is now also sent after an apply opens a public port, turns on an access limit (with its kind), turns on automatic image updates (with the policy), makes a scheduled task, rolls back, or restores a volume, after the fix of a suggestion is applied, and after a project is restored from a cluster backup. The web interface tells the server when a plan comes from a suggestion's fix button; which suggestion it was is not sent.

Each one is sent at most once a UTC day for each set of values, and names nobody.
