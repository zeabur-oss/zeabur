---
name: zeabur-dev
description: Day-to-day development loop for the Zeabur repository — environment, the checks CI runs, lint conventions, commits. Use before changing any Rust, TypeScript, Nix or CI file in this repo.
---

# Zeabur development loop

## Environment
- Everything runs inside the Nix dev shell: prefix commands with `nix develop -c …` (or enter
  `nix develop`). Do not use system-installed toolchains; `rust-toolchain.toml` pins the
  compiler and the flake reads it.
- The workspace is a flat `crates/` directory; dependencies only point downwards. Each crate has
  a level in `Cargo.toml` `[workspace.metadata.levels]` (table in `docs/architecture.md`). Put new
  code in the lowest crate that can hold it; never add an upward dependency. Binaries: `zeabur`
  (in-cluster server) and `zeaburctl` (CLI: `ui`, `mcp`, `doctor`).
- `zeabur-kube` is the only crate that talks to kube-apiserver, and only through clients bound to
  a person's credential. Never build a client from Zeabur's service account token, and never
  impersonate.
- Workspace dependency versions live in the root `Cargo.toml` `[workspace.dependencies]`;
  crates use `name.workspace = true`.

## Checks (what CI runs)
```sh
nix develop -c cargo fmt --all --check   # drop --check to apply the formatting
nix develop -c cargo lint          # clippy --workspace --all-targets --all-features --locked -D warnings
RUSTDOCFLAGS="-D warnings" nix develop -c cargo doc --workspace --no-deps --document-private-items --locked
nix develop -c cargo nextest run --workspace --all-features --locked --no-tests=warn
nix develop -c cargo deny --locked check
nix develop -c ./scripts/unsafe-report.sh
nix develop -c ./scripts/check-levels.sh
nix develop -c bash -c 'shellcheck scripts/*.sh && actionlint && nixfmt --check flake.nix'
nix develop -c cargo build --release --locked --no-default-features -p zeabur -p zeaburctl
```
Iterate per crate with `-p <crate>` to keep builds short. Cluster tests: see
`zeabur-cluster-tests`. Web interface: see `zeabur-ui`. Each CI job is `scripts/ci.sh <job>`
(`--list` names them), the same on GitHub and on any Linux machine with Nix;
`scripts/ci-local.sh [job…]` runs several and prints a result line per job (in
`target/ci-local/*.result`), each in its own copy of the checkout but in the machine's own
environment, not a fresh one; on a VM, snapshot its clean state and restore it after every run
(see `zeabur-cluster-tests`). When a check fails on GitHub, see `zeabur-ci`.

## Lint conventions
- `unsafe` is forbidden. No panics on input: no `unwrap`/`expect`/indexing outside tests.
- Suppress a lint only with `#[expect(lint, reason = "…")]`; never `#[allow]`.
- Matches in security logic are exhaustive (no wildcard arms on enums).
- Output to stdout/stderr only through a binary's `output` module; everything else uses
  `tracing`.
- Integration tests wrap their tests in `#[cfg(test)] mod tests` (clippy
  `tests_outside_test_module`).
- Docs: identifiers in backticks (clippy `doc_markdown`); add proper nouns to `clippy.toml`
  `doc-valid-idents` instead of backticking them.

## Documentation
- Rationale goes in rustdoc next to the code: state the decision and why, cite the
  specification section, paper or prior art (see `zeabur-spec-work`). Each crate's README is its
  crate-level doc (`#![doc = include_str!("../README.md")]`).
- A claim that Zeabur does something better or safer needs a test that proves it.

## API
- Handlers live in `zeabur-api`, described with `#[utoipa::path]`, registered with `routes!`.
  After changing one, regenerate the committed description:
  `ZEABUR_UPDATE_OPENAPI=1 nix develop -c cargo nextest run -p zeabur-api`.
- Every response item carries its `origin` (`zeabur_model::Origin`); errors are
  `zeabur_core::problem::Problem`. Gets return the resource itself, lists
  `{items}` with every item; event lists take a `limit` and add `totalSize`. Every answer is
  `Cache-Control: no-store`. A change that breaks a client gets a `BREAKING CHANGE:` footer.

## Tests
- Read `docs/testing.md` before adding, changing or removing a test: the level each behaviour
  belongs at, what not to test, the promises that must keep a test, polling and isolation.

## Fixtures
- `fixtures/<area>/<case>/` holds language-neutral cases (format in `fixtures/README.md`). A new
  operation is documented in the README's table first; `zeabur-test-support`'s test fails on an
  undocumented one. Expected errors are snake_case codes, never message text.

## Commits
- Conventional Commits (`feat:`, `fix:`, `refactor:`, `docs:`, `chore(scope):`, `!` for breaking
  changes with a `BREAKING CHANGE:` footer).
- Sign off every commit (`git commit -s`, DCO). Do not add tool- or session-specific trailers.
- Branches, pull requests, merging and issues: see `zeabur-workflow`.
