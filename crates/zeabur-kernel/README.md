# zeabur-kernel

The values every Zeabur crate shares, and the names Zeabur and Kubernetes must agree on. Nothing
here performs I/O or holds a key, so the pure crates (the model, the projection, gaps and plans)
build anywhere, WebAssembly included.

## Hides

The names under `zeabur.com/` and the field manager `zeabur` (and `Conventions`, which lets
another tool use its own), Kubernetes quantities, the subset of JSON Patch Zeabur sends, the
SHA-256 that derives short names, and the shared value types: `Message`, `Origin`, `Tool`,
`DataStore`, `Denied`. The `NetworkPolicy` of a new project namespace (`isolation`).

## Features

- `openapi`: derives `utoipa::ToSchema` for the shared types. `zeabur-api` enables it.

## Place in the workspace

Level 0: depends on no other workspace crate. Every other crate may depend on it.
