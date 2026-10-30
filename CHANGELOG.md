# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Before 1.0.0, a
minor or patch release may break the API.

## [Unreleased]

### Added

- The cluster page shows Zeabur's version and, from a daily check in the browser against
  GitHub's releases, whether a newer one is out, how to upgrade it the way it was installed, and
  whether the running version has known security problems. `zeaburctl` prints one line when a
  newer version is out, at most once a day. Turn either off with the chart's
  `updateCheck.enabled=false`, `DO_NOT_TRACK=1` or, for `zeaburctl`, `NO_UPDATE_NOTIFIER`
  ([Update check](docs/reference/telemetry.mdx#update-check)).
- `GET /api/v1/cluster/zeabur`: Zeabur's version, whether the update check is on, and how Zeabur
  was installed.

### Changed

- Release notes are the changelog section and a link to the documentation. The digests of the
  image, each platform's image and the chart are in the signed `digests.txt` asset, as
  [Releases](docs/reference/releases.mdx) shows.
- The image is pushed by digest with no `candidate-*` tags left in GHCR.

### Fixed

- Zeabur Proxy's image, `ghcr.io/zeabur-oss/proxy`, is published, signed and attested with each
  release, and the chart pins it by digest. 0.1.1 did not publish it, so an installation with
  VictoriaMetrics or VictoriaLogs left `zeabur-proxy` unable to pull its image.
- `zeabur-installer apply` moves the redirect URI of Zeabur's OAuth client to a changed
  `access.zeabur`, as it already did the home URL. It kept the first host's, so signing in to
  Zeabur at the new host failed with "This app's return address isn't registered".

## [0.1.1] - 2026-10-28

The first published release. 0.1.0 was never published: zeabur-oss withdrew it in every
repository, so everything listed under 0.1.0 in
[CHANGELOG.md](https://github.com/zeabur-oss/zeabur/blob/v0.1.1/CHANGELOG.md) ships first in
0.1.1.

### Changed

- Releases are built, signed and attested by the shared release workflow of
  [zeabur-oss/.github](https://github.com/zeabur-oss/.github). Every signature and attestation
  names `https://github.com/zeabur-oss/.github/.github/workflows/release.yml@…` as its signer,
  with `zeabur-oss/zeabur` and the release tag as the certificate's repository and ref; verify
  with `--certificate-identity-regexp`, `--certificate-github-workflow-repository` and
  `--certificate-github-workflow-ref` (cosign) or `--signer-workflow` and `--source-ref`
  (`gh attestation verify`), as the Releases reference shows.
- A release checks that the catalog and Zeabur Identity releases it pins were signed by that same
  shared workflow.

## [0.1.0] - 2026-10-28 [YANKED]

Never published.


### Added

- Projects and services: namespaces as projects, their objects grouped into services through
  owner and selector references, env and volume references, routes, operator conventions and
  the labels of Helm, Argo CD and Flux, with every value naming the object and field it came
  from.
- Diagnoses in plain words for unhealthy services (crash loops, failed Job runs, out of memory,
  image pulls, probes, unreachable traffic, quotas, scheduling, missing ConfigMaps and Secrets),
  each with its cause, fix and evidence; gap rules for resource requests and limits and readiness
  probes.
- Changes as plan, confirm, apply: environment variables, ConfigMap and Secret keys, hostnames,
  restarts, rollback to an earlier release, scaling, probes, resources, volumes and mounts, edits
  of an object's YAML, ready-made NetworkPolicies, deletes, adopting a namespace and a project's
  details. Applies are JSON Patches as the field manager `zeabur`, guarded by `test` operations
  and a signed plan token.
- Logs (followed live), a terminal in a container, events, CPU and memory now and over time,
  backups (K8up, Velero, CloudNativePG), NetworkPolicy reachability, and the cluster's nodes,
  Kubernetes version support and upgrades.
- Install suggestions for missing components from the
  [Zeabur Catalog](https://github.com/zeabur-oss/catalog), as a `helm`
  command, a Flux `HelmRelease`, an Argo CD `Application` or a k3s `HelmChart`.
- The web interface in English and Traditional Chinese (zh-TW), embedded in the binaries.
- `zeaburctl ui` (local mode with the kubeconfig) and `zeaburctl doctor` (cluster, Zeabur Identity
  and component checks as text or JSON).
- `zeabur`, the in-cluster server, with its Helm chart (`deploy/chart/zeabur`) and container
  image build: sign-in through Zeabur Identity as a confidential OAuth client.
- The HTTP API under `/api/v1`, described by a generated OpenAPI 3.1 document.
- The app template drawer shows what a template's author says to know before deploying it (the
  catalogue's `security.risks`, with how to contain each), folded under "Before you deploy". The
  template API (`GET /api/v1/cluster/templates`) carries them as `risks`.
- The installer checks each catalog chart and manifest URL against the catalog's `chartDigest`
  and `urlDigests` before applying it, and stops on a mismatch. The checked chart goes into the
  `HelmChart`'s `chartContent`, so helm-controller installs that archive instead of fetching
  the chart again. A catalog release without digests is installed without this check
  ([docs/zh-TW/reference/security.mdx](docs/zh-TW/reference/security.mdx)).
- Releases from CI only: the image `ghcr.io/zeabur-oss/zeabur` (linux/amd64, linux/arm64), the
  chart `oci://ghcr.io/zeabur-oss/charts/zeabur` pinned to the image's digest, and `zeaburctl`
  for Linux (amd64, arm64) and macOS (arm64), all signed with Sigstore and attested
  ([docs/reference/releases.md](docs/reference/releases.md)). The chart's `image.digest` value
  pins any image by content.
- IP limits on a service's Networking tab. "Who can open its domains" lists the allowed addresses
  in a Traefik `ipAllowList` Middleware (`<ingress>-ip-allow`) that each Traefik Ingress of the
  service lists first; an `HTTPRoute` refuses it. "Where its pods can connect" makes a
  `NetworkPolicy` for `Egress` (`<service>-egress-limit`) that keeps the project's pods and cluster
  DNS and allows the listed `ipBlock` ranges. Both are planned first, have no finalizer and work
  without Zeabur; the project's `zeabur-project` policy is not touched
  ([docs](docs/zh-TW/configure/ip-limits.mdx)).
- The installer adds the admission policy `zeabur-system-hosts` with Zeabur: an Ingress or
  HTTPRoute in a project cannot use Zeabur's or Zeabur Identity's domain, in a rule, in
  `spec.tls` or through a wildcard, and every rule outside the installer's namespaces must name a
  host. Routes that already break it stay; the security reference lists how to find them and how
  to switch the policy to `Warn`.
- Deploy from an uploaded folder: "Upload a folder" in the web interface (drop a folder or a
  `.zip`) and `zeaburctl up`. The client packs the folder (`.gitignore` and `.dockerignore`
  applied), Zeabur pushes it to the registry as an OCI artifact with the token of the `Build`'s
  upload Secret, and a `Build` with `spec.source.archive` builds it
  ([docs/zh-TW/deploy/upload.mdx](docs/zh-TW/deploy/upload.mdx)). Zeabur keeps no copy, and
  needs no new rights: a project Developer already may create and patch `Build`s and read Secrets.
  The changes `uploadToken` and `archiveDigest`, and
  `POST /api/v1/projects/{p}/builds/{b}/source`.
- `zeaburctl up` sends the archive through Zeabur in the cluster with the
  kubeconfig's bearer token, so it needs no registry address and no port-forward, and makes the
  service when it does not exist (`--project`, `--service`, `--port`, `--method`, `--yes`; asked
  for in a terminal) with the change `createUploadService`, which the web interface sends too.
  It finds Zeabur at the `url` of the ConfigMap `kube-public/zeabur-info`, which the chart
  publishes and every signed-in person may read (`--server` or `ZEABUR_SERVER` overrides it;
  `http://` only for localhost). It sends the token only when it is a JWT whose `iss` is the
  ConfigMap's `issuer`, so a cloud provider's, ServiceAccount or static token stays home. The
  server accepts `Authorization: Bearer` on the API when there is no session cookie, only for a
  token Zeabur Identity issued, acting as the token's holder; changes still carry `Origin` and
  `Zeabur-Request`.
- `zeaburctl up` runs in local mode when the token is not from Zeabur Identity or the cluster has
  no `zeabur-info` (an administrator's kubeconfig, a client certificate, a cloud provider's or
  ServiceAccount token): the same plans and upload run in the process, as in `zeaburctl ui`, and
  the credentials stay on the computer. It stops before uploading when the cluster serves no
  Zeabur Builder, and reaches an in-cluster registry through a port-forward. `zeabur-info` also
  has `registry` and `registryCa` (the CA's ConfigMap) from the chart's `builds`.
- The installer turns on Zeabur Proxy (`proxy.enabled`) when it installs VictoriaLogs, so the Logs
  tab shows the history of replicas that are gone on a team install.
- A new domain uses an Ingress unless "Add domain" names a Gateway listener.
- Maintenance on an Ingress adds a second Ingress (`<name>-maintenance`) that Traefik ranks first;
  on an HTTPRoute it sets the backends to weight 0 next to the page server. An HTTPRoute redirect
  adds one rule at the top.
- The Developer role can manage Traefik Middlewares in its projects.
- `ZEABUR_IDENTITY_PINNED` holds a release tag, Renovate moves it only to newer release tags,
  and `scripts/release-pins.sh` accepts a tag or a tagged commit.

### Security

- Every request to kube-apiserver carries the person's own credential; Zeabur's service
  account has no rights and no token, and nothing is impersonated.
- Secret values never appear in listings, YAML, `last-applied-configuration`, plans, MCP
  answers or logs; they are shown only on an explicit reveal, and a masked value is never
  written back.
- A write never takes over a field that Helm, Argo CD or Flux owns; a stale plan is refused.
- A strict Content Security Policy (no inline script, Trusted Types, no framing), `Host` checks
  and same-origin checks on every change, and `Cache-Control: no-store` on every API answer.
- Supply-chain policy: OSI-approved licences, aws-lc-rs as the only cryptographic backend, a
  seven-day minimum age for new crate and package releases, and a reviewed allowlist for
  shipped `unsafe` and native code.

[Unreleased]: https://github.com/zeabur-oss/zeabur/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/zeabur-oss/zeabur/releases/tag/v0.1.1
