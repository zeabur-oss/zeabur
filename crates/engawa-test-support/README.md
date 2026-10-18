# engawa-test-support

Helpers shared by engawa's tests. Only ever a dev-dependency: nothing here reaches a binary.

- [`fixtures`](crate::fixtures): loads the language-neutral cases in `fixtures/` (see
  `fixtures/README.md` for the format).
- `sandbox` (feature `sandbox`): a namespace on a disposable cluster with a
  person in it, deleted when the test ends, for the cluster tests (`crates/*/tests/kubernetes.rs`).
