---
name: zeabur-dev
description: What is special about the Zeabur repository — crates and binaries, the checks CI runs, API, tests, fixtures and scale rules. Use before changing any Rust, TypeScript, Nix or CI file in this repo.
---

# Zeabur development loop

What is Zeabur's own. Commits and pull requests: `zeabur-workflow`.

## Crates and binaries
- Each crate has a level in `Cargo.toml` `[workspace.metadata.levels]` (table in
  `docs/internal/crates.md`). Binaries: `zeabur` (in-cluster server) and `zeaburctl` (CLI: `ui`,
  `doctor`). Servers get their cluster from Zeatend (zeabur-oss/zeatend), not from this repo.
- `zeabur-kube` is the only crate that talks to kube-apiserver, and only through clients bound to
  a person's credential. Never build a client from Zeabur's service account token (the one
  exception is tools pods in the terminal namespace, `zeabur_api::tools`), and never
  impersonate.

## Checks (what CI runs)
Each CI job is `nix develop -c scripts/ci/<job>.sh`, the same on GitHub and on any Linux machine
with Nix (the jobs: `zeabur-ci`). `scripts/ci-local.sh [job…]` runs several, each in its own copy
of the checkout, and prints a result line per job. Cluster tests: `zeabur-cluster-tests`. Web
interface: `zeabur-ui`.

The screenshots are kept in git and not taken in CI: after changing a page that
`scripts/shots/docs.ts` covers, run `nix develop -c scripts/docs-shots.sh` on aarch64 Linux and
commit `docs/images/shots` (`docs/internal/screenshots.md`). The README and cover images take the
same script with their set, `readme` or `cover`.

## Lint conventions
- Integration tests wrap their tests in `#[cfg(test)] mod tests` (clippy
  `tests_outside_test_module`).

## Decisions
- Design decisions go in `docs/internal/`, not in comments.

## API
- Handlers live in `zeabur-api`, described with `#[utoipa::path]`, registered with `routes!`.
  After changing one, regenerate the committed description:
  `nix develop -c cargo xtask openapi` (also regenerates `web/src/api/zeabur.ts`).
- Every response item carries its `origin` (`zeabur_model::Origin`); errors are
  `zeabur_core::problem::Problem`. Gets return the resource itself, lists
  `{items}` with every item; event lists take a `limit` and add `totalSize`. Every answer is
  `Cache-Control: no-store`. A change that breaks a client gets a `BREAKING CHANGE:` footer;
  the lint job's oasdiff step (`scripts/api-break.ts`) fails without one.

## Tests
- Read `docs/internal/testing.md` before adding, changing or removing a test: the level each behaviour
  belongs at, what not to test, the promises that must keep a test, polling and isolation.
  Published contracts include the OpenAPI description and the rendered chart.

## Fixtures
- `crates/<crate>/fixtures/<area>/<case>/` holds language-neutral cases (format in
  `fixtures/README.md`), loaded with `zeabur_test_support::load!`. A new
  operation is documented in the README's table first; `zeabur-test-support`'s test fails on an
  undocumented one. Expected errors are snake_case codes, never message text.

## Scale
- Watch with kube-rs `watcher`, `reflector` or `Controller`. One LIST of the namespace, selected by
  label, replaces a GET per object.
- What Zeabur writes into one object (a Secret, a ConfigMap) stays under the object limits. Required state lives in the cluster or on disk, never only in memory.
