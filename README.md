<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/cover-dark.avif">
  <img alt="Zeabur: deploy like a PaaS, get standard Kubernetes; the review of a new service lists a Deployment, a Service and an Ingress with their YAML before it creates them" src="docs/images/cover-light.avif">
</picture>

# Zeabur

**Deploy and run your services on your own Kubernetes, simply and securely.**

Zeabur is an open-source platform you host yourself. It runs in your cluster (or on your
machine), shows the cluster as projects and services, explains what is wrong in plain words, and lets you
change things safely. It works with the objects you already have, never acts with more rights
than you have, and keeps no state of its own: no database, no custom resources.

> **Status: pre-alpha (0.1.1).** It works end to end, on your machine and in a cluster, but
> there are no published images or binaries yet, the API may still change, and it has been tried
> on few clusters. Do not point it at a cluster you cannot afford to break.

<table>
<tr>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-home-dark.avif"><img src="docs/images/readme-home-light.avif" alt="Home: what needs you across the cluster, grouped by where each problem comes from"></picture><br><b>What needs you</b>, across every project, grouped by where each problem comes from.</td>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-project-dark.avif"><img src="docs/images/readme-project-light.avif" alt="A project as a map of its services, databases and the connections between them"></picture><br><b>One page per project</b>: a map or a list, with what needs attention and what happened.</td>
</tr>
<tr>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-diagnosis-dark.avif"><img src="docs/images/readme-diagnosis-light.avif" alt="A service that keeps crashing: why, in plain words, and what to do"></picture><br><b>Diagnoses in plain words</b>, with what to do and the evidence.</td>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-rollout-dark.avif"><img src="docs/images/readme-rollout-light.avif" alt="A service rolling out, with its usage and a domain that the browser can't reach"></picture><br><b>Rollouts, usage and domains</b>: progress, CPU and memory against the limits, and whether each domain answers.</td>
</tr>
<tr>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-database-dark.avif"><img src="docs/images/readme-database-light.avif" alt="A PostgreSQL database: its state, a warning about one instance with no backups, and how to connect"></picture><br><b>Databases</b> with their connection details and a warning when nothing would take over.</td>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-cluster-dark.avif"><img src="docs/images/readme-cluster-light.avif" alt="The cluster: an end-of-life Kubernetes version and nodes running out of memory and disk"></picture><br><b>The cluster</b>: Kubernetes support dates, node pressure, backups, notifications and components.</td>
</tr>
</table>

<sub>Screenshots of the built-in demo data (`cargo run -p zeabur-api --example demo`), taken by `scripts/docs-shots.sh docs/readme-shots.yaml`.</sub>

## Principles

