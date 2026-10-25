# Security policy

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub's private vulnerability reporting:
"Report a vulnerability" on the
[Security tab of zeabur-oss/zeabur](https://github.com/zeabur-oss/zeabur/security/advisories/new).
Do not open public issues, discussions or pull requests for security problems.

Useful in a report: the version or commit, how Zeabur runs (`zeaburctl ui`, in a cluster with
Zeabur Identity, or installed by `zeabur-installer`), the Kubernetes distribution and version, and
the steps that show the problem. A
vulnerability in Zeabur Identity itself belongs to
[zeabur-oss/identity](https://github.com/zeabur-oss/identity/security/advisories/new).

## Supported versions

| Version | Supported |
| --- | --- |
| latest 0.0.x | yes |
| older releases | no |

Zeabur is pre-alpha, for evaluation on clusters you can afford to break. Only the latest 0.0.x
release receives security fixes, in a new release; fixes are not backported, and there is no
guaranteed response or patch timeline. Releases before 1.0.0 may change the API and the chart's
values without a migration path.

## What the installer trusts

`zeabur-installer` runs as root and changes the host: it writes files under `/etc/rancher`,
`/etc/sysctl.d` and `/var/lib/rancher`, creates the `etcd` user and installs systemd units. Run it
on a machine meant for this, not on a shared server.

- **RKE2** downloads are checked against SHA-256 digests compiled into the installer
  (`crates/zeabur-installer-plan/src/pins.rs`). RKE2 releases carry no signature, so these
  digests are where trust in RKE2 starts. Choosing another version with `rke2Version` falls back
  to the release's own checksum list, served from the same place as the files; the plan marks
  that version unsupported.
- **Catalog charts and manifests** are downloaded by the installer and checked against the
  SHA-256 digests the Zeabur Catalog release names; a mismatch stops `apply`. The checked chart
  goes into the `HelmChart` object, so helm-controller fetches nothing. The catalog also pins
  every component image by digest. A catalog release without digests falls back to
  helm-controller fetching the chart by version, unchecked.
- **Secrets** never appear in the configuration file or the plan. A secret input names a file
  with mode 0600, which the installer refuses when the group or others can read it, or is
  generated once and kept in its Kubernetes Secret.
