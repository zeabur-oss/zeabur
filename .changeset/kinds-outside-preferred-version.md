---
default: patch
---

# Kinds outside their group's preferred version count as served

A kind that its API group serves only at a version other than the preferred one, such as VictoriaMetrics' `VMRule` at `v1beta1` while the group prefers `v1`, no longer shows as not served. Templates and installs that need such a kind now go ahead. Zeabur OSS uses the version kubectl uses: the preferred version when it serves the kind, otherwise the highest-priority version that does.
