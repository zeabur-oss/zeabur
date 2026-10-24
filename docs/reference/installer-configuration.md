# Configuration

`zeabur-installer` reads one YAML file, `/etc/zeabur-installer/config.yaml` by default
(`--config` names another). Unknown fields are errors, so a typo is caught before anything
changes. Field names are camelCase.

## `access` (required)

How people reach Zeabur and Zeabur Identity.

| Field | Meaning |
|---|---|
| `mode` | `domain`: public DNS names; certificates come from Let's Encrypt through cert-manager (HTTP-01), so port 80 must be reachable from the internet. `local`: a CA made on the server signs them, for test machines; browsers trust it once `/var/lib/rancher/zeabur-installer/local-ca.crt` is imported. |
| `identity` | Zeabur Identity's host name, without `https://`. Its issuer is `https://<identity>`. |
| `zeabur` | Zeabur's host name. It must differ from `identity`: an OpenID Connect issuer is a whole origin. |
| `invitations` | Default `true`: Zeabur Identity accepts invitation links, which Zeabur's Members page and `zeabur-identityctl invite` make. Set `false` when people and their groups come only from version control; `zeabur-identityctl setup-link` still works. |

In both modes the two names must resolve to the server, for browsers and for the server itself.
Pods resolve names through DNS, not through `/etc/hosts`. For a test machine, names such as
`login.<address>.sslip.io` resolve to `<address>` without any DNS setup.

## Components

| Field | Meaning |
|---|---|
| `preset` | `minimal`: ingress, certificates and metrics-server. `standard` (default): everything the catalog recommends for the profile. `secure`: `standard` plus runtime detection, vulnerability scanning and intrusion prevention. |
| `enable` | Capabilities (`runtime-security`) or component ids (`volume-backup/velero`) added to the preset. A component's dependencies are added with it. |
| `disable` | Capabilities or component ids taken out. Disabling one that another chosen component needs is an error that names both. |
| `profile` | `single-node` (default) or `multi-node`; selects the catalog's values for the profile. |

### Storage

The installer always adds the local-path storage class: volumes on the node's own disk. The
catalog's `replicated-storage` capability, Longhorn, adds the class `longhorn`, whose volumes
are replicated across nodes and can be expanded.

| Profile | Longhorn | Default class |
|---|---|---|
| `single-node` | only with `enable: [replicated-storage]` | `local-path` |
| `multi-node` | in the `standard` preset | `longhorn` |

`plan` shows the classes and which one is the default. Switching an existing cluster to
`multi-node` makes `longhorn` the default and local-path a plain class; `plan` warns before
`apply` does it, and volumes already on local-path stay there. If the cluster has a default class
the installer did not make, `apply` keeps it and leaves local-path out, or, when the plan makes
`longhorn` the default, stops until you take the annotation off the other class. Databases that
replicate themselves, such as CloudNativePG clusters, are better on `local-path`: name it in
their `storageClass`.

With Longhorn in the plan, `apply` prepares the node before installing RKE2. It installs the
iSCSI initiator and NFS client packages that are missing (`open-iscsi` and `nfs-common` with
apt, `iscsi-initiator-utils` and `nfs-utils` with dnf, `open-iscsi` and `nfs-client` with
zypper), loads `iscsi_tcp` now and at boot, and enables `iscsid`. On another distribution it
stops and names the packages to install by hand.

`zeabur-installer plan` lists what was chosen and why (`preset`, `enabled` or `dependency`), and
what was left out and why: a component that runs on k3s only, for example.

### Inputs

Some components need values, such as Velero's bucket. `plan` stops at a missing one and
prints the line to add:

```yaml
inputs:
  volume-backup/velero:
    bucket: backups
    s3-url: https://s3.example.com
```

Secret inputs never go in this file. Name a file holding the value instead; it must have mode
0600:

```yaml
secretFiles:
  volume-backup/velero:
    credentials: /root/secrets/velero-credentials
```

A secret the catalog marks `generate: random` that has no file gets 64 random bytes (as 128
hexadecimal digits) on the first `apply`, and keeps them afterwards.

## `zeabur`

`true` (default) installs Zeabur Identity and Zeabur. `false` installs RKE2 and the components only.

## `rke2Version`

Leave it out to install the release this installer pins, whose downloads are checked against
digests compiled into the installer. Naming another release, such as `v1.37.1+rke2r1`, is
allowed but unsupported. The plan warns about it, and its downloads are checked only against the
release's own `sha256sum-<arch>.txt`. The catalog may also not have been tested on that
Kubernetes version.

## `host`

| Field | Meaning |
|---|---|
| `tlsSan` | Extra names and addresses for the Kubernetes API server's certificate, to reach it from elsewhere. |
| `requireEncryptedDisk` | Stop when `/var/lib/rancher` is not on a dm-crypt device. Without it, `apply` and `doctor` only warn. |

## `sources`

Development builds, used while a product has no published release. Each one replaces what the
installer would otherwise download.

| Field | Meaning |
|---|---|
| `catalog` | A catalog `index.json` on disk. |
| `imageArchives` | `docker save` archives, copied to RKE2's image directory and imported when it starts. An archive taken out of the list is removed from there. |
| `identityImage` | Zeabur Identity's image reference. |
| `identityctl` | A `zeabur-identityctl` binary. |
| `zeaburImage` | Zeabur's image reference. |
| `zeaburChart` | Zeabur's chart: a directory or a packaged `.tgz`. |

## `join`

A file with a top-level `join` makes this machine join an existing cluster instead of creating
one. `zeabur-installer node add` on a server prints it. Only `rke2Version`, `host` and
`sources.imageArchives` may sit beside it; a joining node installs RKE2 and nothing else.

| Field | Meaning |
|---|---|
| `server` | `https://<address>:9345`, a server's RKE2 supervisor. |
| `role` | `worker` (default) or `server`. |
| `token` | The token `node add` printed. The file must then be mode 0600. |
| `tokenFile` | Or a file holding it, mode 0600. |
| `expect.provider`, `expect.region` | The server's location. `plan` compares it with this VM's. |
| `serverConfig` | Servers only, required: `disable` (RKE2 components the cluster disables) and `identity` (whether kube-apiserver trusts Zeabur Identity). |
| `longhorn` | `true` when the cluster runs Longhorn: `apply` prepares this node for it as on the first server. `node add` sets it. |

`host.tlsSan` applies to servers only.

## Examples

- [installer-examples/domain.yaml](installer-examples/domain.yaml): a server with public DNS names.
- [installer-examples/local.yaml](installer-examples/local.yaml): a test machine.
- [installer-examples/development.yaml](installer-examples/development.yaml): unpublished builds.
- [installer-examples/join.yaml](installer-examples/join.yaml): another node for an existing cluster.
