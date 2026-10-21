# zeabur-model

The data Zeabur shows: projects, services, the relations between them, and for every value the
Kubernetes object it came from (its *origin*). Each top-level node has an `id` in the shape of
its API path (`projects/shop/services/deployment/api`).

## Hides

The shape of the projection as seen by clients. The HTTP API, the MCP server and the generated
TypeScript client all serialise these types; nothing here knows how they are computed.

## Place in the workspace

Level 1: may depend on `zeabur-kernel`.

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
