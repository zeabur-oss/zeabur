---
name: zeabur-dev
description: Day-to-day development loop for the Zeabur repository — environment, the checks CI runs, lint conventions, commits. Use before changing any Rust, TypeScript, Nix or CI file in this repo.
---

# Zeabur development loop

Follow the shared skills (install: `npx skills add zeabur-oss/skills --skill <name>`;
`skills-lock.json` pins them): `zeabur-oss-rust` (Nix toolchain, workspace, lints, common
checks), `zeabur-oss-standards` (with [`STANDARDS.md`](STANDARDS.md) here),
`zeabur-oss-workflow` (commits and pull requests; Zeabur's part in `zeabur-workflow`). This skill
adds what is Zeabur's own.

## Crates and binaries
- Each crate has a level in `Cargo.toml` `[workspace.metadata.levels]` (table in
  `docs/internal/crates.md`). Binaries: `zeabur` (in-cluster server), `zeaburctl` (CLI: `ui`,
  `doctor`) and `zeabur-installer` (runs as root on a Linux server; crates `zeabur-installer-*`,
  guide in `docs/get-started/quickstart.md` and `docs/reference/installer*.md`).
- `zeabur-kube` is the only crate that talks to kube-apiserver, and only through clients bound to
  a person's credential. Never build a client from Zeabur's service account token (the one
  exception is tools pods in the terminal namespace, `zeabur_api::tools`), and never
  impersonate.

## Checks (what CI runs, beyond `zeabur-oss-rust`'s)
```sh
nix develop -c ./scripts/ci.sh unsafe-report
nix develop -c ./scripts/check-levels.sh
nix develop -c cargo build --release --locked --no-default-features -p zeabur -p zeaburctl
```
The installer's web interface (`crates/zeabur-installer/web`) is not in CI: run
`nix develop -c bash -c 'cd crates/zeabur-installer/web && bun install && bun run fmt:check && bun run lint && bun run typecheck'`
and `nix develop -c scripts/build-installer-ui.sh`.
The screenshots are kept in git and not taken in CI: after changing a page that `docs/shots.yaml`
covers, run `nix develop -c scripts/docs-shots.sh` on aarch64 Linux, as CI does, and commit
`docs/images/shots` (`docs/internal/screenshots.md`). The README and cover images take the same
script with their manifest, `docs/<set>-shots.yaml`.
Cluster tests: see `zeabur-cluster-tests`. Web interface: see `zeabur-ui`. Each CI job is
`scripts/ci.sh <job>` (`--list` names them), the same on GitHub and on any Linux machine with
Nix; `scripts/ci-local.sh [job…]` runs several and prints a result line per job (in
`target/ci-local/*.result`), each in its own copy of the checkout but in the machine's own
environment; on a VM, snapshot its clean state and restore it after every run (see
`zeabur-cluster-tests`). When a check fails on GitHub, see `zeabur-ci`.

## Lint conventions beyond `zeabur-oss-rust`
- The installer's output module is `out`.
- Integration tests wrap their tests in `#[cfg(test)] mod tests` (clippy
  `tests_outside_test_module`).

## Comments
- `zeabur-oss-standards`, section 1, in Rust, TypeScript, CSS and scripts alike: only what the
  code cannot say; decisions go in `docs/internal/`.

## API
- Handlers live in `zeabur-api`, described with `#[utoipa::path]`, registered with `routes!`.
  After changing one, regenerate the committed description:
  `ZEABUR_UPDATE_OPENAPI=1 nix develop -c cargo nextest run -p zeabur-api`.
- Every response item carries its `origin` (`zeabur_model::Origin`); errors are
  `zeabur_core::problem::Problem`. Gets return the resource itself, lists
  `{items}` with every item; event lists take a `limit` and add `totalSize`. Every answer is
  `Cache-Control: no-store`. A change that breaks a client gets a `BREAKING CHANGE:` footer.

## Tests
- Read `docs/internal/testing.md` before adding, changing or removing a test: the level each behaviour
  belongs at, what not to test, the promises that must keep a test, polling and isolation.

## Fixtures
- `crates/<crate>/fixtures/<area>/<case>/` holds language-neutral cases (format in
  `fixtures/README.md`), loaded with `zeabur_test_support::load!`. A new
  operation is documented in the README's table first; `zeabur-test-support`'s test fails on an
  undocumented one. Expected errors are snake_case codes, never message text.
