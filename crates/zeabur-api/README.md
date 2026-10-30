# zeabur-api

The HTTP API under `/api`, and the OpenAPI document generated from it.

## Hides

Routing, request and response encoding, and errors as RFC 9457 Problem Details. Every handler
calls `zeabur-kube` with the requesting person's credential, through
[`ClusterAccess`](crate::access::ClusterAccess): local mode has one person (the kubeconfig's),
the in-cluster server a client per request, tests an in-memory cluster.

## Design

- **Every item names its origin.** Each value in a response carries the Kubernetes object (and,
  where it is one field, the field path) it was read from, so a person can always go from
  Zeabur's view to the object behind it. One type, `Origin`, for all of them.
- **One shape for gets, one for lists.** A get returns the resource itself; a list returns
  `{items}` with every item. Event lists return the newest `limit` and count every match in
  `totalSize` (`views`, `query`).
- **The project document is the unit.** One read of a namespace gives the whole projection;
  the service and resource endpoints are cuts of it, and the web interface derives its views
  from the cached document (`projects`).
- **What cannot be read is said.** A project lists the kinds the person may not list
  (`unreadable`) instead of silently showing less.
- **Nothing is cached.** Every answer is `Cache-Control: no-store` (`cache`): answers are read as
  the person and carry values such as environment variables. Clients poll rather than watch.
- **Images are read from their registries, never on Zeabur's own rights.** `images/inspect`
  reads an image's declared ports and user anonymously, with credentials the person typed, or
  with a pull Secret read as the person; credentials go only to the registry they name. The
  in-cluster server does not connect to loopback, link-local, multicast or unspecified
  addresses, which its pod may reach and a person may not. Answers are kept in memory for five
  minutes, per credential; a registry that cannot be read is an answer (`unreadable`), so
  deploying never waits on one.
- **Secret values are an explicit action.** Listings never include them; `POST
  …/secrets/{name}/reveal` returns them with `Cache-Control: no-store`, and nothing logs or keeps
  them. Granting `list` on Secrets already lets a person read their values [k8s-secrets], so the
  reveal adds no right; it keeps values out of every page that does not ask for them.
- **Component management needs a view of the whole cluster.** `/cluster/capabilities` answers
  `403` when any detection probe is refused, rather than showing installed components as
  missing.
- **Logs follow over Server-Sent Events.** One `data` line per log line, an `error` event with a
  problem when the stream breaks; no WebSocket dependency for a one-way stream [whatwg-sse].
- **A terminal's container is a plan first.** A debug container and a tools pod are applied
  like any plan, alone in it; `tools` is the only place the person's token leaves Zeabur, and
  only through exec stdin into the person's own tools pod (`docs/reference/architecture.mdx`, Terminals).

## Place in the workspace

Level 4: may depend on every layer below it.

## OpenAPI from the code

The OpenAPI document is generated from the handlers and committed as `openapi.yaml` and
`openapi.json`; a test regenerates both and fails on any difference, so the document cannot drift
from the server (`ZEABUR_UPDATE_OPENAPI=1 cargo nextest run -p zeabur-api` rewrites them). The
server never builds the document: it serves the committed `openapi.json` at `/api/openapi.json`.

The generator is utoipa with utoipa-axum [utoipa]. Handlers stay plain axum handlers; a
`#[utoipa::path]` attribute and `#[derive(ToSchema)]` describe them, and one route table in
`lib.rs` states each route's method, path and handler once. The server registers the table in a
plain axum `Router`; the test builds it into utoipa-axum's `OpenApiRouter` for the document, and
fails when a path in the table differs from its `#[utoipa::path]`. utoipa 6 emits OpenAPI 3.1.0 by
default, and adds about fifteen crates over axum with no `unsafe` code in its own sources and no
schemars, chrono or TLS stack.

The utoipa crates are pinned to exact versions and were reviewed before they were added: one
owner and publisher for every release, crates.io archives identical to the tagged commits, no
build script, no `unsafe` and no file, network or process access in the macros (only the
`CARGO_PKG_*` variables for the default `info`). Two of their features stay off: `yaml`, which
pulls a C-to-Rust translation of libyaml with hundreds of `unsafe` functions (the committed YAML
is written with serde-saphyr in a test instead), and `config`, which makes the macro read a file
at compile time.

Alternatives considered:

- **Dropshot** [dropshot] is a complete server framework on hyper, not axum, so it would replace
  the router and middleware. Version 0.17.1 emits OpenAPI 3.0.3, generates schemas with a pinned
  schemars 0.8, logs through slog, and resolves to about 150 crates.
- **aide** [aide] wraps axum and emits OpenAPI 3.1, but its stable release (0.15.1) is from
  August 2025, the next line is still alpha, and it depends on schemars.

## References

- [utoipa] juhaku and contributors, utoipa 6.0.0 and utoipa-axum 0.3.0, crate sources.
  <https://github.com/juhaku/utoipa>
- [dropshot] Oxide Computer Company, dropshot 0.17.1, crate source.
  <https://github.com/oxidecomputer/dropshot>
- [aide] aide 0.15.1, crate source. <https://github.com/tamasfe/aide>
- [k8s-secrets] The Kubernetes Authors, "Good practices for Kubernetes Secrets," Kubernetes
  Documentation. <https://kubernetes.io/docs/concepts/security/secrets-good-practices/>
- [whatwg-sse] WHATWG, "HTML Living Standard, §9.2 Server-sent events."
  <https://html.spec.whatwg.org/multipage/server-sent-events.html>
- [rfc9457] M. Nottingham, E. Wilde and S. Dalal, "Problem Details for HTTP APIs," RFC 9457,
  IETF, July 2023. <https://www.rfc-editor.org/rfc/rfc9457>

[utoipa]: https://github.com/juhaku/utoipa
[dropshot]: https://github.com/oxidecomputer/dropshot
[aide]: https://github.com/tamasfe/aide
[rfc9457]: https://www.rfc-editor.org/rfc/rfc9457
[k8s-secrets]: https://kubernetes.io/docs/concepts/security/secrets-good-practices/
[whatwg-sse]: https://html.spec.whatwg.org/multipage/server-sent-events.html
