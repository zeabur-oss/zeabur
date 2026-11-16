---
name: zeabur-oss-rust
description: The Rust conventions every zeabur-oss Rust workspace shares (zeabur, identity, builder) — the Nix-pinned nightly, a flat crates/ workspace with downward levels, workspace dependencies, the lint set (no unsafe, no panics on input, #[expect] with a reason, exhaustive matches in security logic, output modules), rustdoc and the checks CI runs. Use before changing any Rust, Cargo or rustfmt/clippy configuration in a zeabur-oss repository.
---

<!-- Edit this in zeabur-oss/.github (skills/zeabur-oss-rust/SKILL.md); the weekly skills sync overwrites the copy in other repositories. -->

# Zeabur OSS Rust

The repository's dev skill has its crate table, binaries, extra checks and generated files.

## Toolchain

- Everything runs inside the Nix dev shell: prefix commands with `nix develop -c …` (or enter
  `nix develop`). Do not use system-installed toolchains.
- `rust-toolchain.toml` is the one place the compiler is set: the nightly by date, components and
  targets. `flake.nix` builds it with `rust-bin.fromRustupToolchainFile`, and rustup reads the same
  file. The nightly enables the parallel front end (`-Zthreads`), current clippy lints and rustfmt's
  import ordering. No `#![feature]` is used.
- Renovate proposes the new nightly date in `rust-toolchain.toml` (its `rust-toolchain` manager, after the
  preset's seven-day wait). On that pull request, check that `nix flake update rust-overlay` resolves the
  date and fix new clippy lints. The date is the same in every zeabur-oss Rust repository, so the four
  bump pull requests merge together.
- Release binaries are built with `trim-paths = "all"`, so they carry no build-host paths.
- Optional compiler cache: install [kache](https://ninja.kunobi.com/docs/kache/getting-started/c-cpp)
  and set `RUSTC_WRAPPER=kache` in your shell profile; `nix develop` keeps it. Release and image
  builds unset `RUSTC_WRAPPER`.

## Workspace

- A flat `crates/` directory; dependencies only point downwards. Where the workspace has levels
  (`[workspace.metadata.levels]` in `Cargo.toml`, checked by [`scripts/check-levels.sh`](scripts/check-levels.sh), run from the workspace root), a crate
  depends only on crates of a lower level. Put new code in the lowest crate that can hold it;
  never add an upward dependency.
- Dependency versions live in the root `Cargo.toml` `[workspace.dependencies]`; crates use
  `name.workspace = true`. Adding one: `zeabur-oss-dependencies`.
- Each crate's README is its crate-level doc (`#![doc = include_str!("../README.md")]`).

## Lints

Lints are set once for the workspace in `Cargo.toml`. All are `warn`, so unfinished work still
compiles locally, and `cargo lint` (clippy, all targets and features, `--locked`,
`-D warnings`) turns every warning into an error in CI.

- `unsafe_code` and `non_ascii_idents` are forbidden. clippy `pedantic`, `cargo` and `nursery`
  are on.
- No panics on input: no `unwrap`/`expect`/indexing outside tests (`unwrap_used`, `expect_used`,
  `indexing_slicing` …).
- Matches in security logic are exhaustive: no wildcard arms on enums
  (`wildcard_enum_match_arm`). No silently dropped errors (`map_err_ignore`,
  `let_underscore_must_use` …).
- Suppress a lint only with `#[expect(lint, reason = "…")]`, which also fails once it is no
  longer needed; never `#[allow]`.
- Only a binary's `output` module writes to stdout or stderr; everything else uses `tracing`.
- Docs: identifiers in backticks (clippy `doc_markdown`); add proper nouns to `clippy.toml`
  `doc-valid-idents` instead of backticking them.
- `rustfmt.toml` groups and orders imports, wraps comments at 100 columns and fails on code it
  cannot format. When editing with scripts, match on short, stable anchors.

## Checks every workspace runs

```sh
nix develop -c bash -c 'cargo fmt --all --check && nixfmt --check flake.nix && shellcheck scripts/*.sh && actionlint'
nix develop -c cargo lint
RUSTDOCFLAGS="-D warnings" nix develop -c cargo doc --workspace --no-deps --document-private-items --locked
nix develop -c cargo nextest run --workspace --all-features --locked --no-tests=warn
nix develop -c cargo deny --locked check
```

Iterate per crate with `-p <crate>` to keep builds short. Drop `--check` from `cargo fmt` to
apply the formatting.

## Comments

Comments and rustdoc follow `zeabur-oss-standards`, section 1: only what the code cannot say.
Behaviour that rests on a specification carries the constraint and the citation in rustdoc next
to the code (`zeabur-oss-spec-work`); the reasons for the design go in the docs. Cite only what
you have read. A source you have not read is not cited at all; if it matters, note it in the
internal docs or an issue.
