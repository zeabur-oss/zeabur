# zeabur-plan

Turns a person's request into a plan: the writes, the fields they change, who owns those fields
now, which workloads it affects, and what to know before applying. The requests ([`Change`]):
set a variable, change a ConfigMap or Secret key, add, rename or remove a hostname, restart a
workload or roll it back to an earlier release, scale it, set a probe or its resources, add or
grow a volume and move a mount, replace an object with edited YAML, create a ready-made object
([`CREATABLE`]), delete one, adopt a namespace or let it go, and set a project's details.
A plan token binds the request, the resource versions it was computed against and the person,
so an apply carries out exactly what was confirmed, or nothing; this crate defines the claims,
`zeabur-api` seals them.

## Hides

How an edit maps to fields and to a write. Zeabur writes as the field manager `zeabur`, sends
only what it changes, tests the `resourceVersion` it read, and never takes over a field Helm,
Argo CD or Flux owns: an object one of them manages may be restarted but not edited or deleted.

## Place in the workspace

Level 3: may depend on `zeabur-kernel`, `zeabur-model`, `zeabur-adapters` and `zeabur-graph`. It
computes; `zeabur-api` reads the cluster, asks for the plan, signs it and sends the writes
through `zeabur-kube`.

## Tests

This crate's `fixtures/mutation` (`mutation.*` operations, including `mutation.plan`) runs in
`tests/fixtures.rs`; `tests/roundtrip.rs` re-projects planned objects.
