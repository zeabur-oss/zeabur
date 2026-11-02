---
default: patch
---

# Zeabur asks for 24Mi of memory instead of 32Mi

Zeabur used about 6Mi in a cluster with 35 pods, so the chart's request drops to 24Mi. The limit stays at 128Mi.
