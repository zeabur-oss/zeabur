# zeabur-mcp

A read-only Model Context Protocol server over stdio, so a local agent can ask "which service is
restarting, and why?". `zeaburctl mcp` runs it.

## Hides

The tool set and what each tool reads. Tools read the Zeabur API through a
[`Reader`](crate::Reader), which `zeaburctl` implements with the API in its own process and the
person's kubeconfig. The answers are the web interface's, under the same RBAC, and a Secret's
values are hidden in one place: the API.

## Tools

| Tool | Reads |
|---|---|
| `cluster_summary` | `/cluster`, `/cluster/upgrades`, `/cluster/gaps`, and `/cluster/capabilities` when the person may read it all |
| `list_projects` | `/projects` |
| `get_project` | `/projects/{p}`, cut to its services: health, replicas, restarts, container states with the last termination, diagnoses |
| `get_service` | `/projects/{p}/services/{kind}/{name}` |
| `project_events` | `/projects/{p}/events?limit=` (50 unless asked) |
| `pod_logs` | `/projects/{p}/pods/{pod}/logs` with `tail_lines` (100 unless asked, at most 1000), `container` and `previous` |
| `object_yaml` | `/projects/{p}/objects/{group}/{version}/{resource}/{name}`, Secret values hidden |
| `project_usage` | `/projects/{p}/usage`: each pod's CPU and memory now, from metrics-server |
| `service_history` | `/projects/{p}/services/{kind}/{name}/history?range=` (`1h` unless asked, `6h`, `24h`, `7d`), from the metrics store |

An answer is its JSON as compact text and as `structuredContent`. A refusal or failure is a
result with `isError: true` carrying the API's problem unchanged, with `fix` and the `denied`
right, so the model can tell the person what to ask for [mcp-tools].

## Read-only

Every tool reads paths from one allowlist ([`ALLOWLIST`](crate::ALLOWLIST)), and a `Reader`
can only `GET`. No tool reaches `POST`, revealing a Secret, plans and applies, the terminal or a
log stream; a test checks each allowlisted path against the API's OpenAPI document. Values a model
passes (a project, a pod, a kind) must each be one plain path segment, so they cannot reach
another path. Another test calls every tool against a cluster holding a Secret and finds its
value in no answer.

## Use it with Claude Code

```sh
claude mcp add zeabur -- zeaburctl mcp
claude mcp add zeabur-staging -- zeaburctl mcp --context staging
```

Standard output carries only protocol messages, messages for the person go to standard error,
and the server exits when standard input closes [mcp-stdio].

## Protocol

The protocol is rmcp's, the official Rust SDK [rmcp]: its server handler, tool macros, JSON
Schemas of the arguments (schemars) and stdio transport. It answers clients of the stateless
revision 2026-07-28 (`server/discover`, the version in each request's `_meta`) and clients still
on 2025-11-25 (`initialize`, `ping`) [mcp-versioning]. Zeabur writes no JSON-RPC or version
handling of its own.

## Place in the workspace

Level 4: may depend only on the crates below it; it uses `zeabur-core` for its own problems and
`zeabur-modules` for the history range it shares with the API (its `schemars` feature), and
does not depend on `zeabur-api`.

## References

- [mcp-stdio] Model Context Protocol, "Transports: stdio," revision 2026-07-28.
  <https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio>
- [mcp-versioning] Model Context Protocol, "Versioning and Compatibility," revision 2026-07-28.
  <https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning>
- [mcp-tools] Model Context Protocol, "Tools," revision 2026-07-28.
  <https://modelcontextprotocol.io/specification/2026-07-28/server/tools>
- [rmcp] Model Context Protocol, rmcp (Rust SDK). <https://github.com/modelcontextprotocol/rust-sdk>

[mcp-stdio]: https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio
[mcp-versioning]: https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning
[mcp-tools]: https://modelcontextprotocol.io/specification/2026-07-28/server/tools
[rmcp]: https://github.com/modelcontextprotocol/rust-sdk
