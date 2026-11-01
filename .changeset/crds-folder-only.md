---
default: patch
---

# Upgrading an installation that has the VictoriaMetrics operator no longer stops on a field conflict over its CRDs

`zeabur-installer apply` now applies only the `crds/` folders Helm would install, and leaves CRDs a chart makes from its templates to Helm.
