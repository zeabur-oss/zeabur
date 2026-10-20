# Zeabur

**Deploy and run your services on your own Kubernetes, simply and securely.**

Zeabur is an open-source platform you host yourself. It runs in your cluster (or on your
machine), shows the cluster as projects and services, explains what is wrong in plain words, and lets you
change things safely. It works with the objects you already have, never acts with more rights
than you have, and keeps no state of its own: no database, no custom resources.

> **Status: pre-alpha (0.0.1).** It works end to end, on your machine and in a cluster, but
> there are no published images or binaries yet, the API may still change, and it has been tried
> on few clusters. Do not point it at a cluster you cannot afford to break.

<p align="center">
  <img src="docs/images/home.png" alt="Home: what needs attention across all projects, and each project's health as Zeabur's project cards" width="900">
</p>

| | |
| --- | --- |
| ![A project's architecture: domains, services and data, with what each depends on](docs/images/architecture.png) | ![A service that keeps crashing, explained in plain words with what to do](docs/images/crash-loop.png) |
| **Architecture** of a project, from the objects it has. | **Diagnoses** in plain words, with the fix and the evidence. |
| ![The confirmation before a restart, in one plain sentence](docs/images/plan.png) | |
| **Every change is a plan** you confirm; the details are one click away. | |

<sub>Screenshots of the built-in demo data (`cargo run -p zeabur-api --example demo`).</sub>

## Principles

- **Your rights, not Zeabur's.** Every request is made with your own credential (a token from
  [Zeabur Identity](https://github.com/zeabur/identity), or your kubeconfig on your machine);
  kube-apiserver decides. In a cluster, admin rights are asked for only when your everyday ones
  are refused; they need your passkey again and last a few minutes.
- **Kubernetes objects are the only state.** No database and no custom resources: a project is
  a namespace. Remove Zeabur and the cluster is as it was.
- **Works with Helm, Argo CD and Flux.** Objects they manage are shown as such; Zeabur does not
  fight them for fields.
- **Suggests, does not install.** Missing pieces come with a `helm` command, a Flux
  `HelmRelease`, an Argo CD `Application` or a k3s `HelmChart`.
- **Light.** One static binary, small enough to run next to k3s on a 2 GB machine.

## What works today

- **Projects and services.** Namespaces shown as projects, and their workloads, Services,
  routes, ConfigMaps, Secrets and volumes grouped into services, with what Helm, Argo CD or Flux
  manages marked as such. The cluster page shows the nodes, how long the Kubernetes version is
  supported, upgrades, and which recommended components are installed.
- **Diagnoses in plain words.** Why a service is unhealthy (a crash loop, out of memory, an image
  that cannot be pulled, a failing probe, traffic that cannot reach it, a quota, no node with
  room, missing configuration), with the fix and the status fields and events it rests on; and
  what a project is missing (resource requests and limits, readiness probes, Ingresses that can
  move to Gateway API). A refusal by RBAC is told apart from a component that is not installed.
- **Changes as plan, confirm, apply.** Environment variables, ConfigMap and Secret keys,
  hostnames, restarts, scaling, probes, resources, volumes and mounts, an object's YAML,
  deleting an object, ready-made NetworkPolicies, adopting a namespace as a project and its
  details. A plan shows the fields it writes and who owns them now; the apply writes only those
  fields, and is refused if anything changed since the plan.
- **Releases and rollback** of a workload's pod template, through the same plans.
- **Logs and a terminal**: a pod's logs, followed live, and a shell in a container.
- **Usage**: CPU and memory now (metrics-server) and their history (a Prometheus-compatible
  store such as VictoriaMetrics).
- **Suggested installs** for missing components, as a `helm` command, a Flux `HelmRelease`, an
  Argo CD `Application` or a k3s `HelmChart`, from
  [Zeabur Catalog](https://github.com/zeabur/catalog).
- **In a cluster**, with its Helm chart: sign-in through Zeabur Identity with a passkey, admin rights
  confirmed with the passkey again, and nothing left behind after uninstalling.
- **`zeaburctl mcp`**: a read-only [Model Context Protocol](https://modelcontextprotocol.io)
  server over stdio, so an AI assistant can ask what is failing and why, with your rights and
  never a Secret's value.
- **`zeaburctl doctor`**: checks the cluster version, the Zeabur Identity setup and the recommended
  components, as text or JSON, with an exit code for scripts.

Not yet: creating new workloads, editing or deleting objects that Helm, Argo CD or Flux manage
(they can be restarted), more than one replica in a cluster, and signing in without Zeabur Identity.

## Quick start

On your machine, with your kubeconfig. Download `zeaburctl` for Linux or macOS (Apple silicon)
from a [release](https://github.com/zeabur/zeabur/releases) and verify it
([Releases](docs/releases.md#download-and-verify-zeaburctl)), or build it from source in the
[Nix](https://nixos.org) dev shell, which provides every tool:

```sh
git clone https://github.com/zeabur/zeabur && cd zeabur
nix develop
./scripts/build-ui.sh
ZEABUR_UI_DIST="$PWD/web/dist" cargo build --release --locked -p zeaburctl
./target/release/zeaburctl ui                  # --context <name> for another context
```

`zeaburctl ui` listens on a random loopback port, opens the browser, and makes every request
with your current kubeconfig context. `zeaburctl doctor` and `zeaburctl mcp` use it the same way.

In a cluster, for everyone who signs in through Zeabur Identity: [Install in a cluster](docs/install.md).

## Learn more

- [Install in a cluster](docs/install.md): the Helm chart, signing in through Zeabur Identity.
- [Releases](docs/releases.md): what a release contains, verifying it, publishing one.
- [Architecture](docs/architecture.md): principles, layers, crates.
- [Contributing](CONTRIBUTING.md): environment, checks, dependency policy, tests.
- [Testing](docs/testing.md): what is tested at which level, and what is not.

## Licence and credits

Apache-2.0. Contributions are accepted under the [Developer Certificate of Origin](DCO).

Third-party content in this repository:

- [Zeabur Catalog](https://github.com/zeabur/catalog) in
  `crates/zeabur-catalog/bundle` (Apache-2.0, `bundle/LICENSE`).
- Kubernetes' release schedule in `crates/zeabur-catalog/src/versions.rs`, from
  [kubernetes/website](https://github.com/kubernetes/website) `data/releases` (CC BY 4.0, The
  Kubernetes Authors).
- Rendered manifests of upstream Helm charts in `fixtures/charts`, test input only, each under
  its chart's licence (Apache-2.0 or MIT; listed in `fixtures/charts/README.md`).

The web interface is built with [@zeabur/design-system](https://github.com/zeabur/design-system)
(Apache-2.0), which contains shadcn/ui's components, theme and base styles (MIT), the Inter and
JetBrains Mono fonts (SIL Open Font License 1.1) and the Zeabur logo; its README lists their
notices. The built interface, embedded in the binary and the image, includes those fonts and
the logo.

The container image includes Mozilla's CA certificate bundle as packaged by nixpkgs (MPL-2.0).
