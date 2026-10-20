# Architecture

Zeabur shows a Kubernetes cluster as projects and services, and lets people change them, without
holding any power of its own. It is a projection: Kubernetes objects are the only state.

## Principles

- **No power of its own.** Every read and write is made with the person's credential: a token
  issued by Zeabur Identity in the cluster, or their kubeconfig context on their machine. kube-apiserver
  decides. Zeabur does not impersonate, and its service account can do nothing a person cannot.
- **Kubernetes objects are the only state.** No database, no cache of Secret values, and no
  custom resources for layers L0 to L2. A project is a namespace; adopting one only adds a label.
  Removing Zeabur leaves the cluster as it was.
- **Project, not own.** Services are inferred from owner references, selectors, env and volume
  references, backendRefs, operator conventions and the labels of Helm, Argo CD and Flux. For
  objects a GitOps tool or Helm manages, Zeabur exports a patch or warns, and never fights over
  fields.
- **Suggest, not install.** Zeabur detects missing capabilities and writes the `helm` command, Flux
  `HelmRelease`, Argo CD `Application` or k3s `HelmChart`; people install them.

## Product layers

| Layer | What it adds | Runs as |
|---|---|---|
| L0 | read: projects, services, relations, logs, events | `zeaburctl ui` (local) |
| L1 | write: plans, JSON Patch, create and delete with preconditions, releases and rollback | `zeaburctl ui` (local) |
| L2 | the in-cluster server, sign-in through Zeabur Identity, elevation | `zeabur` |
| L3, L4 | full sign-in, multi-tenancy | later |

A layer depends only on the layers below it; turning one off leaves those below working.

## Binaries

- **`zeabur`**: the in-cluster server. It serves the API under `/api` and the embedded web
  interface, and signs people in as a confidential client of Zeabur Identity.
- **`zeaburctl`**: the command-line tool on a person's machine: `ui` (the interface with the
  current kubeconfig context, on a random loopback port), `mcp` (a read-only MCP server over
  stdio) and `doctor`.

The split follows Zeabur Identity (`zeabur-identity`/`zeabur-identityctl`): code only a person's machine runs never ships in
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
- **`zeabur-kube` is the only crate that talks to kube-apiserver**, and it only offers clients
  bound to a person's credential.
- **Integrations with installed components are modules** (`zeabur-modules`, one module per
  provider): they declare the resources they read and turn the objects into their own views.
  Only the composing crates (`zeabur-api`, `zeabur-mcp`, the binaries) may depend on a module
  crate (`[workspace.metadata.modules]`, also checked by `scripts/check-levels.sh`), so the core
  never knows which integrations exist.

```mermaid
flowchart BT
  core[zeabur-core]
  model[zeabur-model] --> core
  kube[zeabur-kube] --> core
  adapters[zeabur-adapters] --> core
  ui[zeabur-ui]
  auth[zeabur-auth] --> core
  graph[zeabur-graph] --> model & adapters
  modules[zeabur-modules] --> kube & adapters
  gaps[zeabur-gaps] --> graph
  plan[zeabur-plan] --> graph
  recommend[zeabur-catalog] --> kube
  api[zeabur-api] --> kube & gaps & plan & recommend & modules
  mcp[zeabur-mcp] --> core & modules
  zeabur([zeabur]) --> api & ui & auth
  zeaburctl([zeaburctl]) --> api & mcp & ui
```

Arrows point from a crate to what it may depend on (transitive edges omitted).

| Level | Crate | What it hides |
|---|---|---|
| 0 | [`zeabur-core`](../crates/zeabur-core) | names shared with Kubernetes: label keys, the field manager, error categories |
| 1 | [`zeabur-model`](../crates/zeabur-model) | the projected view clients see: projects, services, their endpoints and routes, sources |
| 1 | [`zeabur-kube`](../crates/zeabur-kube) | how a request reaches kube-apiserver, always as the person |
| 1 | [`zeabur-adapters`](../crates/zeabur-adapters) | conventions of Helm, Argo CD, Flux, operators, Gateway API; label selectors |
| 1 | [`zeabur-ui`](../crates/zeabur-ui) | embedding and serving the web interface |
| 1 | [`zeabur-auth`](../crates/zeabur-auth) | signing in through Zeabur Identity (OAuth client), sessions, cross-site checks |
| 2 | [`zeabur-graph`](../crates/zeabur-graph) | the projection rules (pure functions, tested with `fixtures/`) |
| 2 | [`zeabur-modules`](../crates/zeabur-modules) | each installed component's resources and what Zeabur makes of them (one module per provider) |
| 3 | [`zeabur-gaps`](../crates/zeabur-gaps) | gap rules and plain-language diagnostics |
| 3 | [`zeabur-plan`](../crates/zeabur-plan) | change plans, minimal apply bodies, impact |
| 3 | [`zeabur-catalog`](../crates/zeabur-catalog) | the recommendations bundle and install suggestions |
| 4 | [`zeabur-api`](../crates/zeabur-api) | the HTTP API and its OpenAPI document |
| 4 | [`zeabur-mcp`](../crates/zeabur-mcp) | the read-only MCP server: its tools and the API reads behind them |
| 5 | [`zeabur`](../crates/zeabur), [`zeaburctl`](../crates/zeaburctl) | **binaries** |
| 6 | [`zeabur-test-support`](../crates/zeabur-test-support) | test helpers |

Every crate has a README that is also its crate-level documentation (`cargo doc --open`).

## Explaining problems

Everything Zeabur says about a failure is read from the cluster at the time of the request, with
the person's rights, and names the objects and fields it rests on:

- **Diagnoses** (`zeabur-gaps`) say what fails, why, how to fix it, where the cause lies (the
  code, the configuration, the cluster, a component, an upstream registry) and who can fix it.
- **Incidents** group what one cluster cause breaks (an ingress controller or cluster DNS down,
  a node not ready), so the cluster's fault is not shown as each service's own.
- **Traces** follow each domain and private address hop by hop, from DNS to a ready pod.
- **Plan previews** say, before an apply, which pods are replaced, whether new ones fit the
  quotas and the nodes, and which data and fields go.
- **Timelines and rollouts** read releases, `managedFields`, events and container statuses; no
  history of Zeabur's own is kept, so what the cluster does not keep (who made a write, an
  event older than its TTL) is not shown.
- **Evidence bundles** put a service's problems, trace, events, previous logs and objects in one
  Markdown text, with no Secret and no variable value.

What cannot be read is named (`unreadable`), and a check that cannot be made is `unknown`, never
`ok`.

## Web interface

The interface's sources live in [`web/`](../web) (React, TanStack Router and Query, shadcn/ui on
Base UI with its own theme, Phosphor icons). Its build output is not committed:
`scripts/build-ui.sh` builds it, and `zeabur-ui` embeds it when `ZEABUR_UI_DIST` points at it.
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
