# zeabur-test-support

Helpers shared by Zeabur's tests. Only ever a dev-dependency: nothing here reaches a binary.

- `sandbox` (feature `sandbox`): a namespace on a disposable cluster with a
  person in it, deleted when the test ends, for the cluster tests (`crates/*/tests/kubernetes.rs`).

Its own test checks every crate's `fixtures/` against `fixtures/README.md`; the cases are loaded
with `zeabur-fixtures`.
