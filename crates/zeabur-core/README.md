# zeabur-core

What the in-cluster server and `zeaburctl ui` share. Nothing here performs I/O; only
`secret::forbid_core_dumps` changes two settings of the process when a binary starts.

## Hides

The error categories that the API renders as RFC 9457 Problem Details; which requests a server of
Zeabur answers (`guard`: its `Host`, and changes only from its own pages), the same rules for the
in-cluster server and `zeaburctl ui`; the cryptography (randomness, AEAD, constant-time
comparison) through aws-lc-rs; and how a secret is held in memory (`secret`: strings that never
grow, no core dumps).

It re-exports the modules of `zeabur-kernel`, so the server crates name them through one crate.

## Place in the workspace

Level 1: depends on `zeabur-kernel` only.
