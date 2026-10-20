# zeabur-graph

Turns a snapshot of Kubernetes objects into projects, services and the relations between them.

## Hides

The projection rules: how ownerReferences, selectors, env and volume references, backendRefs,
operator conventions and GitOps labels become "this Deployment, this Service and this Secret are
one service". Pure functions from a snapshot to a projection, tested entirely with the fixtures in
`fixtures/`.

## Place in the workspace

Level 2: may depend on `zeabur-core`, `zeabur-model` and `zeabur-adapters`. No I/O.

## Design

- **Pure functions of a [`Snapshot`].** Whoever reads the cluster builds the snapshot with the
  person's credential; everything here is computed from it and nothing else, so the same
  snapshot always gives the same projection and every rule is tested with fixtures.
- **Secret values are never read.** A snapshot's Secrets carry metadata and `type` only; a
  Secret's key count comes from `Snapshot::secret_key_counts`. The projection does not read
  `data` or `stringData` even when present (`tests/fixtures.rs` checks it).
- **Ownership comes from references, not ownerReferences.** ownerReferences say which
  controller deletes an object, not which workload uses it: a ConfigMap two Deployments import
  has none. A resource's owners are the workloads that reference it ([`relations`]): `env` and
  `envFrom` of every container, volumes and projected volume sources, `imagePullSecrets`, the
  PVCs a StatefulSet's claim templates created (named `<template>-<set>-<ordinal>`), the
  Services whose selector matches the pod labels, and the Ingresses routing to those Services.
  One owner: `owned`; several: `shared`; none: `orphan`. Owners are sorted by kind then name.
- **What is not a workload or a resource.** A Job a CronJob controls is one of its runs.
  Service account token and Helm release Secrets, and the `kube-root-ca.crt` ConfigMap, are
  machinery.
- **Operator conventions decide grouping only through `zeabur-adapters`.** A workload a
  database operator controls goes to `databases`, an operator's Deployment to `operators`, and
  the explicit group label wins over both.
- **Nothing is dropped.** A mount with no matching volume is shown as `Unknown`; a dangling
  reference still counts.
