# Changelog

All notable changes to Zeabur. Versions follow [Semantic Versioning](https://semver.org/); before 1.0.0, a minor or patch release may break the API. Each section comes from the change files in `.changeset/` (CONTRIBUTING.md, "Releases").

## 0.1.2-beta.0 (2026-11-01)

Coming from 0.1.1, read Breaking Changes first: an installer configuration with `preset` is refused, and upgrading the chart with `--reuse-values` keeps the old image (see the fix about `--reset-then-reuse-values` below).

### Breaking Changes

#### The installer configuration has no `preset`

`plan` and `apply` refuse a `/etc/zeabur-installer/config.yaml` that still has `preset`; remove the line. A team install takes the catalog's recommended components for its profile, plus Zeabur Builder, Keel and CloudNativePG; `ingress`, `certificates` and `metrics/metrics-server` are always installed. `enable` and `disable` change that list, by capability or component ID.

#### `container-registry/zot` is now `builds/zeabur-builder`

Zeabur Builder and the zot registry it pushes to are one component. `disable` still takes the old ID and warns with the new one, and `enable` with the old ID stops with an error that names it. On a server where 0.1.1 installed zot, `apply` adds Builder and upgrades zot's release instead of deleting it; `disable: [builds/zeabur-builder]` leaves zot running, with a warning.

#### A join configuration names the registry `registry`, not `zot`

`node add` prints `registry: {host, clusterIp, ca}`. Run `node add` again to join a node with this installer.

#### API: `installationsProblem` is a `Problem`, and every operation has a verb-first ID

`GET /api/v1/cluster/github-app` returns `installationsProblem` as a whole `Problem` (summary and fix) instead of a string. Every operation in `openapi.yaml` has an explicit camelCase `operationId` (`getProject`, `listProjects`, `getWhoAmI`), so a client generated from the description changes its method names.

### Features

#### Zeabur and `zeaburctl` tell people when a newer version is out

The cluster page shows Zeabur's version. Once a day, the browser checks GitHub's releases. When a newer one is out, the page says so and shows how to upgrade for the way Zeabur was installed. It also says when the running version has known security problems. `zeaburctl` prints one line when a newer version is out, at most once a day. Turn the update check off with the installer's review page checkbox or `updateCheck: false`, the chart's `updateCheck.enabled=false`, `DO_NOT_TRACK=1` or `NO_UPDATE_NOTIFIER`. See [Update check](docs/reference/telemetry.mdx#update-check). `GET /api/v1/cluster/zeabur` returns Zeabur's version, whether the update check is on, and how Zeabur was installed (`installer`, `helm`, `argo-cd`, `flux` or `zeaburctl`).

#### An experience program counts installations and daily active people

Zeabur sends one event the first time each person uses its web interface in a UTC day, and the installer sends one when `apply` starts, finishes or fails. Each event carries a random installation ID, the versions, the profile and the distribution; never domains, host names, IP addresses, names or the contents of the cluster. The installer joins by default: uncheck the box on its review page, pass `--no-telemetry`, set `telemetry: false` or `DO_NOT_TRACK=1`. With Helm, Zeabur stays out until `telemetry.enabled=true` with a `telemetry.installationId`. `ZEABUR_TELEMETRY=debug` prints what would be sent. See [Experience program](docs/reference/telemetry.mdx).

#### On an installed server, `plan` says what `apply` will change

`plan` reads the record that `apply` leaves in `kube-system/zeabur-installer` and starts with `Changes since the last apply`: the K3s, RKE2 or KubeSolo release, the host files that change and what restarts, components added, replaced or no longer updated, and each chart's version, or "values changed" when only its values differ. `plan --json` has the same under `changes`, and the installer page's review step shows it as "Changes to this server". A component that became a default after the last install is listed but installed only when `enable` names it. `plan` and `apply` stop when a newer installer wrote the record, and name the version to use.

#### `zeabur-installer` installs only components the catalog tested on the server's distribution

`plan` leaves out a default component that was not tested on K3s, RKE2 or KubeSolo, and installs one the configuration asks for with a warning. `doctor` and `apply` run the health checks the catalog ships with each component, and `doctor` prints the catalog's advice when one fails.

#### The installer uses Zeabur Catalog v0.1.2

Component summaries and their zh-TW and zh-CN translations come from the catalog. On KubeSolo, the MariaDB operator now reconciles a new cluster's objects, and Longhorn is no longer offered. See [its release notes](https://github.com/zeabur-oss/catalog/releases/tag/v0.1.2).

#### Personal installs run KubeSolo v1.2.3 and can add databases

`enable: [databases]` adds CloudNativePG on a personal install; it is not a default.

#### The installer fixes the node's name at install

A hostname change, such as Parallels' hostname sync or a cloud renaming a server on reboot, made K3s register a second node with the same address. The installer now writes the name once (`host.nodeName`, else the hostname when it is valid and free, else `node-` and 6 characters derived from `/etc/machine-id`) and refuses to rename an installed node. The installer page has a "Node name" field, read-only once installed. A joining node avoids names already in the cluster.

#### `sources.proxyImage` replaces Zeabur Proxy's image

It is written to the chart as `proxy.image` when VictoriaLogs or VictoriaMetrics is installed. Unset, the chart's own image is used.

#### A `.zip` upload keeps file modes and symbolic links

Zeabur reads the execute bit and symbolic links of entries made on Unix, as a `.tar.gz` upload already kept them. A link that points outside the folder, or an entry under a link, is refused.

#### A release tag may carry a SemVer pre-release part (`v0.1.2-beta.1`)

Such a release is a GitHub pre-release, never the latest, and has its own section here.

### Fixes

- The image is pushed by digest with no `candidate-*` tags left in GHCR

#### Upgrading with `helm upgrade --reuse-values` kept the old image

A release chart sets `image.digest` in its values, and `--reuse-values` keeps the last release's values, so the Deployment kept running the previous version. The install guide now upgrades with `--reset-then-reuse-values`; use it when you upgrade from 0.1.1.

#### Upgrading an installation that has the VictoriaMetrics operator no longer stops on a field conflict over its CRDs

`zeabur-installer apply` now applies only the `crds/` folders Helm would install, and leaves CRDs a chart makes from its templates to Helm.

#### A chart's CRDs are updated when it upgrades

Before applying a catalog chart, `apply` applies the `crds/` of the chart and its subcharts with server-side apply, so an upgrade such as K8up's no longer leaves its CRDs behind. CRDs are never deleted.

#### `zeabur-installer apply` moves the redirect URI of Zeabur's OAuth client

It follows a changed `access.zeabur`, as it already did the home URL. It kept the first host's, so signing in to Zeabur at the new host failed with "This app's return address isn't registered".

#### Zeabur Proxy's image is published with each release

`ghcr.io/zeabur-oss/proxy` is signed and attested with each release, and the chart pins it by digest. 0.1.1 did not publish it, so an installation with VictoriaMetrics or VictoriaLogs left `zeabur-proxy` unable to pull its image.

#### The installer closes the CIS gaps kube-bench found that it can fix

`local-path-storage`'s `default` ServiceAccount no longer mounts a token (5.1.5). On RKE2, the server certificates are no longer world-readable (1.1.20), and the kubelet gets the same cipher list and `streaming-connection-idle-timeout=5m` as on K3s (4.2.12).

#### `zeaburctl doctor` no longer warns about Zeabur Identity where nothing needs it

On a personal installation, or a cluster opened only with `zeaburctl ui`, the Zeabur Identity checks are skipped. They still run when a project you can see runs the Zeabur dashboard, or when doctor cannot tell.

#### A container is "crash looping" only by the kubelet's own rule

A ready container that had restarted once showed as crash looping. The status light and the diagnosis now share the kubelet's back-off rule: the container is in `CrashLoopBackOff`, or it has restarted at least twice, its last run lasted under 10 minutes and its current run has not lasted 10 minutes yet. The exit code only picks the wording.

#### A GitHub App Secret with an `api-url` that is not https is refused where it is read

Zeabur answers with "`api-url` must be an https URL" and how to fix it, localized, instead of failing later in Zeabur Builder.

#### The installer's components page no longer says a component left out for memory is about building apps

Any component a personal install leaves out because the server has too little memory now reads "Needs N MiB of memory, and this server has less".

#### Signing in from far away takes one redirect less

A page opened without a session goes straight to Zeabur Identity instead of through `/auth/login` first. The API gzips JSON answers from a kilobyte up.

#### The interface answers sooner on a slow connection

A page that takes longer than 100 ms to open shows a progress bar over the old page, and the plan dialog shows its title and "Checking changes…" while its code loads. The project page no longer waits for incidents and suggestions. A read that succeeds takes one request to the Kubernetes API instead of two.

#### Zeabur is smaller and starts with less memory

Zeabur registers its routes without building its OpenAPI document (3.4 MB off the binary, and the 433 KB document no longer kept in memory). It embeds its web files once, brotli-compressed, and decodes a copy only for a browser that does not accept brotli (4.7 MB fewer embedded bytes). Idle HTTP connections close 30 seconds after their last use.

#### Smaller fixes in the interface

In Safari, opening a service's quick look no longer scrolls the project map. A menu pressed before its code arrived no longer closes straight away. The zh-TW and zh-CN interface strings were reworded in everyday speech.

#### The docs say what to use on Windows

Run the Linux `zeaburctl` in WSL; there is no Windows build yet.

#### Zeabur Identity v0.1.2-beta.0

A release installs it in place of v0.1.1. It stores the sign-in pages' styles and scripts brotli-compressed, and its image is pushed by digest without `candidate-*` tags ([its release notes](https://github.com/zeabur-oss/identity/releases/tag/v0.1.2-beta.0)).

#### Release notes are the changelog section and a link to the documentation

The digests of the image, each platform's image and the chart are in the signed `digests.txt` asset, as [Releases](docs/reference/releases.mdx) shows.

### Security

#### Zeabur and Zeabur Proxy no longer write Kubernetes response bodies to their logs

kube-rs logged a response body it could not parse, and some refusal messages, at `WARN`, which the default level shows. Such a body can be a Secret or a list of Secrets, and an admission policy's refusal can quote a Secret's value. Both binaries now drop kube-rs's log lines; Zeabur reports the same errors itself.

#### The form's YAML no longer passes through `sessionStorage`

Going from a form to its YAML kept the YAML, which can hold registry passwords and secret variable values, in `sessionStorage`, which a browser may write to disk to restore a session. It now stays in memory.

## 0.1.1 (2026-10-28)

The first published release. 0.1.0 was never published: zeabur-oss withdrew it in every repository, so everything listed under 0.1.0 in [CHANGELOG.md](https://github.com/zeabur-oss/zeabur/blob/v0.1.1/CHANGELOG.md) ships first in 0.1.1.

### Changed

- Releases are built, signed and attested by the shared release workflow of [zeabur-oss/.github](https://github.com/zeabur-oss/.github). Every signature and attestation names `https://github.com/zeabur-oss/.github/.github/workflows/release.yml@…` as its signer, with `zeabur-oss/zeabur` and the release tag as the certificate's repository and ref; verify with `--certificate-identity-regexp`, `--certificate-github-workflow-repository` and `--certificate-github-workflow-ref` (cosign) or `--signer-workflow` and `--source-ref` (`gh attestation verify`), as the Releases reference shows.
- A release checks that the catalog and Zeabur Identity releases it pins were signed by that same shared workflow.

## 0.1.0 (2026-10-28)

Never published.

### Added

- Projects and services: namespaces as projects, their objects grouped into services through owner and selector references, env and volume references, routes, operator conventions and the labels of Helm, Argo CD and Flux, with every value naming the object and field it came from.
- Diagnoses in plain words for unhealthy services (crash loops, failed Job runs, out of memory, image pulls, probes, unreachable traffic, quotas, scheduling, missing ConfigMaps and Secrets), each with its cause, fix and evidence; gap rules for resource requests and limits and readiness probes.
- Changes as plan, confirm, apply: environment variables, ConfigMap and Secret keys, hostnames, restarts, rollback to an earlier release, scaling, probes, resources, volumes and mounts, edits of an object's YAML, ready-made NetworkPolicies, deletes, adopting a namespace and a project's details. Applies are JSON Patches as the field manager `zeabur`, guarded by `test` operations and a signed plan token.
- Logs (followed live), a terminal in a container, events, CPU and memory now and over time, backups (K8up, Velero, CloudNativePG), NetworkPolicy reachability, and the cluster's nodes, Kubernetes version support and upgrades.
- Install suggestions for missing components from the [Zeabur Catalog](https://github.com/zeabur-oss/catalog), as a `helm` command, a Flux `HelmRelease`, an Argo CD `Application` or a k3s `HelmChart`.
- The web interface in English and Traditional Chinese (zh-TW), embedded in the binaries.
- `zeaburctl ui` (local mode with the kubeconfig) and `zeaburctl doctor` (cluster, Zeabur Identity and component checks as text or JSON).
- `zeabur`, the in-cluster server, with its Helm chart (`deploy/chart/zeabur`) and container image build: sign-in through Zeabur Identity as a confidential OAuth client.
- The HTTP API under `/api/v1`, described by a generated OpenAPI 3.1 document.
- The app template drawer shows what a template's author says to know before deploying it (the catalogue's `security.risks`, with how to contain each), folded under "Before you deploy". The template API (`GET /api/v1/cluster/templates`) carries them as `risks`.
- The installer checks each catalog chart and manifest URL against the catalog's `chartDigest` and `urlDigests` before applying it, and stops on a mismatch. The checked chart goes into the `HelmChart`'s `chartContent`, so helm-controller installs that archive instead of fetching the chart again. A catalog release without digests is installed without this check ([docs/zh-TW/reference/security.mdx](docs/zh-TW/reference/security.mdx)).
- Releases from CI only: the image `ghcr.io/zeabur-oss/zeabur` (linux/amd64, linux/arm64), the chart `oci://ghcr.io/zeabur-oss/charts/zeabur` pinned to the image's digest, and `zeaburctl` for Linux (amd64, arm64) and macOS (arm64), all signed with Sigstore and attested ([docs/reference/releases.md](docs/reference/releases.md)). The chart's `image.digest` value pins any image by content.
- IP limits on a service's Networking tab. "Who can open its domains" lists the allowed addresses in a Traefik `ipAllowList` Middleware (`<ingress>-ip-allow`) that each Traefik Ingress of the service lists first; an `HTTPRoute` refuses it. "Where its pods can connect" makes a `NetworkPolicy` for `Egress` (`<service>-egress-limit`) that keeps the project's pods and cluster DNS and allows the listed `ipBlock` ranges. Both are planned first, have no finalizer and work without Zeabur; the project's `zeabur-project` policy is not touched ([docs](docs/zh-TW/configure/ip-limits.mdx)).
- The installer adds the admission policy `zeabur-system-hosts` with Zeabur: an Ingress or HTTPRoute in a project cannot use Zeabur's or Zeabur Identity's domain, in a rule, in `spec.tls` or through a wildcard, and every rule outside the installer's namespaces must name a host. Routes that already break it stay; the security reference lists how to find them and how to switch the policy to `Warn`.
- Deploy from an uploaded folder: "Upload a folder" in the web interface (drop a folder or a `.zip`) and `zeaburctl up`. The client packs the folder (`.gitignore` and `.dockerignore` applied), Zeabur pushes it to the registry as an OCI artifact with the token of the `Build`'s upload Secret, and a `Build` with `spec.source.archive` builds it ([docs/zh-TW/deploy/upload.mdx](docs/zh-TW/deploy/upload.mdx)). Zeabur keeps no copy, and needs no new rights: a project Developer already may create and patch `Build`s and read Secrets. The changes `uploadToken` and `archiveDigest`, and `POST /api/v1/projects/{p}/builds/{b}/source`.
- `zeaburctl up` sends the archive through Zeabur in the cluster with the kubeconfig's bearer token, so it needs no registry address and no port-forward, and makes the service when it does not exist (`--project`, `--service`, `--port`, `--method`, `--yes`; asked for in a terminal) with the change `createUploadService`, which the web interface sends too. It finds Zeabur at the `url` of the ConfigMap `kube-public/zeabur-info`, which the chart publishes and every signed-in person may read (`--server` or `ZEABUR_SERVER` overrides it; `http://` only for localhost). It sends the token only when it is a JWT whose `iss` is the ConfigMap's `issuer`, so a cloud provider's, ServiceAccount or static token stays home. The server accepts `Authorization: Bearer` on the API when there is no session cookie, only for a token Zeabur Identity issued, acting as the token's holder; changes still carry `Origin` and `Zeabur-Request`.
- `zeaburctl up` runs in local mode when the token is not from Zeabur Identity or the cluster has no `zeabur-info` (an administrator's kubeconfig, a client certificate, a cloud provider's or ServiceAccount token): the same plans and upload run in the process, as in `zeaburctl ui`, and the credentials stay on the computer. It stops before uploading when the cluster serves no Zeabur Builder, and reaches an in-cluster registry through a port-forward. `zeabur-info` also has `registry` and `registryCa` (the CA's ConfigMap) from the chart's `builds`.
- The installer turns on Zeabur Proxy (`proxy.enabled`) when it installs VictoriaLogs, so the Logs tab shows the history of replicas that are gone on a team install.
- A new domain uses an Ingress unless "Add domain" names a Gateway listener.
- Maintenance on an Ingress adds a second Ingress (`<name>-maintenance`) that Traefik ranks first; on an HTTPRoute it sets the backends to weight 0 next to the page server. An HTTPRoute redirect adds one rule at the top.
- The Developer role can manage Traefik Middlewares in its projects.
- `ZEABUR_IDENTITY_PINNED` holds a release tag, Renovate moves it only to newer release tags, and `scripts/release-pins.sh` accepts a tag or a tagged commit.

### Security

- Every request to kube-apiserver carries the person's own credential; Zeabur's service account has no rights and no token, and nothing is impersonated.
- Secret values never appear in listings, YAML, `last-applied-configuration`, plans, MCP answers or logs; they are shown only on an explicit reveal, and a masked value is never written back.
- A write never takes over a field that Helm, Argo CD or Flux owns; a stale plan is refused.
- A strict Content Security Policy (no inline script, Trusted Types, no framing), `Host` checks and same-origin checks on every change, and `Cache-Control: no-store` on every API answer.
- Supply-chain policy: OSI-approved licences, aws-lc-rs as the only cryptographic backend, a seven-day minimum age for new crate and package releases, and a reviewed allowlist for shipped `unsafe` and native code.
