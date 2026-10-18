# Contributing

## Developer Certificate of Origin

This project uses the [Developer Certificate of Origin 1.1](DCO), not a CLA. Sign off every
commit to certify that you wrote it or otherwise have the right to submit it under the project
licence:

```sh
git commit -s
```

CI rejects pull requests whose commits lack a `Signed-off-by` line matching the author. Commit
messages follow [Conventional Commits](https://www.conventionalcommits.org/).

## Before you open a pull request

Run what CI runs (the `check` jobs of `.github/workflows/ci.yml`), inside `nix develop`:

```sh
cargo fmt --all --check && nixfmt --check flake.nix && shellcheck scripts/*.sh && actionlint
cargo lint                                   # clippy, all targets and features, -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items --locked
cargo nextest run --workspace --all-features --locked
cargo deny --locked check
./scripts/unsafe-report.sh
./scripts/check-levels.sh
./scripts/check-chart.sh                     # the Helm chart: lint, renderings, refused values
./scripts/build-without-ui.sh                # the API-only build, without bun
(cd web && bun ci && bun run fmt:check)
./scripts/check-ui-styles.sh && ./scripts/build-ui.sh   # build-ui.sh type-checks too (tsc -b)
(cd web && bun run lint && bun run knip && bun run test && bun run gen:api)
git diff --exit-code -- web/src/api/schema.d.ts     # the generated API types are up to date
./scripts/ui-budget.sh
ENGAWA_UI_DIST="$PWD/web/dist" cargo build --release --locked -p engawa -p engawactl
```

`bun run knip` finds unused files, exports and dependencies in `web/`; `bun run test` runs the
interface's unit tests with `bun test`.

The interface's zh-TW wording is checked with [zhtw-mcp](https://github.com/sysprog21/zhtw-mcp) in
a dev shell of its own (it builds from source; the pin and how to bump it are in `flake.nix`), as
CI's `zh-tw` job does: `nix develop .#zh -c ./scripts/check-zh-tw.sh`. `web/scripts/zh-tw-strings.ts` prints every zh-TW
string after its key; `.zhtw-mcp.toml` holds the rules, the catalogue's chosen terms and the reason
for each rule or term it relaxes. Any warning fails the check.

When you change a vendored file or its pin, also run the checks that compare them with their
upstream (they download it; `.github/workflows/vendored.yml` runs them only for such changes):

```sh
./scripts/check-recommendations.sh && ./scripts/check-genkan-files.sh && ./scripts/check-geist-mono.sh
```

Every CI job is a call to `scripts/ci.sh <job>` (`scripts/ci.sh --list` names them), which enters
its dev shell itself; the workflow only checks out, installs Nix and restores caches. So a job runs
the same on any Linux machine with Nix, and Docker for the cluster jobs: `scripts/ci.sh clippy`.
`scripts/ci-local.sh [job…]` runs several (all by default), each in its own copy of the checkout,
and prints one result line per job; use a disposable machine or VM for the cluster jobs.

Design rationale lives next to the code, in rustdoc: state the decision and its reason, and
cite the specification section, paper or prior art it rests on (the crates show the style).
Never cite something you have not read; write "not verified" instead. Every claim that engawa
does something better or safer is backed by a test, not only by documentation. Documentation is
written in English.

## Toolchain

- `rust-toolchain.toml` is the only place the compiler version is set. rustup reads it directly;
  the Nix flake reads it through fenix `fromToolchainFile`, so both use the same compiler.
- The pinned nightly enables the parallel front end (`-Zthreads`), current clippy lints and
  rustfmt's import ordering. No `#![feature]` is used. Moving to stable is a configuration
  change: drop `-Zthreads` from `.cargo/config.toml` and the nightly-only `rustfmt.toml` options.
- A toolchain bump also updates the `sha256` in `flake.nix`. `scripts/bump-toolchain.sh` does
  both, choosing the newest nightly of the last week with every listed component and target.
- `.github/workflows/update-toolchain.yml` runs that script with `nix flake update` on the 1st
  and 15th of each month and opens one pull request.

Release binaries are built with `trim-paths = "all"`, so they carry no build-host paths.

## Lints and formatting

Lints are set once for the workspace in `Cargo.toml`. All are `warn`, so unfinished work still
compiles locally, and `cargo lint` turns every warning into an error in CI.

- `unsafe_code` and `non_ascii_idents` are forbidden.
- clippy `pedantic`, `cargo` and `nursery` are on.
- A selection of `restriction` lints targets what matters in a server that handles other
  people's credentials: no panics on input (`unwrap_used`, `expect_used`, `indexing_slicing`, …; tests may
  unwrap), exhaustive matches in security logic (`wildcard_enum_match_arm`), no silently dropped
  errors (`map_err_ignore`, `let_underscore_must_use`, …), and no unexplained suppressions:
  use `#[expect(lint, reason = "…")]`, which also fails once it is no longer needed.
- Only the binaries' `output` modules write to stdout or stderr; everything else logs through
  `tracing`.

`rustfmt.toml` groups and orders imports, wraps comments at 100 columns and fails on code it
cannot format.

## Dependencies

`cargo deny check` enforces the policy in `deny.toml`:

