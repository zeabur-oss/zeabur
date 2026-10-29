<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/cover-dark.avif">
  <img alt="Zeabur: deploy like a PaaS, get standard Kubernetes; the review of a new service lists a Deployment, a Service and an Ingress with their YAML before it creates them" src="docs/images/cover-light.avif">
</picture>

# Zeabur

**Simple, secure deployments on your own Kubernetes.**

Zeabur is an open-source platform you host yourself. It runs in your cluster (or on your
machine), shows the cluster as projects and services, explains what is wrong in plain words, and lets you
change things safely. It works with the objects you already have, never acts with more rights
than you have, and keeps no state of its own: no database, no custom resources.

**[oss.zeabur.com/zeabur](https://oss.zeabur.com/zeabur/)** shows what Zeabur does, and
**[the documentation](https://oss.zeabur.com/docs/zeabur/)** covers installing, deploying,
configuring and operating the cluster, and leaving Zeabur, in English,
[繁體中文](https://oss.zeabur.com/zh-TW/docs/zeabur/) and [简体中文](https://oss.zeabur.com/zh-CN/docs/zeabur/).

> **Status: pre-alpha (0.1.1).** The API may still change, and it has been tried on few
> clusters. Do not point it at a cluster you cannot afford to break.

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

## Get started

- **One Linux server**: the installer sets up K3s (or RKE2), Zeabur Identity and Zeabur.
  [Install on a single server](https://oss.zeabur.com/docs/zeabur/get-started/quickstart/)
- **An existing Kubernetes cluster**: install the Helm chart, with sign-in through Zeabur Identity.
  [Install in an existing cluster](https://oss.zeabur.com/docs/zeabur/get-started/install/)
- **Just you, with your kubeconfig**: `zeaburctl ui` opens Zeabur in your browser on your own
  computer. [Personal use](https://oss.zeabur.com/docs/zeabur/get-started/personal/) and
  [zeaburctl](https://oss.zeabur.com/docs/zeabur/reference/zeaburctl/)

[Releases](https://oss.zeabur.com/docs/zeabur/reference/releases/) lists what a release contains
and how to verify it.

## Build from source

In the [Nix](https://nixos.org) dev shell, which provides every tool:

```sh
git clone https://github.com/zeabur-oss/zeabur && cd zeabur
nix develop
./scripts/build-ui.sh
ZEABUR_UI_DIST="$PWD/web/dist" cargo build --release --locked -p zeaburctl
./target/release/zeaburctl ui                  # --context <name> for another context
```

`zeaburctl ui` listens on a random loopback port, opens the browser, and makes every request
with your current kubeconfig context.

## Contributing

- [Contributing](CONTRIBUTING.md): environment, checks, dependency policy, tests.
- [Developer docs](docs/internal/): crates, testing, wording, releasing.
- [`docs/`](docs/) holds the sources of the documentation; edit a page there and it is published
  to [oss.zeabur.com](https://oss.zeabur.com/docs/zeabur/).

## License and credits

Apache-2.0. Contributions are accepted under the [Developer Certificate of Origin](DCO).

Third-party content in this repository:

- [Zeabur Catalog](https://github.com/zeabur-oss/catalog), embedded from its release bundle at the
  commit in `flake.lock` (Apache-2.0).
- Kubernetes' release schedule in `crates/zeabur-catalog/src/versions.rs`, from
  [kubernetes/website](https://github.com/kubernetes/website) `data/releases` (CC BY 4.0, The
  Kubernetes Authors).
- Rendered manifests of upstream Helm charts in `fixtures/charts`, test input only, each under
  its chart's license (Apache-2.0 or MIT; listed in `fixtures/charts/README.md`).

The web interface is built with [@zeabur/design-system](https://github.com/zeabur-oss/design-system)
(Apache-2.0), which contains shadcn/ui's components, theme and base styles (MIT), the Inter and
JetBrains Mono fonts (SIL Open Font License 1.1) and the Zeabur logo; its README lists their
notices. The built interface, embedded in the binary and the image, includes those fonts and
the logo.

The container image includes Mozilla's CA certificate bundle as packaged by nixpkgs (MPL-2.0).
