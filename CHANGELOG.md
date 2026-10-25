# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Before 1.0.0, a
minor or patch release may break the API.

## [Unreleased]

### Added

- Releases from CI only: the image `ghcr.io/zeabur-oss/zeabur` (linux/amd64, linux/arm64), the
  chart `oci://ghcr.io/zeabur-oss/charts/zeabur` pinned to the image's digest, and `zeaburctl`
  for Linux (amd64, arm64) and macOS (arm64), all signed with Sigstore and attested
  ([docs/reference/releases.md](docs/reference/releases.md)). The chart's new `image.digest` value pins any image
  by content.

- IP limits on a service's Networking tab. "Who can open its domains" lists the allowed addresses
  in a Traefik `ipAllowList` Middleware (`<ingress>-ip-allow`) that each Traefik Ingress of the
  service lists first; an `HTTPRoute` refuses it. "Where its pods can connect" makes a
  `NetworkPolicy` for `Egress` (`<service>-egress-limit`) that keeps the project's pods and cluster
  DNS and allows the listed `ipBlock` ranges. Both are planned first, have no finalizer and work
  without Zeabur; the project's `zeabur-project` policy is not touched. It replaces the
  `adminCidr` input of the templates
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
  New changes `uploadToken` and `archiveDigest`, and
  `POST /api/v1/projects/{p}/builds/{b}/source`.

### Changed

- The installer turns on Zeabur Proxy (`proxy.enabled`) when it installs VictoriaLogs, so the Logs
  tab shows the history of replicas that are gone on a team install.
- `ZEABUR_IDENTITY_PINNED` holds a release tag, Renovate moves it only to newer release tags,
  and `scripts/release-pins.sh` accepts a tag or a tagged commit, so a release no longer stops on
  a pin that Renovate pushed to an untagged commit.
- Templates no longer carry consent inputs or default-deny NetworkPolicies, so the deploy form has no
  consent checkbox and a template is no longer "missing Traefik" outside the namespace `traefik`.
  The catalogue is at its latest main; the Traefik risk text says that a Middleware works only in
  its own namespace.
- A new domain uses an Ingress unless "Add domain" names a Gateway listener. Before, any Gateway in
  the cluster turned new domains into HTTPRoutes on the first Gateway found.
- Maintenance on an Ingress adds a second Ingress (`<name>-maintenance`) that Traefik ranks first;
  on an HTTPRoute it sets the backends to weight 0 next to the page server. An HTTPRoute redirect
  adds one rule at the top. The annotations `zeabur.com/maintenance-backend` and
  `zeabur.com/redirect-rules` are no longer read: stop every redirect and maintenance before you
  upgrade, and see the backup docs for restored objects that still have them.
- The Developer role can manage Traefik Middlewares in its projects.

### Removed

- The suggestion to move an Ingress to an HTTPRoute.

## [0.0.1] - 2026-10-18

The first public release, pre-alpha.

### Added

- Projects and services: namespaces as projects, their objects grouped into services through
  owner and selector references, env and volume references, routes, operator conventions and
  the labels of Helm, Argo CD and Flux, with every value naming the object and field it came
  from.
- Diagnoses in plain words for unhealthy services (crash loops, failed Job runs, out of memory,
  image pulls, probes, unreachable traffic, quotas, scheduling, missing ConfigMaps and Secrets),
  each with its cause, fix and evidence; gap rules for resource requests and limits, readiness
  probes and Ingresses that can move to Gateway API.
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
  image build: sign-in through Zeabur Identity as a confidential OAuth client, and admin rights on demand
  confirmed with the passkey again.
- The HTTP API under `/api/v1`, described by a generated OpenAPI 3.1 document.

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

[0.0.1]: https://github.com/zeabur-oss/zeabur/releases/tag/v0.0.1