- Licences must be OSI-approved ([SPDX list](https://spdx.org/licenses/),
  [OSI](https://opensource.org/licenses)); crates come from crates.io only; no `*` versions.
- TLS uses rustls with [aws-lc-rs](https://aws.github.io/aws-lc-rs/) as the only cryptographic
  backend. `ring`, OpenSSL, `native-tls`, the FIPS build of aws-lc and `webpki-roots` (whose
  CDLA licence is not OSI-approved; the image ships a CA bundle instead) are banned.
- [RustSec](https://rustsec.org/) advisories and yanked versions fail the check. Every ignored
  advisory carries its reason in `deny.toml`.

Web packages follow the same licence rule, with one exception: `caniuse-lite`, the browser
support data that Babel's `browserslist` reads while svgr builds the icons, is CC-BY-4.0 (a data
licence, not OSI-approved). It is a development dependency only and never ships in the interface.

Cargo does not pick a release younger than seven days unless it is already in the lock file
(`registry.global-min-publish-age` in `.cargo/config.toml`), which gives the community time to
catch a compromised release. An urgent security update overrides it for one command:
`CARGO_REGISTRY_GLOBAL_MIN_PUBLISH_AGE="0 days" cargo update -p <crate>`.

[Renovate](https://docs.renovatebot.com/) (`renovate.json`) opens the weekly crate, action digest
and k3s updates under the same seven-day delay; compatible crate updates share one pull request.
A crate update that brings in new `unsafe` or native code fails `unsafe-report` until the
allowlist entry below is added and reviewed, so those pull requests are never merged unread.

### Web interface

The web interface's packages are managed with Bun: `web/package.json` pins exact versions,
`web/bun.lock` is committed, and CI installs with `bun ci`, which fails if the two disagree.
`web/bunfig.toml` sets `minimumReleaseAge` to seven days, the same delay as Cargo's. No package
is listed in `trustedDependencies`, so no install script runs. Renovate updates `bun.lock` under
the same delay.

### Shipped `unsafe` and native code

A dependency that reaches the shipped binaries and contains `unsafe` code or builds native code
needs an entry in `supply-chain/unsafe-allowlist.toml` with a reason and the date a maintainer
approved it. `scripts/unsafe-report.sh` scans the shipped dependency graph (normal and build
dependencies of `engawa` and `engawactl` for the image target), lists every such crate and fails
on one that is not approved; an entry marked `pending` is a proposal awaiting review. The count
is a review signal, not an audit.

## Tests

What to test, at which level, and what not to test: [docs/testing.md](docs/testing.md).

- **Unit and integration tests** run anywhere: `cargo nextest run --workspace`. They include the
  language-neutral cases in [`fixtures/`](fixtures/README.md).
- **Cluster tests** and **end-to-end tests** are `#[ignore]`d and require a disposable cluster
  where you are cluster admin. The setup below is shared with the `kubernetes` job in
  `.github/workflows/ci.yml`.

### Disposable k3d cluster

Run from the engawa checkout on a Linux machine or VM with Docker running, Nix with flakes, and
permission to use Docker. Enter `nix develop` for the tools, including k3d and kubectl. Do not use
a production cluster: the tests create and delete namespaces, service accounts and RBAC.

```sh
nix develop
export KUBECONFIG=$(mktemp)
k3d cluster create engawa --image rancher/k3s:v1.36.5-k3s1 --no-lb --wait --timeout 300s
docker pull busybox:1.37 && k3d image import busybox:1.37 --cluster engawa
kubectl get nodes                         # Ready
cargo nextest run --locked --run-ignored only -E 'binary(kubernetes)'
./scripts/build-ui.sh && ./scripts/e2e-ui.sh   # browser tests of engawactl ui (Playwright)
```

Each cluster test works in a namespace of its own with a person in it, from
`engawa-test-support`'s `sandbox`, and deletes it when it ends. The separate kubeconfig prevents
k3d from changing your normal cluster context. When finished:

```sh
k3d cluster delete engawa
```

### engawa in a cluster with Genkan

`scripts/e2e-l2.sh` (the `kubernetes-l2` job) creates a cluster of its own, `engawa-l2`, installs
Genkan and engawa from their images (engawa with its chart) behind Traefik over HTTPS, and drives
a browser through passkey sign-in, a terminal confirmed with the passkey again, and reads and
changes with admin rights. It builds Genkan from a checkout next to this one (`GENKAN_SRC`,
default `../genkan`) at the commit in `GENKAN_PINNED`, refusing a checkout at another commit, and
uses port 443 on the machine. To move to a newer Genkan, check it out, write its commit to
`GENKAN_PINNED`, and run the script.

```sh
nix develop -c ./scripts/e2e-l2.sh
```

## Web interface

Sources are in `web/`. For development, run the demo server (the API over an in-memory cluster
with a few example projects, on `127.0.0.1:8080`) and Vite's dev server, which forwards `/api`
to it:

```sh
cargo run -p engawa-api --example demo
(cd web && bun ci && bun run dev)
```

The example's documentation (`crates/engawa-api/examples/demo.rs`) lists the variables that
refuse reads or offer admin rights, to see those states.

A release binary embeds the built interface; see the `engawa-ui` crate documentation.
