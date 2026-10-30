---
name: zeabur-oss-dependencies
description: The supply-chain policy every zeabur-oss repository shares for Rust crates and Bun packages — OSI licences, aws-lc-rs as the only crypto backend, a seven-day publish age, unsafe and native-code review, no install scripts, committed lock files, Git-pinned design system, fallow through bunx. Use whenever Cargo.toml, Cargo.lock, deny.toml, a package.json, a bun.lock, bunfig.toml or flake.nix changes in any zeabur-oss repository.
---

# Zeabur OSS dependencies

The repository's dependencies skill names its shipped binaries, its frontend directory and its
exceptions. Run every command inside `nix develop`.

## Rust

- **Prefer what is already in the graph.** `cargo tree -i <crate> -e normal` shows whether a
  crate is already compiled in (hyper, rustls, url …). Ask first whether a hundred lines of our
  own would do; a small, reviewed in-tree implementation can beat a new dependency with
  `unsafe` code. Weigh it explicitly.
- **Add at the workspace level**: version and features in the root
  `[workspace.dependencies]`, `name.workspace = true` in the crate. Disable default features you
  do not need.
- **Policy** (`deny.toml`, checked by `cargo deny --locked check`): licences are OSI-approved
  ([SPDX list](https://spdx.org/licenses/), [OSI](https://opensource.org/licenses)); crates come
  from crates.io only; no `*` versions. TLS uses rustls with
  [aws-lc-rs](https://aws.github.io/aws-lc-rs/) as the only cryptographic backend: `ring`,
  OpenSSL, `native-tls`, the FIPS build of aws-lc and `webpki-roots` (its CDLA licence is not
  OSI-approved; images ship a CA bundle instead) are banned. Check that a new crate brings no
  second crypto backend.
- **Advisories.** [RustSec](https://rustsec.org/) advisories and yanked versions fail the check.
  An advisory that does not apply gets an `ignore` entry in `deny.toml` with the reason.
- **Publish age.** Cargo does not pick a release younger than seven days unless it is already in
  the lock file (`registry.global-min-publish-age` in `.cargo/config.toml`). For an urgent
  security fix, override it for one command:
  `CARGO_REGISTRY_GLOBAL_MIN_PUBLISH_AGE="0 days" cargo update -p <crate>`.
- **Unsafe and native code.** Run the unsafe report (`./scripts/unsafe-report.sh`, or
  `zeabur-oss-unsafe-report <package>…` from the `zeabur-oss/.github` flake). Every shipped crate
  that contains `unsafe` or builds native code needs an entry in
  `supply-chain/unsafe-allowlist.toml`: `kind`, a concrete `reason` (where the unsafe is,
  whether it runs only at build time), and `approved = "pending"` until a maintainer reviews it
  and sets the date. Only normal and build dependencies of the shipped binaries count;
  dev-dependencies and test crates do not. A feature enabled only in dev-dependencies does not
  ship; moving it to a normal dependency needs the review.

## Bun packages

- Add with `bun add <pkg>` (or `bun add -d`) from the package's directory. `bunfig.toml` pins
  exact versions and refuses releases younger than seven days (`minimumReleaseAge`).
- Licences are OSI-approved, as for crates. The repository's skill lists any exception.
- Never add `trustedDependencies`: no package may run install scripts. `bun pm untrusted` lists
  the blocked ones.
- Commit `package.json` and `bun.lock` together; CI installs with `bun ci`, which fails if they
  disagree.
- `@zeabur/design-system` is pinned to a commit of its repository: a Git dependency
  (`github:zeabur-oss/design-system#<commit>`, moved with
  `bun add 'github:zeabur-oss/design-system#<commit>'`) or a tarball packed from that commit and
  checked in under `vendor/`; the repository's skill says which. To try an unpublished
  design-system commit, point it at a local tarball (the design system's `design-system-change`
  skill) and `bun install`; never commit `package.json` or `bun.lock` with a path outside the
  repository.
- fallow is not in nixpkgs. It runs as `bunx fallow@<version>` from the `fallow` script in
  `package.json`. Its platform packages ship a prebuilt Rust binary (npm provenance; the wrapper
  checks its signature before it runs) and run no install scripts. Bump it to a release at least
  seven days old, in the script and in `.fallowrc.json`'s `$schema`, in every repository that
  runs it at once. Move it to the flake when nixpkgs has it.

## Nix and Renovate

- `flake.lock` pins nixpkgs and every tool of the dev shell. Renovate (`renovate.json`, extending
  `github>zeabur-oss/.github`) updates it with lock file maintenance every Monday, and opens
  pull requests for packages and action digests after the same seven days (`minimumReleaseAge`).
  Fix what a new tool version breaks on that pull request before merging.
