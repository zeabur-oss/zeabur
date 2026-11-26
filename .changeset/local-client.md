---
default: patch
---

# Zeabur Desktop and zeaburctl ui name themselves in the experience program

`cluster_connected` now carries `client`, `desktop` or `zeaburctl`, and a local run sends `feature_used` each time a feature is used, with `mode: local` and its `client`. This shows how many people use Zeabur Desktop. The experience program page lists the new property, and it is still off with `DO_NOT_TRACK=1`.
