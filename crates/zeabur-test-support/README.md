# zeabur-test-support

Helpers shared by Zeabur's tests. Only ever a dev-dependency: nothing here reaches a binary.

- `fixtures`: loads Zeabur's language-neutral test cases and compares results with them. A case
  is a directory holding `case.yaml`, an optional `input.yaml` with Kubernetes objects and
  `expected.json`. Each crate keeps its cases in its own `fixtures/<area>/`; `load!("<area>")`
  reads them. Objects stay `serde_json::Value`s: the crate under test decides how to type them.
  The format is in the repository's `fixtures/README.md`.
- `sandbox` (feature `sandbox`): a namespace on a disposable cluster with a
  person in it, deleted when the test ends, for the cluster tests (`crates/*/tests/kubernetes.rs`).
- `tls` (feature `tls`): fixed certificates (two CAs, and a leaf for `localhost` and `127.0.0.1`
  that the first signed) and a loopback HTTPS server that presents the leaf, to test which
  certificates a client trusts. `testdata/tls/generate.sh` makes them.

Its own test checks every crate's `fixtures/` against `fixtures/README.md`; the cases are loaded
with `fixtures`.
