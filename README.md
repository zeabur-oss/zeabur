<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/cover-dark.avif">
  <img alt="Zeabur: deploy on your own server, the Zeabur way. Deploy from Git, Docker Compose or an app template, and it is running." src="docs/images/cover-light.avif">
</picture>

**English** · [繁體中文](README.zh-TW.md) · [简体中文](README.zh-CN.md)

# Zeabur OSS

**Deploy on your own server, the way you already do on Zeabur.** Projects, services, domains,
environment variables, and app templates work the way you know from Zeabur. You choose the hosting
provider and the plan.

## Coming from Zeabur?

What works the same way:

- **Projects and services.** A project holds a group of related services. Open it to see how each
  service is doing.
- **Ways to deploy.** Deploy from Git, Docker Compose, a Docker image, or an app template, or
  upload a folder.
- **Domains and variables.** Add a domain to a service, and change its environment variables on
  the service's page.
- **Logs and rollbacks.** Read a service's logs, and go back to the previous version when a new
  one has a problem.

What is different:

- **The server is yours.** Your services run on your own server, and you pay the cloud provider or data
  center you chose for it.
- **You look after it.** You or your team upgrade Zeabur OSS, back up data, and maintain the
  server.
- **It runs on Kubernetes.** Kubernetes is open-source software that runs containers on one or
  more servers, and Zeabur OSS runs on top of it. On a new server,
  [Zeatend](https://oss.zeabur.com/docs/zeabur/install/one-command/) sets it up first.

[Coming from Zeabur](https://oss.zeabur.com/docs/zeabur/get-started/from-zeabur-com/) shows where each of these is in the
documentation, with a screenshot for each step, and where to start. It is also in
[繁體中文](https://oss.zeabur.com/zh-TW/docs/zeabur/get-started/from-zeabur-com/) and
[简体中文](https://oss.zeabur.com/zh-CN/docs/zeabur/get-started/from-zeabur-com/).

## What it does for you

<table>
<tr>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-home-dark.avif"><img src="docs/images/readme-home-light.avif" alt="Home: what needs your attention across the cluster, grouped by where each problem comes from"></picture><br><b>What needs your attention</b>, across every project, grouped by where each problem comes from.</td>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-project-dark.avif"><img src="docs/images/readme-project-light.avif" alt="A project as a map of its services, databases and the connections between them"></picture><br><b>One page per project</b>: a map or a list, with what needs attention and what changed recently.</td>
</tr>
<tr>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-diagnosis-dark.avif"><img src="docs/images/readme-diagnosis-light.avif" alt="A service that keeps crashing: why, in plain words, and what to do"></picture><br><b>Diagnoses in plain words</b>, with what to do and the evidence.</td>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-rollout-dark.avif"><img src="docs/images/readme-rollout-light.avif" alt="A service rolling out, with its usage and a domain that the browser can't reach"></picture><br><b>Updates, usage, and domains</b>: how far an update has gone, CPU and memory against the limits, and whether each domain opens.</td>
</tr>
<tr>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-database-dark.avif"><img src="docs/images/readme-database-light.avif" alt="A PostgreSQL database: its state, a warning about one instance with no backups, and how to connect"></picture><br><b>Databases</b> with their connection details, and a warning when no standby would take over.</td>
<td width="50%"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/readme-cluster-dark.avif"><img src="docs/images/readme-cluster-light.avif" alt="The cluster: an end-of-life Kubernetes version and nodes running out of memory and disk"></picture><br><b>The cluster</b>: Kubernetes support dates, servers low on memory or disk, backups, notifications, and components.</td>
</tr>
</table>

<sub>Screenshots of the built-in demo data (`cargo run -p zeabur-api --example demo`), taken by `scripts/docs-shots.sh readme`.</sub>

## Get started

Pick the page that fits what you have. Each one takes you to the point where you sign in to
Zeabur OSS with a passkey:

- To let Claude Code or Codex do it: [Let an agent install for you](https://oss.zeabur.com/docs/zeabur/install/agent/)
- On a new Linux server: [Install with Zeatend](https://oss.zeabur.com/docs/zeabur/install/one-command/)
- On a Kubernetes cluster you already run: [Install Zeabur OSS in an existing cluster](https://oss.zeabur.com/docs/zeabur/install/existing-cluster/)
- To move your services off Zeabur: [Coming from Zeabur](https://oss.zeabur.com/docs/zeabur/get-started/from-zeabur-com/)

To look at a cluster from your own computer, `zeaburctl ui` opens Zeabur OSS in your browser with your
kubeconfig, the file `kubectl` uses to reach a cluster. See [zeaburctl](https://oss.zeabur.com/docs/zeabur/reference/zeaburctl/).

[Releases](https://oss.zeabur.com/docs/zeabur/reference/releases/) lists what a release contains
and how to verify it.

## Build from source

These commands download the source, enter the [Nix](https://nixos.org) dev shell, which has every
tool, and build and start Zeabur OSS:

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
- [`docs/`](docs/) holds the sources of the documentation, which are published to [oss.zeabur.com](https://oss.zeabur.com/docs/zeabur/).

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
