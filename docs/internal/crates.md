# Crates

How the code is split, built and packaged. What Zeabur does for the people who use it is in
[Architecture](../reference/architecture.md).

## Crates

The code is split into small crates in one flat `crates/` directory:

- **Dependencies only point downwards**, with no cycles ([Kladov][matklad-workspaces]; Lakos and
  Martin, see [References](#references)). Each crate has a *level* in `Cargo.toml`
  (`[workspace.metadata.levels]`); `scripts/check-levels.sh` fails CI when a crate depends on one
  of the same or a higher level.
- **Each crate hides one design decision** behind a narrow interface ([Parnas][parnas]).
- **External systems are traits** with an in-memory and a Kubernetes implementation (ports and
  adapters, [Cockburn][cockburn]), so most tests need no cluster.
- **`zeabur-kube` is the only crate that talks to kube-apiserver**, and it only offers clients
  bound to a person's credential.
- **Integrations with installed components are modules** (`zeabur-modules`, one module per
  provider): they declare the resources they read and turn the objects into their own views.
  Only the composing crates (`zeabur-api`, the binaries) may depend on a module
  crate (`[workspace.metadata.modules]`, also checked by `scripts/check-levels.sh`), so the core
  never knows which integrations exist.
- **The rules are pure crates**: `zeabur-model`, `zeabur-graph`, `zeabur-gaps` and `zeabur-plan`
  perform no I/O and hold no key and depend only on each other (`[workspace.metadata.pure]`, also
  checked by `scripts/check-levels.sh`).

```mermaid
flowchart BT
  model[zeabur-model]
  core[zeabur-core] --> model
  ui[zeabur-ui]
  kube[zeabur-kube] --> core
  auth[zeabur-auth] --> core
  graph[zeabur-graph] --> model
  modules[zeabur-modules] --> kube & model
  gaps[zeabur-gaps] --> graph
  plan[zeabur-plan] --> graph
  recommend[zeabur-catalog] --> kube
  api[zeabur-api] --> kube & gaps & plan & recommend & modules
  zeabur([zeabur]) --> api & ui & auth
  local[zeabur-local] --> api & ui
  zeaburctl([zeaburctl]) --> local
```

Arrows point from a crate to what it may depend on (transitive edges omitted).

| Level | Crate | What it hides |
|---|---|---|
| 1 | [`zeabur-core`](../../crates/zeabur-core) | error categories, request guards, cryptography |
| 0 | [`zeabur-model`](../../crates/zeabur-model) | the projected view clients see (projects, services, endpoints, routes, sources), the names and quantities shared with Kubernetes (`kernel`), conventions of Helm, Argo CD, Flux, operators and Gateway API (`adapters`) |
| 0 | [`zeabur-embed`](../../crates/zeabur-embed) | **build-dependency**: reading a built interface's `manifest.tsv` into the table `zeabur-ui` and `zeabur-installer` embed |
| 0 | [`zeabur-telemetry`](../../crates/zeabur-telemetry) | the usage data Zeabur and the installer may send (types with no free text) and the one sender |
| 1 | [`zeabur-ui`](../../crates/zeabur-ui) | embedding and serving the web interface; with its `interface` feature off, only the serving (the installer's page) |
| 2 | [`zeabur-kube`](../../crates/zeabur-kube) | how a request reaches kube-apiserver, always as the person |
| 2 | [`zeabur-http`](../../crates/zeabur-http) | the outbound HTTP client: address guard, `HTTPS_PROXY` with `CONNECT`, trusted CAs |
| 2 | [`zeabur-auth`](../../crates/zeabur-auth) | signing in through Zeabur Identity (OAuth client), sessions, cross-site checks |
| 2 | [`zeabur-graph`](../../crates/zeabur-graph) | the projection rules (pure functions, tested with `fixtures/`) |
| 3 | [`zeabur-modules`](../../crates/zeabur-modules) | each installed component's resources and what Zeabur makes of them (one module per provider) |
| 3 | [`zeabur-gaps`](../../crates/zeabur-gaps) | gap rules and plain-language diagnostics |
| 3 | [`zeabur-plan`](../../crates/zeabur-plan) | change plans, minimal apply bodies, impact |
| 3 | [`zeabur-catalog`](../../crates/zeabur-catalog) | the recommendations bundle and install suggestions |
| 4 | [`zeabur-api`](../../crates/zeabur-api) | the HTTP API and its OpenAPI document |
| 5 | [`zeabur-local`](../../crates/zeabur-local) | the local service: the API with a person's kubeconfig and the web interface on a loopback port, as `zeaburctl ui` starts it |
| 5 | [`zeabur`](../../crates/zeabur) | **binary**: the in-cluster server |
| 6 | [`zeaburctl`](../../crates/zeaburctl) | **binary**: the command-line tool |
| 6 | [`zeabur-test-support`](../../crates/zeabur-test-support) | test helpers: loading the fixture cases, the cluster tests' sandbox |
| 0 | [`zeabur-installer-plan`](../../crates/zeabur-installer-plan) | the installer's configuration, component selection and install plan; no I/O |
| 1 | [`zeabur-installer-host`](../../crates/zeabur-installer-host) | what the installer does to a host: facts, preflight, downloads, files, systemd, `kubectl` |
| 5 | [`zeabur-installer`](../../crates/zeabur-installer) | **binary**: installs RKE2, Zeabur Identity and Zeabur on a server ([guide](../get-started/quickstart.md)) |
| 7 | [`xtask`](../../xtask) | **binary**, not shipped: development tasks run as `cargo xtask`: the development cluster (`scripts/dev-cluster.sh`) and Zeabur Proxy's canary (`scripts/e2e-proxy.sh`) |

Every crate has a README that is also its crate-level documentation (`cargo doc --open`).

## Apps outside the workspace

[`desktop/`](../../desktop) (the Tauri desktop app) is a Cargo workspace of its own, with its own
lock file, so the server's dependency graph never has Tauri, GTK or WebKit in it. It follows the
main workspace's rules:

- **Shared files.** Cargo, clippy and rustfmt read `.cargo/config.toml` (the `cargo lint` alias,
  rustflags, the publish age), `clippy.toml` and `rustfmt.toml` from the parent folders, and
  `cargo deny --config deny.toml` checks it with the same policy for the desktop platforms. The
  toolchain is the dev shell's (`nix develop .#desktop`, which adds GTK and WebKitGTK on Linux).
- **Copied tables.** Cargo reads `[workspace.lints]`, `[workspace.package]` and
  `[profile.release]` only from the root of a workspace, so `desktop/Cargo.toml` has a copy. `scripts/check-desktop.sh` fails CI when the copy
  differs. The one difference is `unsafe_code`: `deny`, not `forbid`, so that the two modules the
  script lists (`webkit.rs`, `win32.rs`) can wrap the system calls that have no safe binding,
  each with `#![expect(unsafe_code, reason = "…")]`. A new module that names `unsafe_code` fails
  the script.
- **One dependency edge.** Of the workspace crates, `desktop/` may depend only on `zeabur-local`
  (checked by the same script), which each cluster window starts; nothing in the workspace depends
  on `desktop/`.
- **Its own pages.** The chooser and the cluster window's start page are `desktop/web/`, a Vite
  project with its own `package.json` and `bun.lock` under the same package policy as `web/`. The
  app bundles their build; the cluster's web interface is `web/dist`, embedded in `zeabur-local`.

CI's `desktop` job (`scripts/ci/desktop.sh`) runs the script, the checks and build of
`desktop/web`, `cargo fmt`, `cargo lint`, the tests and `cargo deny` for `desktop/` on Linux.
`scripts/build-desktop.sh` bundles the app, and CI's `desktop-bundle` job builds the macOS app and checks it links
nothing from the Nix store (`scripts/check-portable.sh`). The macOS and Windows code is linted on a Mac
(`cargo lint`, and `cargo xwin clippy` for Windows, see `desktop/README.md`).

## Web interface

The interface's sources live in [`web/`](../../web) (React, TanStack Router and Query, shadcn/ui on
Base UI with its own theme, Phosphor icons). Its build output is not committed:
`scripts/build-ui.sh` builds it, and `zeabur-ui` (with its `interface` feature, which the binaries'
`ui` feature turns on) embeds it when `ZEABUR_UI_DIST` points at it. `zeabur-installer` serves its
own page with `zeabur-ui` and leaves Zeabur's interface out.
`cargo build --no-default-features` builds the API alone with Cargo only.

## Fixtures

Language-neutral test cases for the projection, change planning, diagnoses and modules:
Kubernetes objects in, the expected result out. Each crate keeps its own under
`crates/<crate>/fixtures/`, loaded with `zeabur-test-support`; [`fixtures/`](../../fixtures) holds the
format and the chart corpus.

## References

- A. Kladov, "Large Rust Workspaces," Aug. 22, 2021.
  <https://matklad.github.io/2021/08/22/large-rust-workspaces.html>
- D. L. Parnas, "On the criteria to be used in decomposing systems into modules," *Commun.
  ACM*, vol. 15, no. 12, pp. 1053–1058, Dec. 1972, doi:
  [10.1145/361598.361623](https://doi.org/10.1145/361598.361623).
- J. Lakos, *Large-Scale C++ Software Design*. Addison-Wesley, 1996.
- R. C. Martin, *Agile Software Development, Principles, Patterns, and Practices*. Prentice
  Hall, 2002 (the Acyclic and Stable Dependencies Principles).
- A. Cockburn, "The Hexagonal (Ports & Adapters) Architecture," 2005.
  <https://alistair.cockburn.us/hexagonal-architecture/>

[matklad-workspaces]: https://matklad.github.io/2021/08/22/large-rust-workspaces.html
[parnas]: https://doi.org/10.1145/361598.361623
[cockburn]: https://alistair.cockburn.us/hexagonal-architecture/
