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
- **The rules are pure crates**: `zeabur-kernel`, `zeabur-model`, `zeabur-adapters`,
  `zeabur-graph`, `zeabur-gaps` and `zeabur-plan` perform no I/O and hold no key, depend only on
  each other (`[workspace.metadata.pure]`, also checked by `scripts/check-levels.sh`), build for
  WebAssembly and can be packaged on their own.

```mermaid
flowchart BT
  kernel[zeabur-kernel]
  core[zeabur-core] --> kernel
  model[zeabur-model] --> kernel
  adapters[zeabur-adapters] --> kernel
  ui[zeabur-ui]
  kube[zeabur-kube] --> core
  auth[zeabur-auth] --> core
  graph[zeabur-graph] --> model & adapters
  modules[zeabur-modules] --> kube & adapters
  gaps[zeabur-gaps] --> graph
  plan[zeabur-plan] --> graph
  recommend[zeabur-catalog] --> kube
  api[zeabur-api] --> kube & gaps & plan & recommend & modules
  zeabur([zeabur]) --> api & ui & auth
  zeaburctl([zeaburctl]) --> api & ui
```

Arrows point from a crate to what it may depend on (transitive edges omitted).

| Level | Crate | What it hides |
|---|---|---|
| 0 | [`zeabur-kernel`](../../crates/zeabur-kernel) | names shared with Kubernetes (label keys, the field manager), quantities, JSON Patch; no I/O, no cryptography |
| 1 | [`zeabur-core`](../../crates/zeabur-core) | error categories, request guards, cryptography |
| 1 | [`zeabur-model`](../../crates/zeabur-model) | the projected view clients see: projects, services, their endpoints and routes, sources |
| 1 | [`zeabur-adapters`](../../crates/zeabur-adapters) | conventions of Helm, Argo CD, Flux, operators, Gateway API; label selectors |
| 1 | [`zeabur-ui`](../../crates/zeabur-ui) | embedding and serving the web interface |
| 2 | [`zeabur-kube`](../../crates/zeabur-kube) | how a request reaches kube-apiserver, always as the person |
| 2 | [`zeabur-auth`](../../crates/zeabur-auth) | signing in through Zeabur Identity (OAuth client), sessions, cross-site checks |
| 2 | [`zeabur-graph`](../../crates/zeabur-graph) | the projection rules (pure functions, tested with `fixtures/`) |
| 3 | [`zeabur-modules`](../../crates/zeabur-modules) | each installed component's resources and what Zeabur makes of them (one module per provider) |
| 3 | [`zeabur-gaps`](../../crates/zeabur-gaps) | gap rules and plain-language diagnostics |
| 3 | [`zeabur-plan`](../../crates/zeabur-plan) | change plans, minimal apply bodies, impact |
| 3 | [`zeabur-catalog`](../../crates/zeabur-catalog) | the recommendations bundle and install suggestions |
| 4 | [`zeabur-api`](../../crates/zeabur-api) | the HTTP API and its OpenAPI document |
| 5 | [`zeabur`](../../crates/zeabur), [`zeaburctl`](../../crates/zeaburctl) | **binaries** |
| 0 | [`zeabur-fixtures`](../../crates/zeabur-fixtures) | loading the fixture cases (dev-dependency only) |
| 6 | [`zeabur-test-support`](../../crates/zeabur-test-support) | test helpers |
| 0 | [`zeabur-installer-plan`](../../crates/zeabur-installer-plan) | the installer's configuration, component selection and install plan; no I/O |
| 1 | [`zeabur-installer-host`](../../crates/zeabur-installer-host) | what the installer does to a host: facts, preflight, downloads, files, systemd, `kubectl` |
| 5 | [`zeabur-installer`](../../crates/zeabur-installer) | **binary**: installs RKE2, Zeabur Identity and Zeabur on a server ([guide](../get-started/quickstart.md)) |

Every crate has a README that is also its crate-level documentation (`cargo doc --open`).

## Web interface

The interface's sources live in [`web/`](../../web) (React, TanStack Router and Query, shadcn/ui on
Base UI with its own theme, Phosphor icons). Its build output is not committed:
`scripts/build-ui.sh` builds it, and `zeabur-ui` embeds it when `ZEABUR_UI_DIST` points at it.
`cargo build --no-default-features` builds the API alone with Cargo only.

## Fixtures

Language-neutral test cases for the projection, change planning, diagnoses and modules:
Kubernetes objects in, the expected result out. Each crate keeps its own under
`crates/<crate>/fixtures/`, loaded with `zeabur-fixtures`; [`fixtures/`](../../fixtures) holds the
format and the chart corpus.

## Packaging the pure crates

The pure crates and `zeabur-fixtures` are `publish = true`; nothing has been published yet. A
crate can only be published after the crates it depends on, dev-dependencies included, so the
order is:

1. `zeabur-kernel`, `zeabur-fixtures`
2. `zeabur-model`, `zeabur-adapters`
3. `zeabur-graph`
4. `zeabur-gaps`, `zeabur-plan`

`cargo package -p zeabur-kernel -p zeabur-fixtures -p zeabur-model -p zeabur-adapters -p
zeabur-graph -p zeabur-gaps -p zeabur-plan` builds all of them at once against each other. Each
workspace dependency on them in `Cargo.toml` names the workspace version, which must move with
`[workspace.package] version`.

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
