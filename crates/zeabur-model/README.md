# zeabur-model

The data Zeabur shows: projects, services, the relations between them, and for every value the
Kubernetes object it came from (its *origin*). Each top-level node has an `id` in the shape of
its API path (`projects/shop/services/deployment/api`).

## Hides

The shape of the projection as seen by clients. The HTTP API and the generated
TypeScript client all serialise these types; nothing here knows how they are computed.

## Place in the workspace

Level 0: depends on no other workspace crate.

## Modules

- `kernel`: the names Zeabur and Kubernetes must agree on (label keys under `zeabur.com/`, the
  field manager `zeabur`, and `Conventions`, which lets another tool use its own), Kubernetes
  quantities, the subset of JSON Patch Zeabur sends, the SHA-256 that derives short names, and
  the shared value types: `Message`, `Origin`, `Tool`, `DataStore`, `Denied`. The
  `NetworkPolicy` of a new project namespace (`isolation`). Nothing here performs I/O or holds a
  key.
- `adapters`: the labels, annotations and owner conventions of Helm, Argo CD, Flux, and database
  or messaging operators, and one label-selector matcher. `zeabur-graph` asks "which tool
  manages this object, and which objects belong together?" and never matches a third-party
  label itself. Every function takes labels, annotations or owner references and returns an
  answer; an empty label is no label; only a controller reference hands a workload to an
  operator. It answers in its own terms (`adapters::provenance::Tool`, `Engine`,
  `DatabaseCluster`); `zeabur-graph` maps them to the types of this crate.

## Features

- `openapi`: derives `utoipa::ToSchema` for the projected view and the shared types.
  `zeabur-api` enables it.

## Design

- **Plain data.** Serde types in camelCase, no logic beyond constructors: the rules live in
  `zeabur-graph`, so the shape can be read without them.
- **Every value says where it comes from.** An environment variable's `source` is where its
  *value* is read from (`Literal`, `Secret`, `ConfigMap`, `PodField`, `ResourceField`, `File`).
  Every projected item also carries an `origin`: the Kubernetes object (kind and name) and the
  field path inside it where it is declared, such as
  `spec.template.spec.containers[name=app].env[name=NODE_ENV]`. Lists Kubernetes merges by a
  key are addressed by that key, atomic lists by index. A client can always open the object,
  and a change plan knows which field it writes.
- **No Secret values.** A Secret-backed variable is `masked` and has no `value` field at all;
  a Secret resource carries at most its key count.
- **Absent, not empty.** Optional fields are left out rather than sent as `null` or `""`, and
  `false` flags are left out, so a client never has to tell "unset" from "empty".
