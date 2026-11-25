---
default: patch
---

# The Flux export pins the chart, orders releases and keeps what Zeabur keeps

`zeaburctl components export --format flux` now writes the catalog's chart digest on each `OCIRepository`, a `dependsOn` between a component's chart releases, and `kustomize.toolkit.fluxcd.io/prune: disabled` on its namespaces and input Secrets. Flux then installs releases in order, installs the chart the catalog pinned, and keeps the volumes and inputs when it prunes the export.
