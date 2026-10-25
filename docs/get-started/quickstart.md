# Zeabur Installer

**One Linux server, one configuration file, one command: a hardened Kubernetes cluster with Zeabur on it.**

`zeabur-installer` installs, on a single server:

- **[RKE2](https://docs.rke2.io)** with its CIS profile. Secrets are encrypted at rest, Pod
  Security defaults to `restricted` for unlabelled namespaces, and an audit log is kept.
  Project namespaces Zeabur creates use their own `baseline` labels.
- The **[Zeabur Catalog](https://github.com/zeabur-oss/catalog)** components you choose: ingress,
  certificates, metrics, logs and, with `preset: secure`, runtime detection, vulnerability
  scanning and intrusion prevention.
- **[Zeabur Identity](https://github.com/zeabur-oss/identity)**. You sign in to the cluster with a
  passkey; no `admin.conf` gets copied around.
- **[Zeabur](../../README.md)**, the web console for the cluster.

> **Status: v0.0.1 alpha.** Nothing is published yet, so an install names development
> sources (see [examples/development.yaml](../reference/installer-examples/development.yaml)). It has not been run end
> to end on a fresh server yet; see [What is verified](../reference/installer.md#what-is-verified).

## Quick start

On a fresh server running systemd, as root:

```sh
mkdir -p /etc/zeabur-installer
cp docs/reference/installer-examples/domain.yaml /etc/zeabur-installer/config.yaml   # then edit the host names
zeabur-installer plan     # what will be installed, and warnings; changes nothing
zeabur-installer apply    # install; safe to run again
```

At the end, `apply` prints Zeabur's address and a one-time link. Open the link to register the
first admin's passkey.

The server needs 4 GB of memory or more, 30 GB of free disk, and ports 80, 443, 6443, 9345 and
10250 free. Before changing anything, `apply` checks these and the clock, and warns when it finds
files of k3s, kubeadm, k0s or MicroK8s.

## Commands

| Command | What it does |
|---|---|
| `plan [--json]` | Shows the components, charts, namespaces, memory estimate and warnings. Reads the host, changes nothing. |
| `apply` | Installs, or brings the server in line with the configuration. Each step checks what is already there, so an interrupted run resumes. |
| `doctor` | Checks RKE2, each component's charts and readiness, the default storage class, Zeabur Identity and its internal certificate, Zeabur and disk encryption. Exits 0, 1 (warning) or 2 (critical), so a monitoring check can run it. |
| `uninstall zeabur\|identity` | Removes Zeabur, or Zeabur and Zeabur Identity. To remove RKE2 and everything else, run `/usr/local/bin/rke2-uninstall.sh`. On a node that joined a cluster, `uninstall` prints how to take the node out. Once RKE2 is gone, `uninstall` lists the installer's files that are left and `uninstall --purge` removes them. |
| `node add [--role worker\|server] [--ttl 1h]` | On a server: prints the configuration and commands that join another VM to the cluster. |
| `node sync` | On a server: lets the current nodes' traffic through the installer's network policies. Run it after a worker joins and after a node is removed. |

Every command reads `/etc/zeabur-installer/config.yaml` unless `--config` names another file.

## Configuration

One file holds every choice, so the same install can be repeated, reviewed and kept in version
control. The smallest one:

```yaml
access:
  mode: domain            # Let's Encrypt; `local` uses a CA made on the server
  identity: login.example.com
  zeabur: zeabur.example.com
```

[configuration.md](../reference/installer-configuration.md) describes every field: presets, adding and
removing components, component inputs, secret files, the RKE2 version and host settings.

## Multiple servers

Install the first server as above, then add VMs to the same cluster. **Every node must be at the
same hosting provider and in the same region.** Different availability zones in one region are
fine. Between providers or regions, the latency slows etcd, the Kubernetes API and storage, and
can make the cluster unstable.

On the first server:

```sh
zeabur-installer node add             # a worker; the token it prints expires in an hour
zeabur-installer node add --role server
```

It prints a configuration to paste on the new VM, then `zeabur-installer plan` and
`zeabur-installer apply` to run there. `plan` on the new VM measures the round trip to the server and checks the ports it can.
`apply` installs RKE2 only, as an agent for a worker or a server for the control plane, and
waits until the node is Ready.

After a worker joins, run `zeabur-installer node sync` on a server. Until then, the network
policies refuse the new node's traffic. Then revoke the token with the command `node add`
printed. The node keeps working after its token is gone.

Things to know:

- Open these between the nodes: TCP 9345 and 6443 to the servers, TCP 10250 and 9099 and UDP
  8472 between all nodes, TCP 2379 to 2381 between servers.
- The control plane survives losing a server only with three or more servers. Two servers are
  less available than one.
- Volumes on the local-path storage class stay on the node that created them. With `profile:
  multi-node`, the catalog adds Longhorn, whose volumes keep a copy on up to three nodes and can
  be expanded; its class `longhorn` becomes the default and local-path stays available. `apply`
  installs an iSCSI initiator and an NFS client on every node for it, and `node add` tells the
  new node to do the same.

[examples/join.yaml](../reference/installer-examples/join.yaml) is a join configuration with comments.

## Learn more

- [How it works](../reference/installer.md): the steps of `apply`, the security defaults, and what has
  and has not been tested.
- [Contributing](../../CONTRIBUTING.md#installer) and the [security policy](../../SECURITY.md).

## License

[Apache-2.0](../../LICENSE).
