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

Run what CI runs (the jobs of `scripts/ci.sh` that need no cluster), inside `nix develop`:

```sh
cargo fmt --all --check && nixfmt --check flake.nix && shellcheck scripts/*.sh && actionlint
cargo lint                                   # clippy, all targets and features, -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items --locked
cargo nextest run --workspace --all-features --locked
cargo deny --locked check
./scripts/ci.sh unsafe-report
./scripts/check-levels.sh
./scripts/check-chart.sh                     # the Helm chart: lint, renderings, refused values
./scripts/build-without-ui.sh                # the API-only build, without bun
(cd web && bun ci && bun run fmt:check)
./scripts/check-ui-styles.sh && ./scripts/build-ui.sh   # build-ui.sh type-checks too (tsc -b)
(cd web && bun run lint && bun run fallow dead-code --baseline fallow-baseline.json && bun run fallow audit --base main && bun run test && bun run gen:api)
git diff --exit-code -- web/src/api/schema.d.ts     # the generated API types are up to date
./scripts/ui-budget.sh
ZEABUR_UI_DIST="$PWD/web/dist" cargo build --release --locked -p zeabur -p zeaburctl
```

`bun run fallow` runs [fallow](https://github.com/fallow-rs/fallow) at the version pinned in
`web/package.json`. `fallow dead-code` finds unused files, exports and dependencies in `web/` and
imports that cross the layers in `web/.fallowrc.json`; `fallow-baseline.json` lists the findings
that predate it. `fallow audit` fails on new findings in the files a change touches, complexity and
duplication included. `bun run test` runs the interface's unit tests with `bun test`.

The interface's zh-TW wording is checked with [zhtw-mcp](https://github.com/sysprog21/zhtw-mcp) in
a dev shell of its own (it builds from source; the pin and how to bump it are in `flake.nix`), as
CI's `wording` job does: `nix develop .#zh -c ./scripts/check-zh-tw.sh`. `web/scripts/zh-tw-strings.ts` prints every zh-TW
string after its key; `.zhtw-mcp.toml` holds the rules, the catalogue's chosen terms and the reason
for each rule or term it relaxes. Any warning fails the check. The term chosen for each concept
and how each interface element reads are in [docs/internal/wording.md](docs/internal/wording.md);
the general writing rules for all three locales are in the `zeabur-writing` skill
(`.claude/skills/zeabur-writing/`, installed from
[zeabur-oss/skills](https://github.com/zeabur-oss/skills) and pinned in `skills-lock.json`), and
Vale checks docs and catalogues against them: `nix develop -c vale docs README.md web/src/i18n`
(`.vale.ini`). A rule that is wrong for one passage is silenced there with a Vale comment that
says why.

Zeabur Catalog comes in as a flake input: `flake.lock` pins its commit, and the dev shell builds
the catalog's `release` output there, the same `bundle.tar.gz` and `index.json` that its release
signs. `zeabur-catalog` embeds that bundle. To move the pin, run `nix flake update catalog`; to try
an unpushed catalog, add `--override-input catalog git+file://$PWD/../catalog` to `nix develop`.
A release checks the bundle against the catalog's signed release at that commit
(`scripts/release-pins.sh`). Tests read the catalog at the pin too, so moving it goes in a pull
request of its own, with no change in behaviour: `flake.lock`, the `fixtures/charts` corpus
(`scripts/render-chart-corpus.sh`) and the files generated from the catalog
(`ZEABUR_UPDATE_GOLDEN=1` and `ZEABUR_UPDATE_OPENAPI=1` with `cargo nextest run`, then
`bun run gen:api` in `web/`). The files vendored for the interface's styles are checked in
[zeabur-oss/design-system](https://github.com/zeabur-oss/design-system). That package itself is
checked in here as `vendor/zeabur-design-system-<commit>.tgz`, packed from a commit of its
repository, so that `bun ci` downloads nothing from GitHub; `scripts/bump-design-system.sh
<commit>` moves it.

Every CI job is a call to `scripts/ci.sh <job>` (`scripts/ci.sh --list` names them), which enters
its dev shell itself; the workflow only checks out, installs Nix and restores caches. So a job runs
the same on any Linux machine with Nix, and Docker for the cluster jobs: `scripts/ci.sh clippy`.
`scripts/ci-local.sh [job…]` runs several (all by default), each in its own copy of the checkout,
and prints one result line per job; use a disposable machine or VM for the cluster jobs.

A comment says only what the code cannot: a constraint that is not visible, a quirk and its
cause, a workaround, or the specification section, RFC or issue behind behaviour that must not
change (rustdoc cites it as the crates do). Usually one line; no design rationale, history,
measurements or restatements of the code. Decisions and their reasons go in `docs/internal/`.
Cite only what you have read; a source you have not read is not cited at all, and if it matters
it goes in `docs/internal/` or an issue. Every claim that Zeabur does something better or safer
is backed by a test, not only by documentation. Comments and rustdoc are written in English. The docs and interface strings come in en, zh-TW and zh-CN: write
first in any of them, and change all three in the same pull request.

## Toolchain

- `flake.lock` is the only place the compiler version is set. `nix develop` takes the newest
  nightly in the locked rust-overlay that has every component and target listed in `flake.nix`.
  There is no `rust-toolchain.toml`; use `nix develop`, not rustup.
- The pinned nightly enables the parallel front end (`-Zthreads`), current clippy lints and
  rustfmt's import ordering. No `#![feature]` is used. Moving to stable is a configuration
  change: use `rust-bin.stable.latest` in `flake.nix`, drop `-Zthreads` from `.cargo/config.toml`
  and the nightly-only `rustfmt.toml` options.
- Renovate's lock file maintenance (`renovate.json`) updates `flake.lock` every Monday, and with it
  the nightly. A new nightly can bring new clippy lints or change a nightly-only setting; fix those
  on that pull request before merging.

Release binaries are built with `trim-paths = "all"`, so they carry no build-host paths.

## Compiler cache (optional)

The repository sets up no compiler cache. To share compiled crates between checkouts and
worktrees on your machine, install [kache](https://ninja.kunobi.com/docs/kache/getting-started/c-cpp)
and set `RUSTC_WRAPPER=kache` in your shell profile; `nix develop` keeps it. The C code that build
scripts compile (aws-lc-sys) goes through kache too, with no further setup.
`scripts/release-build.sh` unsets `RUSTC_WRAPPER`, so release binaries are always compiled.

## Lints and formatting

Lints are set once for the workspace in `Cargo.toml`. All are `warn`, so unfinished work still
compiles locally, and `cargo lint` turns every warning into an error in CI.

- `unsafe_code` and `non_ascii_idents` are forbidden.
- clippy `pedantic`, `cargo` and `nursery` are on.
- A selection of `restriction` lints targets what matters in a server that handles other
  people's credentials and in an installer that runs as root on someone's server: no panics on
  input (`unwrap_used`, `expect_used`, `indexing_slicing`, …; tests may
  unwrap), exhaustive matches in security logic (`wildcard_enum_match_arm`), no silently dropped
  errors (`map_err_ignore`, `let_underscore_must_use`, …), and no unexplained suppressions:
  use `#[expect(lint, reason = "…")]`, which also fails once it is no longer needed.
- Only the binaries' `output` modules (`out` in `zeabur-installer`) write to stdout or stderr;
  everything else logs through `tracing`.

`rustfmt.toml` groups and orders imports, wraps comments at 100 columns and fails on code it
cannot format.

## Dependencies

`cargo deny check` enforces the policy in `deny.toml`:

- Licences must be OSI-approved ([SPDX list](https://spdx.org/licenses/),
  [OSI](https://opensource.org/licenses)); crates come from crates.io only; no `*` versions.
- TLS uses rustls with [aws-lc-rs](https://aws.github.io/aws-lc-rs/) as the only cryptographic
  backend. `ring`, OpenSSL, `native-tls`, the FIPS build of aws-lc and `webpki-roots` (whose
  CDLA licence is not OSI-approved; the image ships a CA bundle instead, and the installer uses
  the system's CA store) are banned.
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
approved it. `zeabur-oss-unsafe-report` (from zeabur-oss/.github, run by `scripts/ci.sh
unsafe-report`) scans the shipped dependency graph (normal and build dependencies of `zeabur`,
`zeaburctl`, `zeabur-proxy` and `zeabur-installer` for the image target), lists every such crate and
fails on one that is not approved; an entry marked `pending` is a proposal awaiting review. The
count is a review signal, not an audit.

## Tests

What to test, at which level, and what not to test: [docs/internal/testing.md](docs/internal/testing.md).

- **Unit and integration tests** run anywhere: `cargo nextest run --workspace`. They include the
  language-neutral cases in [`fixtures/`](fixtures/README.md).
- **Cluster tests** and **end-to-end tests** are `#[ignore]`d and require a disposable cluster
  where you are cluster admin: the development cluster below, which the cluster jobs of
  `scripts/ci.sh` use too.

### Development cluster

Three commands give a k3d cluster with Zeabur Identity and Zeabur, installed with the installer's
own objects and values, at <https://zeabur.localhost>. Run them from the Zeabur checkout on a Linux
machine or VM with Docker running, Nix with flakes, and permission to use Docker; the cluster
uses port 443. Do not point anything at a production cluster: the tests create and delete
namespaces, service accounts and RBAC.

```sh
git clone https://github.com/zeabur-oss/zeabur && cd zeabur
nix run .#dev-cluster -- up       # prints the URL, a one-time setup link and the CA
nix run .#dev-cluster -- down     # deletes the cluster, its volume and its images
```

`up` builds Zeabur and Zeabur Proxy from the checkout and Zeabur Identity from the commit or tag in
`ZEABUR_IDENTITY_PINNED` (no image of it is published yet), then prints the setup link that enrols
the first admin's passkey. Your browser accepts the site once it trusts the printed CA
(`target/dev-cluster/tls/ca.crt`). On a VM, forward port 443 from your machine
(`sudo ssh -L 443:127.0.0.1:443 <vm>`); `*.localhost` resolves to your own machine. Run `up` again
after a change: it rebuilds and redeploys what changed. Options:

- `--profile full`: also CloudNativePG with the Barman Cloud plugin, the Percona Operator for
  MongoDB, Keel, cert-manager, zot, a test S3 and Zeabur Builder (from `ZEABUR_BUILDER_PINNED`).
- `--identity-src ../identity`: Zeabur Identity from that working tree, as it is. To move to a newer
  Zeabur Identity, write its release tag to `ZEABUR_IDENTITY_PINNED`.
- `ZEABUR_REGISTRY_MIRRORS=docker.io=http://mirror:5001,ghcr.io=http://mirror:5002`: the node pulls
  through these registry mirrors (each `registry=endpoint`), for a machine behind a slow or
  rate-limited link. `K3S_IMAGE` picks the K3s release.

- `--profile bare`: only the cluster. The cluster tests and local mode run there: their people
  are no admins, and Zeabur's admission policy would stop them changing a namespace's labels.

The tests (`nix develop` first):

```sh
export KUBECONFIG=$PWD/target/dev-cluster/kubeconfig
# on `up --profile bare`
cargo nextest run --locked --run-ignored only -E 'binary(kubernetes)'
./scripts/build-ui.sh && ./scripts/e2e-ui.sh     # zeaburctl ui in local mode (Playwright)
# on `up` or `up --profile full`
./scripts/e2e-l2-browser.sh                      # Zeabur in the cluster (Playwright)
```

Each cluster test works in a namespace of its own with a person in it, from
`zeabur-test-support`'s `sandbox`, and deletes it when it ends. A release `zeaburctl` embeds the
interface: `./scripts/build-ui.sh && ZEABUR_UI_DIST=$PWD/web/dist cargo build --release -p zeaburctl`.

`scripts/e2e-l2.sh` (the `kubernetes-l2` job) is `up`, the browser tests, then checks from outside
(Zeabur's service accounts have no extra rights beyond pods in `zeabur-terminals`, the idle pod's memory, removing Zeabur leaves the
cluster as it was) and `down`. It takes `up`'s options; `ZEABUR_L2_KEEP=1` keeps the cluster and
skips the removal check. On a cluster of `--profile full`, a browser test that would skip for want
of a component fails instead.

## Web interface

Sources are in `web/`. For development, run the demo server (the API over an in-memory cluster
with a few example projects, on `127.0.0.1:8080`) and Vite's dev server, which forwards `/api`
to it:

```sh
cargo run -p zeabur-api --example demo
(cd web && bun ci && bun run dev)
```

The example's documentation (`crates/zeabur-api/examples/demo.rs`) lists the variables that
refuse reads or offer admin rights, to see those states.

Components, tokens and fonts come from
[zeabur-oss/design-system](https://github.com/zeabur-oss/design-system), a Git dependency pinned to a
commit in `web/package.json`. It ships its sources, which Vite compiles like the interface's own
(`web/vite.config.ts`). To change a component, change it there, then move the pin:

```sh
(cd web && bun add 'github:zeabur-oss/design-system#<commit>')
```

To try a design-system change here before it is pushed, link a checkout next to this one with
`bun link` (see its README), and run `bun install` afterwards to return to the pin.

A release binary embeds the built interface; see the `zeabur-ui` crate documentation.

Motion work follows the `zeabur-ui` skill and the transitions.dev skills that `skills-lock.json`
pins; install those with `npx skills experimental_install` (they are not kept in this repository).

## Installer

`zeabur-installer` installs K3s or RKE2, Zeabur Catalog components, Zeabur Identity and Zeabur on a
Linux server ([guide](https://oss.zeabur.com/docs/zeabur/get-started/quickstart/)). It is three crates, each depending only on the
ones above it:

- `zeabur-installer-plan`: configuration, catalog, selection, values and the plan. No I/O.
- `zeabur-installer-host`: facts, preflight, downloads, files, systemd and `kubectl`.
- `zeabur-installer`: the command line, the steps of `apply` and the web interface.

Build on Linux for the server's architecture:

```sh
nix develop
./scripts/build-installer-ui.sh
ZEABUR_INSTALLER_UI_DIST="$PWD/crates/zeabur-installer/web/dist" cargo build --release --locked -p zeabur-installer
```

A release build embeds the web interface and stops without `ZEABUR_INSTALLER_UI_DIST`; a debug
build embeds a placeholder page instead. The interface is in `crates/zeabur-installer/web`, with
its own `package.json`, `bun.lock` and the same lint and format rules as `web/`. CI installs
exactly the lock file (`bun ci`), checks the format in the `format` job, and builds and lints it in
the `ui` job. To check it by hand:

```sh
(cd crates/zeabur-installer/web && bun ci && bun run fmt:check && bun run lint && bun run typecheck)
```

`crates/zeabur-installer-plan/src/pins.rs` names the RKE2 release and its SHA-256 digests.
`scripts/bump-rke2.sh <version>` rewrites both from the release's checksum lists. Review the
diff, and run the catalog's tests on that Kubernetes minor before you move to it.

`cargo nextest run --workspace` runs the planning code against the pinned catalog's `index.json` and
the host helpers against temporary directories. To run `apply`, use a disposable Linux VM with
systemd, 4 GB of memory and nothing else installed: RKE2 takes over the machine. Never run it on
a machine you need.
