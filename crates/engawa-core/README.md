# engawa-core

Primitives every Engawa crate shares. Nothing here performs I/O.

## Hides

The names Engawa and Kubernetes must agree on: label and annotation keys under
`engawa.engawahq.dev/`, the field manager name `engawa`, and the error categories that the API
renders as RFC 9457 Problem Details; and which requests a server of Engawa answers
(`guard`: its `Host`, and changes only from its own pages), the same rules for the in-cluster
server and `engawactl ui`.

## Place in the workspace

Level 0: depends on no other workspace crate. Every other crate may depend on it.
