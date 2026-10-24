# How it works

`plan` and `apply` compute the same plan from three inputs: the configuration, a Zeabur Catalog
release (`index.json`), and three facts about the host (architecture, host name, memory). The
planning code (`zeabur-installer-plan`) does no I/O, so every decision is unit-tested and the
same inputs always give the same plan, byte for byte. `apply` then carries the plan out step by
step. Each step looks at what is already there first, so running `apply` again resumes an
interrupted install or repairs drift.

## The steps of `apply`

1. **Preflight.** The installer checks that it runs as root under systemd and that no other
   Kubernetes distribution is installed. It checks that the ports are free (unless RKE2 from an
   earlier run is the one using them): 80 and 443, 6443 and 9345 on a server, and the kubelet's
   10250. ServiceLB takes 80 and 443 on every node, workers too, and forwards them on the nodes
   where Traefik runs (its Service keeps the client's address with `externalTrafficPolicy:
   Local`); Traefik can move to any node. It checks that there is enough disk space and whether
   the disk is encrypted, and asks `timedatectl` whether the clock is synchronised. A failed
   check stops the run before anything changes.
2. **Node preparation.** Only when the plan includes Longhorn. Longhorn attaches volumes over
   iSCSI from the node itself, so the installer installs the iSCSI initiator and NFS client
   packages that are missing, writes `/etc/modules-load.d/zeabur-longhorn.conf` and loads
   `iscsi_tcp`, and enables and starts `iscsid`. If `multipathd` runs, it adds
   `/etc/multipath/conf.d/zeabur-longhorn.conf`, which keeps it off Longhorn's disks (tgt's
   `IET`/`VIRTUAL-DISK` devices) and restarts it. Each part is checked first, so a second run
   changes nothing; preflight lists what is missing as warnings.
3. **Host files.** The installer writes RKE2's `config.yaml` and its audit policy. It writes the
   kernel parameters the CIS profile requires (and loads them) and creates the `etcd` user. With
   Zeabur Identity, it adds a drop-in that points kube-apiserver at an authentication
   configuration file, and that file starts out trusting no issuer. It sets the node's addresses
   in `config.yaml.d/20-node-ip.yaml`: the first IPv4 address on the default route's interface,
   and an IPv6 address on the default IPv6 route's interface that is neither temporary nor
   deprecated (a static one first, then a stable SLAAC one). Without it RKE2 may take a temporary
   IPv6 address, which changes within a day. Every file is written atomically and only when its
   content or mode differs.
4. **RKE2.** If the pinned version is not already installed, the installer downloads it, checks
   the digests, unpacks it into `/usr/local` and starts `rke2-server`. If RKE2 is already
   running and its configuration changed, it restarts it. It then waits for `/readyz` and for
   the node to report Ready with a heartbeat newer than the start, so a Ready condition from
   before a restart does not count.

   Archives in `sources.imageArchives` are copied to `agent/images/` as
   `zeabur-installer-<name>`, only when their content differs. RKE2 imports an archive when its
   size or modification time changes, and it records the time in whole seconds, so the
   installer drops the fraction from each archive's time and an unchanged one is not imported
   again on each start.
   Archives the installer copied that are no longer listed, and archives from older installers
   under a listed name without the prefix, are removed, so an old archive never overrides a
   newer image.
5. **Namespaces.** The installer creates every namespace the components use. A namespace is
   exempt from the `restricted` Pod Security level only when its component needs more, and the
   plan says why. `longhorn-system` is one: Longhorn's manager, CSI plugin and instance managers
   mount `/dev` and the kubelet's directory from the host and run `iscsiadm` and `mount` in the
   host's namespaces. Every namespace except the ingress namespace gets a baseline `NetworkPolicy`:
   traffic may come in from the same namespace, from the installer's other namespaces and from
   the nodes, and from nowhere else. The installer's namespaces carry the label
   `installer.zeabur.com/baseline: "true"`, which the policies select, and the installer applies
   namespaces under their own field manager, so a component's own namespace labels (Zeabur
   Identity's `app.kubernetes.io/managed-by`, for one) are merged rather than replaced.
6. **Storage.** RKE2, unlike k3s, ships no storage class. Unless the cluster already has a
   default one, the installer adds rancher's local-path-provisioner as the default class. Each
   volume is a directory under `/var/lib/zeabur/storage` on this server's disk, with no copy
   elsewhere: back it up (Velero or K8up can). The directory is outside RKE2's, so
   `rke2-uninstall.sh` leaves the data; deleting a volume's claim deletes its directory.

   When the plan makes Longhorn's class the default (`profile: multi-node`), local-path is added
   as a plain class instead, and on a cluster where it was the default it stops being one; the
   plan warns about that first. A default class the installer did not make then stops the run,
   since two defaults would leave new claims to chance. `doctor` reports when the default class
   differs from the plan's.
7. **Components.** Components are installed in the catalog's order, Longhorn first so that the
   volumes of the components after it land on its class. For each one, the installer creates
   its Secrets and applies its releases. A chart release becomes a `HelmChart` object for RKE2's
   helm-controller, with `failurePolicy: abort`. The installer waits for helm-controller's
   install job, then for the catalog's readiness checks. Catalog checks that create test objects
   are left to the catalog's own CI.
8. **Zeabur Identity.** In `local` mode, the installer first makes the CA. It then applies
   Zeabur Identity's CRDs, manifests and Ingress and waits for its rollout. It replaces the
   authentication configuration with the one `zeabur-identityctl kube-config` writes, and waits
   until kube-apiserver's reload metric shows the new file was loaded. kube-apiserver reaches
   Zeabur Identity through its Service, so this step does not depend on DNS or the public
   certificate.
9. **Zeabur.** The installer registers Zeabur's OAuth client, with Zeabur's URL as its home so
   Zeabur Identity lists it among the apps, and keeps its secret in a Secret.
   It creates the plan-signing key and, in `local` mode, the CA bundle. It then installs the chart
   as a `HelmChart` and waits until Zeabur is available.
10. **First admin.** On the first install, the installer prints a one-time link for the first
   admin's passkey, valid for 24 hours.
11. **Record.** The installer writes what it installed to the ConfigMap
    `kube-system/zeabur-installer`, which `doctor` reads.

## Adding nodes

`node add` runs on a server and prints a join configuration for the new VM:

- **Server address**: `https://<InternalIP>:9345`, the RKE2 supervisor on this server, unless
  `--address` names another.
- **Token** for a worker: a Kubernetes bootstrap token, a Secret `bootstrap-token-<id>` in
  `kube-system` that kube-controller-manager deletes when it expires (one hour by default,
  `--ttl` up to 24 hours). The token string is `K10<ca-hash>::<id>.<secret>`; the hash is the
  SHA-256 of the cluster CA, so the new VM refuses a server with another CA. Zeabur creates the
  same Secret through the API. RKE2 accepts bootstrap tokens on the agent routes only, so a
  server gets the cluster's server token instead, which does not expire.
- **Location**: the server's provider and region, from its node labels.
- For a server, **`serverConfig`**: the components the cluster disables and whether
  kube-apiserver trusts Zeabur Identity. Every server must agree on these.

On the new VM, `plan` reads the cloud metadata (AWS, GCP, Azure, DigitalOcean, Hetzner, Vultr,
Linode and Oracle; 300 ms to connect, 1 s to answer, nothing if absent) and compares it with the
expected location. It times five TCP connections to port 9345 and warns when the median is over
10 ms, or calls it critical over 50 ms: between availability zones of one region a round trip
takes about 0.5 to 2 ms, and every etcd write waits for a majority of servers. It tries the TCP
ports it needs. UDP 8472 cannot be checked this way, and the plan says so.

`apply` writes the token to `/etc/rancher/rke2/zeabur-join-token` (mode 0600), the same CIS
profile, kernel parameters and kubelet settings as the first server, and a drop-in
`config.yaml.d/10-join.yaml` with the server, the token file and the node's location labels
(`installer.zeabur.com/provider`, `topology.kubernetes.io/region` and `zone`). It installs RKE2,
starts `rke2-agent` or `rke2-server`, and waits until the node is Ready, reading its own Node
with the kubelet's credentials. A server also copies Zeabur Identity's authentication
configuration from the ConfigMap `kube-system/zeabur-identity-authn`, which `apply` on the first
server publishes.

When the cluster runs Longhorn, `node add` adds `longhorn: true` to the join configuration, and
`apply` on the new VM prepares it as on the first server before installing RKE2. A node that
joined before Longhorn was added has none of it; `doctor` on a server reports the nodes
Longhorn finds not ready, from its `nodes.longhorn.io` conditions. Add `longhorn: true` to that
node's configuration and run `apply` there.

The baseline NetworkPolicies let in traffic from every node's address and from the first address
of every node's pod range, where Canal's VXLAN traffic comes from. A new worker is not in them
until `node sync` rewrites them; `doctor` on a server warns until then. A joining server runs the
same sync itself once it is Ready.

On a joined node, `doctor` checks the RKE2 unit, that the node is Ready, the distance to the
server and the disk encryption, and on a server whether the authentication configuration matches
the cluster's. It also warns when the node's InternalIP is a temporary or deprecated IPv6
address, or one the host no longer has. `uninstall` there prints how to drain and remove the
node, run `rke2-uninstall.sh`, then `node sync` on a server to take its addresses out of the
policies.

## What stays after uninstalling

`rke2-uninstall.sh` removes RKE2 and its data, but not the installer's own files: the download
cache and local CA in `/var/lib/rancher/zeabur-installer`, the configuration
`/etc/zeabur-installer/config.yaml`, and on a joined node the token file
`/etc/rancher/rke2/zeabur-join-token` if RKE2 left it. Once RKE2 is gone, `uninstall` lists what
is left, and `uninstall --purge` removes it, along with the module and multipath drop-ins written
for Longhorn. The packages installed for Longhorn stay. Volumes under `/var/lib/zeabur/storage`
and Longhorn's replicas under `/var/lib/longhorn` stay either way: delete them by hand once they
are backed up.

## Security defaults

- **RKE2's `cis` profile**: Secrets are encrypted at rest, Pod Security `restricted` applies
  outside the exempt namespaces, NetworkPolicies cover the system namespaces, and the kubelet
  protects kernel defaults.
- **Audit log**: request metadata only, so Secret values stay out of it. Every request on
  Secrets is kept, from anyone. Of the rest, only writes by people and workloads are kept: reads,
  Events, Leases, access reviews, health checks and requests from nodes and the control plane are
  dropped. The log is `/var/lib/rancher/rke2/server/logs/audit.log` on each server; RKE2 keeps
  10 files of 100 MB for at most 30 days. RKE2's own default policy logs nothing.
- **Secrets encryption**: `doctor` on the first server fails when Secrets in etcd are not encrypted.
  Rotate the key with `rke2 secrets-encrypt rotate-keys`
  ([RKE2 docs](https://docs.rke2.io/security/secrets_encryption)).
- **`kubeconfig` mode 0600** and a PID limit per pod.
- **No long-lived admin credential handed out.** People reach the cluster through Zeabur
  Identity. Their everyday tokens can read the cluster; changing it takes a fresh passkey tap.
  `/etc/rancher/rke2/rke2.yaml` stays on the server as root's credential of last resort.
- **Pinned RKE2 digests.** See the [security policy](../../SECURITY.md#what-the-installer-trusts).

## What is verified

| Part | Status |
|---|---|
| Planning: selection, dependencies, values, inputs, secrets, namespaces, RKE2 files | Unit-tested |
| Host helpers: atomic files, secret file modes, digests, checksum lists, chart packing | Unit-tested |
| RKE2 digests | Checked against the release's lists by `scripts/bump-rke2.sh` |
| `apply` on a fresh server | Verified with the `minimal` preset on Ubuntu 26.04 arm64, RKE2 v1.36.5 |
| RKE2 `cis` with the catalog's components (Pod Security exemptions, NetworkPolicy baseline) | **Not verified yet** |
| Authentication file with `anonymous.enabled: false` on RKE2 | **Not verified yet**: RKE2 runs k3s's server, which drops `--anonymous-auth=false` when the file is set, so the file follows k3s's form |
| `local` mode name resolution from pods (sslip.io names, hairpin through ServiceLB) | **Not verified yet** |
| Storage classes and Longhorn node preparation: planning, the default class, package names per distribution | Unit-tested |
| Longhorn on RKE2 `cis` with the baseline NetworkPolicies; node preparation on Debian, RHEL and SUSE hosts; the multipath drop-in | **Not verified yet** |
| Join planning: configuration, warnings, latency thresholds, location comparison, `node add` output | Unit-tested |
| Cloud metadata on real providers | **Not verified yet**: parsers are tested against documented responses |
| A worker or server joining a real cluster, `node sync` with Canal | Verified on two Ubuntu 26.04 arm64 VMs, RKE2 v1.36.5 |
| The CA hash from `kube-root-ca.crt` equals the server token's | Checked by `node add` each time; equal on the same VMs |
