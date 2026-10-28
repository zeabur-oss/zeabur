---
name: zeabur-dependencies
description: Add or upgrade a Rust or web dependency in Zeabur under its supply-chain policy (OSI licences, aws-lc-rs only, seven-day publish age, unsafe/native review, bun.lock). Use whenever Cargo.toml, Cargo.lock, web/package.json, web/bun.lock or flake.nix changes.
---

# Dependencies

## Rust
- **Prefer what is already in the graph.** `cargo tree -i <crate> -e normal` shows whether a
  crate is already compiled in. Ask first whether a hundred lines of our own would do; weigh a
  new dependency with `unsafe` code explicitly.
- **Add at the workspace level**: version and features in the root
  `[workspace.dependencies]`, `name.workspace = true` in the crate. Disable default features you
  do not need; check that no second crypto backend arrives (`ring`, OpenSSL and `webpki-roots`
  are banned in `deny.toml`).
- **Publish age.** Cargo refuses versions younger than seven days. For an urgent security fix:
  `CARGO_REGISTRY_GLOBAL_MIN_PUBLISH_AGE="0 days" cargo update -p <crate>`.
- **Run** `cargo deny --locked check` and `./scripts/ci.sh unsafe-report`. Every shipped crate that
  contains `unsafe` or builds native code needs an entry in `supply-chain/unsafe-allowlist.toml`:
  `kind`, a concrete `reason`, and `approved = "pending"` until a maintainer reviews it and sets
  the date. Only normal and build dependencies of `zeabur`, `zeaburctl`, `zeabur-proxy` and
  `zeabur-installer` count. A feature
  enabled only in dev-dependencies does not ship; moving it to a normal dependency needs the
  review (as kube's and axum's `ws`, for the terminal, brought in `tungstenite`).
- **Advisories** that do not apply need an `ignore` entry in `deny.toml` with the reason.
- `k8s-openapi` is built for Kubernetes 1.34 (`v1_34`), the oldest version Zeabur supports.

## Web (`web/`)
- Add with `bun add <pkg>` (or `bun add -d`) inside `nix develop`, from `web/`. `bunfig.toml`
  pins exact versions and refuses releases younger than seven days (`minimumReleaseAge`).
- Licences are OSI-approved, as for crates. The one exception is `caniuse-lite` (CC-BY-4.0
  data, pulled in by svgr's Babel, development only; see CONTRIBUTING.md).
- Never add `trustedDependencies`: no package may run install scripts.
- Commit `package.json` and `bun.lock` together; CI installs with `bun ci`, which fails if they
  disagree.
- Icons are Phosphor's, from `@iconify-json/ph` (built in by unplugin-icons with its JSX compiler,
  `@svgr/core` + `@svgr/plugin-jsx`; all dev
  dependencies). Components come from the shadcn registry
  (`bunx --bun shadcn@latest add <component>`), on Base UI.
- fallow is not in nixpkgs. It runs as `bunx fallow@<version>` from the `fallow` script in
  `package.json`. Its platform packages ship a prebuilt Rust binary (npm provenance; the wrapper
  checks its signature before it runs) and run no install scripts. Bump it to a release at least
  seven days old, in the script and in `.fallowrc.json`'s `$schema`. Move it to the flake when
  nixpkgs has it.
- Check the first-load size after adding a runtime package: `./scripts/build-ui.sh &&
  ./scripts/ui-budget.sh`.
