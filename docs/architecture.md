# Architecture

engawa shows a Kubernetes cluster as projects and services, and lets people change them, without
holding any power of its own. It is a projection: Kubernetes objects are the only state.

## Principles

- **No power of its own.** Every read and write is made with the person's credential: a token
  issued by Genkan in the cluster, or their kubeconfig context on their machine. kube-apiserver
  decides. engawa does not impersonate, and its service account can do nothing a person cannot.
- **Kubernetes objects are the only state.** No database, no cache of Secret values, and no
  custom resources for layers L0 to L2. A project is a namespace; adopting one only adds a label.
  Removing engawa leaves the cluster as it was.
- **Project, not own.** Services are inferred from owner references, selectors, env and volume
  references, backendRefs, operator conventions and the labels of Helm, Argo CD and Flux. For
  objects a GitOps tool or Helm manages, engawa exports a patch or warns, and never fights over
  fields.
- **Suggest, not install.** engawa detects missing capabilities and writes the `helm` command, Flux
  `HelmRelease`, Argo CD `Application` or k3s `HelmChart`; people install them.

## Product layers

| Layer | What it adds | Runs as |
|---|---|---|
| L0 | read: projects, services, relations, logs, events | `engawactl ui` (local) |
| L1 | write: plans, JSON Patch, create and delete with preconditions, releases and rollback | `engawactl ui` (local) |
| L2 | the in-cluster server, sign-in through Genkan, elevation | `engawa` |
| L3, L4 | full sign-in, multi-tenancy | later |

A layer depends only on the layers below it; turning one off leaves those below working.

## Binaries

- **`engawa`**: the in-cluster server. It serves the API under `/api` and the embedded web
  interface, and signs people in as a confidential client of Genkan.
- **`engawactl`**: the command-line tool on a person's machine: `ui` (the interface with the
  current kubeconfig context, on a random loopback port), `mcp` (a read-only MCP server over
  stdio) and `doctor`.

The split follows Genkan (`genkan`/`genkanctl`): code only a person's machine runs never ships in
the server image.

## Crates

The code is split into small crates in one flat `crates/` directory:

- **Dependencies only point downwards**, with no cycles ([Kladov][matklad-workspaces]; Lakos and
  Martin, see [References](#references)). Each crate has a *level* in `Cargo.toml`
  (`[workspace.metadata.levels]`); `scripts/check-levels.sh` fails CI when a crate depends on one
  of the same or a higher level.
- **Each crate hides one design decision** behind a narrow interface ([Parnas][parnas]).
- **External systems are traits** with an in-memory and a Kubernetes implementation (ports and
  adapters, [Cockburn][cockburn]), so most tests need no cluster.
- **`engawa-kube` is the only crate that talks to kube-apiserver**, and it only offers clients
  bound to a person's credential.
- **Integrations with installed components are modules** (`engawa-modules`, one module per
  provider): they declare the resources they read and turn the objects into their own views.
  Only the composing crates (`engawa-api`, `engawa-mcp`, the binaries) may depend on a module
  crate (`[workspace.metadata.modules]`, also checked by `scripts/check-levels.sh`), so the core
  never knows which integrations exist.

```mermaid
flowchart BT
  core[engawa-core]
  model[engawa-model] --> core
  kube[engawa-kube] --> core
  adapters[engawa-adapters] --> core
  ui[engawa-ui]
  auth[engawa-auth] --> core
  graph[engawa-graph] --> model & adapters
  modules[engawa-modules] --> kube & adapters
  gaps[engawa-gaps] --> graph
  plan[engawa-plan] --> graph
  recommend[engawa-recommend] --> kube
  api[engawa-api] --> kube & gaps & plan & recommend & modules
  mcp[engawa-mcp] --> core & modules
  engawa([engawa]) --> api & ui & auth
  engawactl([engawactl]) --> api & mcp & ui
```

Arrows point from a crate to what it may depend on (transitive edges omitted).

| Level | Crate | What it hides |
|---|---|---|
| 0 | [`engawa-core`](../crates/engawa-core) | names shared with Kubernetes: label keys, the field manager, error categories |
| 1 | [`engawa-model`](../crates/engawa-model) | the projected view clients see: projects, services, their endpoints and routes, sources |
| 1 | [`engawa-kube`](../crates/engawa-kube) | how a request reaches kube-apiserver, always as the person |
| 1 | [`engawa-adapters`](../crates/engawa-adapters) | conventions of Helm, Argo CD, Flux, operators, Gateway API; label selectors |
| 1 | [`engawa-ui`](../crates/engawa-ui) | embedding and serving the web interface |
| 1 | [`engawa-auth`](../crates/engawa-auth) | signing in through Genkan (OAuth client), sessions, cross-site checks |
| 2 | [`engawa-graph`](../crates/engawa-graph) | the projection rules (pure functions, tested with `fixtures/`) |
| 2 | [`engawa-modules`](../crates/engawa-modules) | each installed component's resources and what engawa makes of them (one module per provider) |
| 3 | [`engawa-gaps`](../crates/engawa-gaps) | gap rules and plain-language diagnostics |
| 3 | [`engawa-plan`](../crates/engawa-plan) | change plans, minimal apply bodies, impact |
| 3 | [`engawa-recommend`](../crates/engawa-recommend) | the recommendations bundle and install suggestions |
| 4 | [`engawa-api`](../crates/engawa-api) | the HTTP API and its OpenAPI document |
| 4 | [`engawa-mcp`](../crates/engawa-mcp) | the read-only MCP server: its tools and the API reads behind them |
| 5 | [`engawa`](../crates/engawa), [`engawactl`](../crates/engawactl) | **binaries** |
| 6 | [`engawa-test-support`](../crates/engawa-test-support) | test helpers |

Every crate has a README that is also its crate-level documentation (`cargo doc --open`).

## Web interface

The interface's sources live in [`web/`](../web) (React, TanStack Router and Query, shadcn/ui on
Base UI with its own theme, Phosphor icons). Its build output is not committed:
`scripts/build-ui.sh` builds it, and `engawa-ui` embeds it when `ENGAWA_UI_DIST` points at it.
`cargo build --no-default-features` builds the API alone with Cargo only.

## Fixtures

[`fixtures/`](../fixtures) holds language-neutral test cases for the projection and change
planning: Kubernetes objects in, the expected result out.

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
