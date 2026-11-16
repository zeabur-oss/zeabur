<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/cover-dark.avif">
  <img alt="Zeabur: deploy like a PaaS, get standard Kubernetes; the review of a new service lists a Deployment, a Service and an Ingress with their YAML before it creates them" src="docs/images/cover-light.avif">
</picture>

# Zeabur

**Simple, secure deployments on your own Kubernetes.**

Zeabur is an open-source platform you host yourself. It is for small teams that run applications on Kubernetes without dedicated operations staff, for providers that manage clusters for their customers, and for people who run a cluster at home. It shows the cluster as projects and services, lists what a change will write before it applies it, and says in plain words what is wrong when something breaks. It works with the objects you already have, never acts with more rights than you have, and keeps no state of its own: no database, no custom resources.

## Your first task

Deploy a sample Nginx application, confirm it is up, break it on purpose with an image version that does not exist, then find the evidence on screen: the rollout is stuck, a new replica cannot download its image, and the events say so. [Deploy your first service](https://oss.zeabur.com/docs/zeabur/get-started/first-project/?utm_source=zeabur) follows it step by step and shows what success, failure, "this needs more rights" and "not known yet" look like.

## Choose a route

- **An existing cluster, with your kubeconfig.** You need a kubeconfig that reaches the cluster and `zeaburctl` on your computer. Rights to read let you see services, and rights to change let you deploy; the cluster's RBAC decides both. You don't need Zeabur Identity and nothing is installed in the cluster. `zeaburctl ui` opens Zeabur in your browser on your own computer, lists the cluster's Namespaces first, and says which rights are missing where it can't read. [Connect with a kubeconfig](https://oss.zeabur.com/docs/zeabur/get-started/kubeconfig/?utm_source=zeabur)
- **Your own server.** You need a Linux server with systemd and root access, at least 4 GiB of memory, and sign-in and admin domains that point to it. The installer checks the server, installs K3s (or RKE2), Zeabur Identity and Zeabur, and shows a link for the first administrator to register a passkey. [Install on a single server](https://oss.zeabur.com/docs/zeabur/get-started/quickstart/?utm_source=zeabur). A small machine only you use needs 1.5 GiB: the installer sets up KubeSolo without the dashboard or Zeabur Identity, and you open Zeabur with `zeaburctl ui` and the server's kubeconfig. [Personal use](https://oss.zeabur.com/docs/zeabur/install/personal/?utm_source=zeabur)

To put Zeabur in an existing cluster with sign-in for a team, see [Install in an existing cluster](https://oss.zeabur.com/docs/zeabur/install/existing-cluster/?utm_source=zeabur).

## What it does and doesn't do

The supported work, how Zeabur sits next to CI/CD and GitOps (services that Helm, Argo CD or Flux manage are read-only), what it writes to the cluster, what leaving looks like and the known limits are in [What Zeabur does and doesn't do](https://oss.zeabur.com/docs/zeabur/get-started/what-zeabur-does/?utm_source=zeabur). The [documentation](https://oss.zeabur.com/docs/zeabur/?utm_source=zeabur) is also in [繁體中文](https://oss.zeabur.com/zh-TW/docs/zeabur/?utm_source=zeabur) and [简体中文](https://oss.zeabur.com/zh-CN/docs/zeabur/?utm_source=zeabur), and [oss.zeabur.com/zeabur](https://oss.zeabur.com/zeabur/?utm_source=zeabur) shows what Zeabur does.

## Add more when you need it

- **Zeabur Identity** lets each person sign in, with roles per project. You don't need it to connect with a kubeconfig.
- **Zeabur Builder** builds images from a Git repository or a folder.
- **Zeabur Catalog** holds tested component versions and application templates.

## Tell us where it failed

Open the [feedback form](https://github.com/zeabur-oss/zeabur/issues/new?template=task-feedback.yml) and say which task you tried and where it stopped. Don't paste a kubeconfig, a token or logs.

## Experience program

When you join, Zeabur sends the version and how it is used, as fixed values only. It never sends domains, names, logs, kubeconfigs or the contents of your cluster. The installer joins by default and you can uncheck the box on its review page. Installing with Helm stays out until you turn it on. Set `DO_NOT_TRACK=1` to turn everything off. [Experience program](https://oss.zeabur.com/docs/zeabur/reference/telemetry/?utm_source=zeabur) lists every event and property.

[Releases](https://oss.zeabur.com/docs/zeabur/reference/releases/?utm_source=zeabur) lists what a release contains and how to verify it.

## What it looks like

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

<sub>Screenshots of the built-in demo data (`cargo run -p zeabur-api --example demo`), taken by `scripts/docs-shots.sh readme`.</sub>

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

The web interface is built with [@zeabur-oss/design-system](https://github.com/zeabur-oss/design-system)
(Apache-2.0), which contains shadcn/ui's components, theme and base styles (MIT), the Inter and
JetBrains Mono fonts (SIL Open Font License 1.1) and the Zeabur logo; its README lists their
notices. The built interface, embedded in the binary and the image, includes those fonts and
the logo.

The container image includes Mozilla's CA certificate bundle as packaged by nixpkgs (MPL-2.0).