- **Your rights, not Zeabur's.** Every request is made with your own credential (a token from
  [Zeabur Identity](https://github.com/zeabur-oss/identity), or your kubeconfig on your machine);
  kube-apiserver decides. In a cluster, admin rights are asked for only when your everyday ones
  are refused; they need your passkey again and last a few minutes.
- **Kubernetes objects are the only state.** No database and no custom resources: a project is
  a namespace. Remove Zeabur and the cluster is as it was.
- **Works with Helm, Argo CD and Flux.** Objects they manage are shown as such; Zeabur does not
  fight them for fields.
- **Suggests, and installs only through a plan.** Missing pieces come with a `helm` command, a
  Flux `HelmRelease`, an Argo CD `Application` or a K3s/RKE2 `HelmChart`; when the cluster runs that
  controller, a plan you review creates the objects.
- **Light.** One static binary, small enough to run next to K3s or RKE2 on a 2 GB machine.

## What works today

- **Projects and services.** Namespaces shown as projects, and their workloads, Services,
  routes, ConfigMaps, Secrets and volumes grouped into services, with what Helm, Argo CD or Flux
  manages marked as such. The cluster page shows the API server's readiness, the nodes, how long
  the Kubernetes version is supported, upgrades, and which recommended components are installed.
- **Diagnoses in plain words.** Why a service is unhealthy (a crash loop, out of memory, an image
  that cannot be pulled, a failing probe, traffic that cannot reach it, a quota, no node with
  room, missing configuration), with the fix and the status fields and events it rests on; and
  what a project is missing (resource requests and limits, readiness probes, Ingresses that can
  move to Gateway API). A refusal by RBAC is told apart from a component that is not installed.
- **Changes as plan, confirm, apply.** Environment variables, ConfigMap and Secret keys,
  hostnames, restarts, scaling, probes, resources, volumes and mounts (on a chosen storage
  class, grown only where the class allows), an object's YAML, deleting an object, ready-made
  NetworkPolicies, adopting a namespace as a project and its details. A plan shows the fields it writes and who owns them now; the apply writes only those
  fields, and is refused if anything changed since the plan.
- **Releases and rollback** of a workload's pod template, through the same plans.
- **Cluster backups** with [Velero](https://velero.io): where they go (S3-compatible storage,
  keys kept in a Secret), a schedule for the whole cluster or chosen projects with the files in
  volumes copied, and each backup's outcome in plain words, set through the same plans.
  Databases keep their own backups; restoring is left to the `velero` command-line tool.
- **Logs and a terminal**: a pod's logs, followed live, and a shell in a container.
- **Usage**: CPU and memory now (metrics-server) and their history (a Prometheus-compatible
  store such as VictoriaMetrics).
- **Suggested installs** for missing components, as a `helm` command to copy, or a Flux
  `HelmRelease`, an Argo CD `Application` or a K3s/RKE2 `HelmChart` that a plan creates, from
  [Zeabur Catalog](https://github.com/zeabur-oss/catalog).
- **In a cluster**, with its Helm chart: sign-in through Zeabur Identity with a passkey, admin rights
  confirmed with the passkey again, and nothing left behind after uninstalling.
- **`zeaburctl doctor`**: checks the cluster version, the Zeabur Identity setup and the recommended
  components, as text or JSON, with an exit code for scripts.

Not yet: creating new workloads, editing or deleting objects that Helm, Argo CD or Flux manage
(they can be restarted), more than one replica in a cluster, and signing in without Zeabur Identity.

## Quick start

On your machine, with your kubeconfig. Download `zeaburctl` for Linux or macOS (Apple silicon)
from a [release](https://github.com/zeabur-oss/zeabur/releases) and verify it
([Releases](docs/reference/releases.md#download-and-verify-zeaburctl)), or build it from source in the
[Nix](https://nixos.org) dev shell, which provides every tool:

```sh
git clone https://github.com/zeabur-oss/zeabur && cd zeabur
nix develop
./scripts/build-ui.sh
ZEABUR_UI_DIST="$PWD/web/dist" cargo build --release --locked -p zeaburctl
./target/release/zeaburctl ui                  # --context <name> for another context
```

`zeaburctl ui` listens on a random loopback port, opens the browser, and makes every request
with your current kubeconfig context. `zeaburctl doctor` uses it the same way.

In a cluster, for everyone who signs in through Zeabur Identity: [Install on an existing cluster](docs/get-started/install.md).
On a fresh Linux server, `zeabur-installer` installs RKE2, Zeabur Identity and Zeabur in one
step: [Quickstart](docs/get-started/quickstart.md).

## Learn more

- [Documentation](docs/): installing, deploying, configuring and operating the cluster,
  and leaving Zeabur.
- [Releases](docs/reference/releases.md): what a release contains and how to verify it.
- [Architecture](docs/reference/architecture.md): principles and how Zeabur explains problems.
- [Contributing](CONTRIBUTING.md): environment, checks, dependency policy, tests.
- [Developer docs](docs/internal/): crates, testing, wording, releasing.

## Licence and credits

Apache-2.0. Contributions are accepted under the [Developer Certificate of Origin](DCO).

Third-party content in this repository:

- [Zeabur Catalog](https://github.com/zeabur-oss/catalog), embedded from its release bundle at the
  commit in `flake.lock` (Apache-2.0).
- Kubernetes' release schedule in `crates/zeabur-catalog/src/versions.rs`, from
  [kubernetes/website](https://github.com/kubernetes/website) `data/releases` (CC BY 4.0, The
  Kubernetes Authors).
- Rendered manifests of upstream Helm charts in `fixtures/charts`, test input only, each under
  its chart's licence (Apache-2.0 or MIT; listed in `fixtures/charts/README.md`).

The web interface is built with [@zeabur/design-system](https://github.com/zeabur-oss/design-system)
(Apache-2.0), which contains shadcn/ui's components, theme and base styles (MIT), the Inter and
JetBrains Mono fonts (SIL Open Font License 1.1) and the Zeabur logo; its README lists their
notices. The built interface, embedded in the binary and the image, includes those fonts and
the logo.

The container image includes Mozilla's CA certificate bundle as packaged by nixpkgs (MPL-2.0).
