# Testing

A test is worth keeping when it fails because a promise Zeabur makes broke, and only then. It
calls the code the way a caller does (the HTTP API, a crate's public functions, the rendered
page), so a refactor that keeps the behaviour keeps the test green [swe-unit].

## Levels

Each behaviour is tested at the lowest level that can see it, and repeated higher only for what
that level cannot see: wiring, a real kube-apiserver, a real browser [pyramid].

| Level | Where | For |
|---|---|---|
| Unit, property and fixture tests | the pure crates (`zeabur-graph`, `-plan`, `-gaps`, `-modules`, `-recommend`, …) and [`fixtures/`](../../fixtures/README.md) | the rules: projection, plans, diagnoses, gaps. Properties where an invariant exists (a planned change projects back to exactly what was asked) [quickcheck]; examples for known edges and past bugs, which random inputs rarely reach [proptest-limits] |
| API tests over the in-memory cluster | `crates/zeabur-api/tests/it`, one test binary (helpers shared with the cluster tests in `tests/common`) | the HTTP API end to end: routes, status codes, problems, what leaves the server |
| Cluster tests on k3s | `crates/*/tests/kubernetes.rs`, `#[ignore]`d ([CONTRIBUTING](../../CONTRIBUTING.md#tests)); each test's namespace and person come from `zeabur-test-support`'s `sandbox` | what only kube-apiserver decides: acting as the person, RBAC refusals, `test` preconditions, field managers, concurrent writes. The in-memory cluster is a fake; what it claims about kube-apiserver is checked here, by one suite of write behaviours run against both (`crates/zeabur-kube/tests/contract`) [swe-doubles] |
| Browser tests (Playwright) | `web/e2e` (local mode, `scripts/e2e-ui.sh`) and `web/e2e/l2` (in a cluster with Zeabur Identity, `scripts/e2e-l2.sh`) | user journeys and what only a browser shows: the CSP, masked values on screen, keyboard use, passkey sign-in. Logic in the interface is tested with `bun test` [e2e-good] |
| Zeabur Proxy's canary | `scripts/e2e-proxy.sh` (`cargo xtask e2e-proxy`), in a cluster with VictoriaLogs and `proxy.enabled` (and VictoriaMetrics for the metrics part) | that a person reads no other namespace's log lines or series, however the request is put; the proxy's attack table, property and fuzz target cover its parser in `cargo test` and the `fuzz` job |

## What we do not test

- **Trivial code**: getters, constants, `Display` strings, serde derives of plain structs [khorikov].
- **Restatements of the code**: a table or `match` copied into a test, the absence of a removed
  route, the size of a catalogue. They break on every change and catch nothing; such
  change-detector tests are rewritten or deleted [change-detector].
- **Third-party code**: kube-rs, aws-lc, React. We test our use of them at our seam.
- **Coverage for its own sake.** Coverage finds untested code; it is not a target, and it is no
  reason to keep a test [coverage].

## Promises that always have a test

Removing or weakening one of these tests needs another test of the same promise first.

- Secret values never leave the server: not in YAML, lists, `last-applied-configuration` or
  plans, and a masked value is never written back.
- Every read and write acts as the person, with their credential; nothing is impersonated.
- A write never takes over a field Helm, Argo CD or Flux owns.
- A plan's JSON Patch `test` operations refuse a stale write (`409`); of concurrent applies of one
  plan, exactly one wins.
- A plan token is refused when altered, expired, or used for another project.
- A refusal by RBAC is told apart from a component that is not installed.
- Data-loss bugs found before stay fixed (editing one key of a Secret the person cannot read
  keeps the other keys).
- The interface keeps its size budget and its Content Security Policy.

## Waiting and isolation

- **Poll with a deadline, never sleep.** Wait for a condition, give up at a deadline, and say what
  was last seen. Waiting on asynchronous work, often with a fixed sleep, is the most common cause
  of flaky tests [flaky] [nondeterminism]; Playwright's `waitForTimeout` is not used [playwright].
- **No shared state.** Each API test builds its own in-memory cluster. Each cluster test works in
  a namespace of its own with a random name and deletes it, and any cluster-scoped object, from a
  drop guard, so a failed run leaves nothing behind; tests assume they share the cluster [sig-e2e].
- **No real clock where the answer depends on the date**: tests fix it (`zeabur_api::router_at`).
- **A flaky test is broken.** It is fixed or removed, not retried until green [nondeterminism].

## Mutation testing

[cargo-mutants](https://mutants.rs) runs weekly (`.github/workflows/mutants.yml`, settings in
`.cargo/mutants.toml`). It makes small edits to the code, such as a negated condition or a
deleted `match` arm, and runs the tests; a *missed* mutant is code no test checks. Mutants
detected by a suite track the real faults it detects [mutants-real], and developers shown live
mutants write better tests [mutants-google].

The report gates nothing. For a missed mutant, add or sharpen a test of the behaviour that should
have broken, through the public interface, not of the mutation itself; when the mutant changes
nothing observable, leave it [mutants-results]. Run it on one file with
`cargo mutants -f <path>`.

[swe-unit]: https://abseil.io/resources/swe-book/html/ch12.html
[swe-doubles]: https://abseil.io/resources/swe-book/html/ch13.html
[pyramid]: https://martinfowler.com/articles/practical-test-pyramid.html
[quickcheck]: https://www.cs.tufts.edu/comp/150FP/archive/john-hughes/quick.pdf
[proptest-limits]: https://proptest-rs.github.io/proptest/proptest/limitations.html
[e2e-good]: https://testing.googleblog.com/2016/09/testing-on-toilet-what-makes-good-end.html
[khorikov]: https://www.manning.com/books/unit-testing
[change-detector]: https://testing.googleblog.com/2015/01/testing-on-toilet-change-detector-tests.html
[coverage]: https://cs.ubc.ca/~rtholmes/papers/icse_2014_inozemtseva.pdf
[flaky]: https://doi.org/10.1145/2635868.2635920
[nondeterminism]: https://martinfowler.com/articles/nonDeterminism.html
[playwright]: https://playwright.dev/docs/api/class-page#page-wait-for-timeout
[sig-e2e]: https://github.com/kubernetes/community/blob/master/contributors/devel/sig-testing/writing-good-e2e-tests.md
[mutants-real]: https://homes.cs.washington.edu/~rjust/publ/JustJIEHF2014b-abstract.html
[mutants-google]: https://arxiv.org/abs/2103.07189
[mutants-results]: https://mutants.rs/using-results.html
