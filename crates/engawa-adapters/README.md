# engawa-adapters

What other tools write on the objects they manage, and what it means.

## Hides

The labels, annotations and owner conventions of Helm, Argo CD, Flux, and database or messaging
operators. `engawa-graph` asks "which tool manages this object, and which objects belong
together?" and never matches a third-party label itself, so supporting a new tool changes only
this crate.

## Place in the workspace

Level 1: may depend on `engawa-core`. Pure functions; no I/O.

## Design

- **Pure functions over metadata.** Every function takes labels, annotations or owner
  references (k8s-openapi's `ObjectMeta`) and returns an answer; nothing reads a cluster.
- **Third-party keys live here only.** Helm, Argo CD and Flux marks (`provenance`), database
  operators and their controllers (`operators`), and the recommended `app.kubernetes.io/*`
  labels (`group`). engawa's own label, `engawa.engawahq.dev/group`, is here too; its field
  manager `engawa` is re-exported from `engawa-core`, which holds the names every crate shares.
- **One label-selector matcher** (`selector`). Kubernetes' set-based selector semantics, shared
  by the projection (a Deployment's pods) and the modules (NetworkPolicy, K8up) so the two cannot
  drift; each caller decides what an empty selector means.
- **An empty label is no label.** Every convention treats an empty value as absent.
- **Only a controller reference hands a workload to an operator.** Any object may list other
  owners for garbage collection; that does not make it part of their database.
- **This crate answers in its own terms** (`Tool`, `Engine`, `DatabaseCluster`); `engawa-graph`
  maps them to `engawa-model`'s types, so the two level-1 crates do not depend on each other.
