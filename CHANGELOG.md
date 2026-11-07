# Changelog

All notable changes to Zeabur. Versions follow [Semantic Versioning](https://semver.org/); before 1.0.0, a minor or patch release may break the API. Each section comes from the change files in `.changeset/` (CONTRIBUTING.md, "Releases").

## 0.2.0 (2026-11-07)

Read [what's new in Zeabur 0.2.0](https://oss.zeabur.com/blog/zeabur-0-2-0) for the highlights.

### Breaking Changes

#### Zeabur and zeabur-proxy no longer take flags for session lengths and proxy limits

`zeabur --session-idle-minutes` and `--session-max-hours`, and `zeabur-proxy --timeout`, `--max-bytes`, `--review-cache`, `--max-inflight` and `--max-inflight-per-person`, with their `ZEABUR_SESSION_*` and `ZEABUR_PROXY_*` environment variables, are removed. The values stay the same: a session ends after one hour idle or twelve hours in all, and the proxy keeps its 30-second timeout, 16 MiB request limit and 10-second review cache. The chart and the installer never set them. If you added one to a Deployment by hand, delete it from its `args` and `env`.

#### The installer no longer renames old component input names

`apply` and `gui` no longer rewrite old input names in the configuration file, and no command reads an old name as the new one. If your `inputs` or `secretFiles` still use an old name, rename the key by hand before you upgrade:

- `builds/zeabur-builder`: `cluster-ip` → `clusterIp`
- `intrusion-prevention/crowdsec`: `bouncer-key` → `bouncerKey`, `lapi-secret` → `lapiSecret`, `registration-token` → `registrationToken`
- `remote-access/tailscale-operator`: `oauth-client-id` → `oauthClientId`, `oauth-client-secret` → `oauthClientSecret`
- `volume-backup/velero`: `s3-url` → `s3Url`, `repository-password` → `repositoryPassword`, `backup-key` → `backupKey`, `file-backup` → `fileBackup`

#### The installer no longer takes over a standalone zot when it installs Zeabur Builder

`container-registry/zot` in `enable` or `disable` is now an unknown component, and `apply` no longer turns a zot that zeabur-installer 0.1.0 or 0.1.1 installed into Zeabur Builder's. If the server has that zot, remove it before you run `apply` with this version. `kubectl -n kube-system get helmchart zot -o jsonpath='{.metadata.annotations.installer\.zeabur\.com/component}'` prints `container-registry/zot` on such a server, and `builds/zeabur-builder` or nothing on any other. Remove `container-registry/zot` from `enable` and `disable`, then run `kubectl -n kube-system delete helmchart zot`, wait until `kubectl -n zot get pods` lists none, and run `kubectl delete namespace zot`. The namespace holds the images zot stored; builds push them again. `apply` then installs Zeabur Builder with its own zot.

#### The installer configuration has no `preset`

`plan` and `apply` refuse a `/etc/zeabur-installer/config.yaml` that still has `preset`; remove the line. A team install takes the catalog's recommended components for its profile, plus Zeabur Builder, Keel and CloudNativePG; `ingress`, `certificates` and `metrics/metrics-server` are always installed. `enable` and `disable` change that list, by capability or component ID.

#### `container-registry/zot` is now `builds/zeabur-builder`

Zeabur Builder and the zot registry it pushes to are one component. Replace `container-registry/zot` with `builds/zeabur-builder` in `enable` and `disable`.

#### A join configuration names the registry `registry`, not `zot`

`node add` prints `registry: {host, clusterIp, ca}`. Run `node add` again to join a node with this installer.

#### API: `installationsProblem` is a `Problem`, and every operation has a verb-first ID

`GET /api/v1/cluster/github-app` returns `installationsProblem` as a whole `Problem` (summary and fix) instead of a string. Every operation in `openapi.yaml` has an explicit camelCase `operationId` (`getProject`, `listProjects`, `getWhoAmI`), so a client generated from the description changes its method names.

### Features

#### Set a project's build memory and CPU

A project's Settings has a Builds section for the memory and CPU one build may use, up to what the largest node has. An empty field uses Zeabur Builder's default. A build that ran out of memory now links to it.

#### Zeabur reads the new Zeabur Catalog format

`zeabur-installer` marks a component untested where the catalog did not test it on the server's distribution and platform, and leaves out a component whose Kubernetes versions do not include the cluster's. Component inputs use the catalog's new names, such as `s3Url`, `fileBackup` and `clusterIp`, in `inputs` of the configuration file. A component or template that needs a release type, Pod Security level or generated value that this build does not know is not offered.

#### Suggest the port an image declares

When Zeabur Builder built a service's image and the image declares ports with `EXPOSE`, the Networking tab of a service with no private address says which ports the image declares and offers **Add a private address on port …** for each TCP port its containers and Services do not use yet. The button plans the same Service as the form; nothing changes until the plan is applied. Builder reports the ports from the version that records them in a Build's `status.exposedPorts`.

#### `zeabur-installer` marks components the catalog did not test on the server's distribution

`plan`, `doctor` and the installer's pages mark a component that was not tested on K3s, RKE2 or KubeSolo as untested, and `plan` installs it with a warning. `doctor` and `apply` run the health checks the catalog ships with each component, and `doctor` prints the catalog's advice when one fails.

#### A release tag may carry a SemVer pre-release part (`v0.1.2-beta.1`)

Such a release is a GitHub pre-release, never the latest, and has its own section here.

#### Add a private address from the Networking tab

A Deployment, StatefulSet or DaemonSet with no Service gets an **Add a private address** form on its Networking tab. It plans a ClusterIP Service named like the service, selecting its pods by `matchLabels` and carrying every port its containers declare, with the port filled in from the readiness probe or a declared port. The `kubectl` command is under **Use kubectl instead**.

#### Zeabur and `zeaburctl` tell people when a newer version is out

The cluster page shows Zeabur's version. Once a day, the browser checks GitHub's releases. When a newer one is out, the page says so and shows how to upgrade for the way Zeabur was installed. It also says when the running version has known security problems. `zeaburctl` prints one line when a newer version is out, at most once a day. Turn the update check off with the installer's review page checkbox or `updateCheck: false`, the chart's `updateCheck.enabled=false`, `DO_NOT_TRACK=1` or `NO_UPDATE_NOTIFIER`. See [Update check](docs/reference/telemetry.mdx#update-check). `GET /api/v1/cluster/zeabur` returns Zeabur's version, whether the update check is on, and how Zeabur was installed.

#### An experience program counts installations and daily active people

Zeabur sends one event the first time each person uses its web interface in a UTC day, and the installer sends one when `apply` starts, finishes or fails. Each event carries a random installation ID, the versions, the profile and the distribution; never domains, host names, IP addresses, names or the contents of the cluster. The installer joins by default: uncheck the box on its review page, pass `--no-telemetry`, set `telemetry: false` or `DO_NOT_TRACK=1`. With Helm, Zeabur stays out until `telemetry.enabled=true` with a `telemetry.installationId`. `ZEABUR_TELEMETRY=debug` prints what would be sent. See [Experience program](docs/reference/telemetry.mdx).

#### On an installed server, `plan` says what `apply` will change

`plan` reads the record that `apply` leaves in `kube-system/zeabur-installer` and starts with `Changes since the last apply`: the K3s, RKE2 or KubeSolo release, the host files that change and what restarts, components added, replaced or no longer updated, and each chart's version, or "values changed" when only its values differ. `plan --json` has the same under `changes`, and the installer page's review step shows it as "Changes to this server". A component that became a default after the last install is listed but installed only when `enable` names it. `plan` and `apply` stop when a newer installer wrote the record, and name the version to use.

#### Personal installs run KubeSolo v1.2.3 and can add databases

`enable: [databases]` adds CloudNativePG on a personal install; it is not a default.

#### The installer fixes the node's name at install

A hostname change, such as Parallels' hostname sync or a cloud renaming a server on reboot, made K3s register a second node with the same address. The installer now writes the name once (`host.nodeName`, else the hostname when it is valid and free, else `node-` and 6 characters derived from `/etc/machine-id`) and refuses to rename an installed node. The installer page has a "Node name" field, read-only once installed. A joining node avoids names already in the cluster.

#### `sources.proxyImage` replaces Zeabur Proxy's image

It is written to the chart as `proxy.image` when VictoriaLogs or VictoriaMetrics is installed. Unset, the chart's own image is used.

#### A `.zip` upload keeps file modes and symbolic links

Zeabur reads the execute bit and symbolic links of entries made on Unix, as a `.tar.gz` upload already kept them. A link that points outside the folder, or an entry under a link, is refused.

### Fixes

#### A failed build says why in a sentence

The build card leads with what went wrong and what to do next, with the builder's output behind Details and its line breaks kept. The services table keeps CPU and Memory on one line.

#### Git and folder services start with port 8080, and a public domain when the cluster has a domain suffix

The add drawers name the service after the repository or folder (or `web` before one is chosen) and, when the cluster has a domain suffix, fill in the public domain from the start. With Railpack the port defaults to 8080, which Zeabur passes as `PORT`, so a website gets a URL without extra steps. With Dockerfile the service starts with no network until a port is filled in. A folder with a Dockerfile switches the build to Dockerfile.

#### Webhooks reach pods on other K3s nodes

Calico on K3s now sends all traffic between nodes through VXLAN. Before, a node reached a pod on another node in the same subnet from its own address, which the baseline network policy blocks, so webhook calls from the API server timed out and `apply` failed. A cluster installed earlier keeps the old setting; Add nodes → Common failures has the command that changes it.

#### Components ask for the memory they were measured to use

On a single node, each default component's memory request follows what it used on a 4 GB K3s server. VictoriaLogs asks for less memory, leaves out Kubernetes' own namespaces and keeps kube-apiserver's audit log.

#### Upgrading an installation that has the VictoriaMetrics operator no longer stops on a field conflict over its CRDs

`zeabur-installer apply` now applies only the `crds/` folders Helm would install, and leaves CRDs a chart makes from its templates to Helm.

#### Doctor says how to delete a Longhorn node that Kubernetes removed

After you remove a worker, Longhorn keeps a node entry for it. `zeabur-installer doctor` used to say the node joined without Longhorn. It now says Longhorn still lists the node and gives the command to delete it. Add and maintain nodes → Remove a node has the full steps.

#### Doctor reports a Deployment scaled to 0

`zeabur-installer doctor` counted a Deployment with no Pods wanted as available, so a component whose Deployments were scaled to 0 passed. It now reports `deployment/<name> is scaled to 0`.

#### Doctor reports a broken workload once

`zeabur-installer doctor` lists an unavailable Deployment or DaemonSet once, under its namespace. Before, it was repeated under every component that installs into that namespace, such as the four in `observability`. A component's passing line now says `charts installed` (and `health checks pass` when the catalog has checks), since its workloads are reported under their namespace.

#### Dragonfly starts on any server and shows its logs

Each size sets a CPU limit, the thread count and `maxmemory` together, so a store no longer exits on servers with more CPUs than its memory allows threads for. Small now needs 384 MiB. Dragonfly's pods are found by the labels its operator sets, so Logs, the terminal and crash diagnosis see them.

#### Signing in from far away takes one redirect less

A page opened without a session goes straight to Zeabur Identity instead of through `/auth/login` first. The API gzips JSON answers from a kilobyte up.

#### Doctor and apply skip pods that have ended

After a full disk the kubelet leaves Completed and Evicted pods behind. `zeabur-installer doctor` and `apply` no longer wait for them to become Ready, so a component whose live pods are Running passes once the disk is free again.

#### A health check waits for the objects it checks to appear

`apply` and `doctor` no longer fail at once with "no matching resources found" when a check covers objects a controller creates after install, such as Longhorn's nodes or Traefik's pods. They wait for at least one to exist, then for its condition, both within the check's timeout.

#### A failed chart install or upgrade stops `apply` and says how to see why

`apply` used to wait 15 minutes and then only report that the chart wasn't ready. It now stops as soon as Helm marks the release failed and tells you to run `helm history` on a machine that has Helm installed and can reach the cluster. The installer doesn't install Helm. Fix the cause and run `apply`, which updates the chart and stops there again. Then run `helm rollback` (or `helm uninstall` if the first install failed) and run `apply` once more. Rolling back first doesn't help: helm-controller upgrades again right away with the chart's old values.

#### Zeabur Identity v0.1.2

A release installs it in place of v0.1.1. It stores the sign-in pages' styles and scripts brotli-compressed, asks for 24Mi of memory instead of 32Mi, and a passkey sign-in no longer ends on "This sign-in expired" when autofill and the **Sign in with a passkey** button both answered ([its release notes](https://github.com/zeabur-oss/identity/releases/tag/v0.1.2)).

#### Containers can signal their own processes on Ubuntu 26.04

On Ubuntu 26.04 with kernel 7.0.0-41 or later, containers on K3s, RKE2 and KubeSolo could not send signals to their own processes, so PostgreSQL and other images that rely on them hung at start. Where `kernel.apparmor_restrict_unprivileged_unconfined` is 1, the installer now writes and loads `/etc/apparmor.d/cri-containerd.apparmor.d` before the cluster starts: containerd's default profile with the rules that let such containers signal and trace each other. On a cluster that is already running, the next `apply` restarts K3s, RKE2 or KubeSolo once. `uninstall --purge` removes the file.

#### A one-node install estimates each component's memory from the catalog's measurement on K3s or RKE2

The review page used the figures measured on kind; the catalog also measures each component on K3s and RKE2, which can differ by tens of MiB.

#### The installer's components page no longer says a component left out for memory is about building apps

Any component a personal install leaves out because the server has too little memory now reads "Needs N MiB of memory, and this server has less".

#### `zeabur-installer apply` moves the redirect URI of Zeabur's OAuth client

It follows a changed `access.zeabur`, as it already did the home URL. It kept the first host's, so signing in to Zeabur at the new host failed with "This app's return address isn't registered".

#### The installer asks about the experience program and the update check on an Options page

They move out of the review page into an Options step between Components and Review. Review lists whether each is on and links back to change it.

#### Plan on a joined node doesn't ask for its own ports to be free

On a node whose `k3s-agent`, `rke2-agent` or `rke2-server` already runs, `plan` says ports 80, 443 and 10250 belong to the running unit, instead of listing them as ports that must be free. `apply` already skipped that check there.

#### A K3s or RKE2 server that runs out of memory stops a pod, not the cluster

`zeabur-installer` keeps memory for the OS and the distribution from pods with the kubelet's `system-reserved` and `kube-reserved`, and evicts pods when less than 400 MiB stays free for 30 seconds or less than 200 MiB at once. A build that runs out of memory fails alone instead of taking down K3s and every pod with it. On RKE2, etcd and kube-controller-manager ask for less memory, so the components still fit a 4 GiB server. The Options step shows how much memory is kept for the system. Running `zeabur-installer apply` again writes the new settings to an existing server and restarts K3s or RKE2; on a worker, run `zeabur-installer join` again.

#### Longhorn's node check passes after apply

Longhorn reports a node whose `dm_crypt` module is not loaded, so `apply` now loads it with `iscsi_tcp` and at every boot.

#### Doctor says how to refresh a Longhorn node's checks

Longhorn's manager reads a node's packages and kernel modules when it starts, so a node's conditions stay `False` after `zeabur-installer apply` fixed the node. `doctor` now tells the reader to restart that node's longhorn-manager Pod after `apply`, with the command.

#### MySQL backups can be set up again

Setting up backups for a MySQL database, or restoring one from a backup, no longer stops with "The cluster does not serve ServiceAccount (v1)".

#### The image is pushed by digest with no `candidate-*` tags left in GHCR


#### The installer chooses components from the configuration file alone

`zeabur-installer plan` no longer reads the install record to choose components: a default the record lacks is installed. The record still shows what `apply` adds and removes.

#### Keep node columns aligned when a node has no disk data

In the node list, a node without disk data leaves the Disk column empty. Before, its CPU and memory moved over by one column and no longer lined up with the other nodes.

#### The baseline network policies use each node's VXLAN address from the first apply

On a new K3s install, `apply` could write the baseline network policies before Calico wrote the node's VXLAN address. The policies then listed the first address of the node's replica network range instead, and `zeabur-installer doctor` reported that they did not let the node's VXLAN address in yet. `apply` and `node sync` now wait up to two minutes for the VXLAN address of every Ready node, and on K3s the policies use only the address Calico gives the node. Both commands leave out a node that isn't Ready, and once that node is Ready, `doctor` asks for another `node sync`.

#### A personal install records its KubeSolo version under `version`

A personal install now keeps the KubeSolo version in the `version` field of the install record, as K3s and RKE2 installs do, and the installer no longer reads the old `kubesolo` field. If you installed a personal server with a beta release, `apply` does not see the version it installed until you copy the field once. On the server, run:

```sh
sudo /var/lib/zeabur-installer/bin/kubectl --kubeconfig /var/lib/kubesolo/pki/admin/admin.kubeconfig \
  -n kube-system patch configmap zeabur-installer --type merge \
  -p "{\"data\":{\"version\":\"$(sudo /var/lib/zeabur-installer/bin/kubectl --kubeconfig /var/lib/kubesolo/pki/admin/admin.kubeconfig -n kube-system get configmap zeabur-installer -o jsonpath='{.data.kubesolo}')\"}}"
```

Running `zeabur-installer apply` again also writes the new field, so you can skip the command if you are about to apply.

#### Plan lists Zeabur's changes too

Under Changes, `plan` now lists the Zeabur and Zeabur Identity releases an upgrade installs, such as `Zeabur  v0.1.1 -> v0.2.0`, and the old and new URL when the Zeabur Identity or Zeabur domain changes. After an upgrade from an installer that didn't record the releases, it shows only the new one.

#### Plan checks the sources paths

`plan` and `apply` stop before changing the host when a `sources` path is missing or is a directory where a file belongs, naming the key, such as `sources.identityctl`.

#### A review that is still loading no longer shows Cancel where the main button appears

While a review loads, its dialog shows Cancel and its main button, disabled until the review is ready. Before, Cancel sat alone where the main button appears, so clicking it quickly closed the review without applying the changes.

#### Zeabur Proxy's image is published with each release

`ghcr.io/zeabur-oss/proxy` is signed and attested with each release, and the chart pins it by digest. 0.1.1 did not publish it, so an installation with VictoriaMetrics or VictoriaLogs left `zeabur-proxy` unable to pull its image.

#### Release notes are the changelog section and a link to the documentation

The digests of the image, each platform's image and the chart are in the signed `digests.txt` asset, as [Releases](docs/reference/releases.mdx) shows.

#### A team install needs 2700 MiB on K3s and 3500 MiB on RKE2

The minimums count what the scheduler counts: the memory requests of the distribution's own pods, of Zeabur, of Zeabur Identity and of the components, on top of the memory kept for the system and the eviction threshold. Below them pods stayed Pending after the install. The requests come from the catalog's measurements, Zeabur's chart and Zeabur Identity's release, not from numbers in the installer.

Components that need room for work, Zeabur Builder, VictoriaMetrics and VictoriaLogs, are left out on a server where they and their work would not fit, as a personal install already does. Zeabur Builder needs 3900 MiB on K3s and 4800 MiB on RKE2. So a 4 GiB server installs K3s or RKE2 without the builder.

An install that already runs is upgraded on a server below the new minimum, with a warning that suggests turning off the crash kernel reserve, and keeps the components it has.

#### Daily users are no longer counted again after Zeabur restarts

`zeabur_active` carries `person`, a hash of the user name and the day under a key derived from the plan key, so someone who uses Zeabur before and after a restart is counted once that day. The user name can't be worked out from it, and the hash changes every day.

#### Template summaries come from Zeabur Catalog

The template list shows each template's summary from the catalogue in English, zh-TW and zh-CN. A template without one shows its description. `GET /api/v1/cluster/templates` returns them as `tagline` and `taglineLocales`.

#### Zeabur asks for 24Mi of memory instead of 32Mi

Zeabur used about 6Mi in a cluster with 35 pods, so the chart's request drops to 24Mi. The limit stays at 128Mi.

#### Zeabur checks for a newer version each time a page opens, and zeaburctl no longer misses it

The cluster page used to ask GitHub from each browser once a day, and a failed request stopped the check for the rest of that day. Now the Zeabur server asks GitHub at most once an hour and keeps the answer, and each page reads it when it opens and hourly while it stays open. People behind one office address no longer share GitHub's limit of 60 requests an hour, and browsers no longer contact GitHub.

`zeaburctl` dropped the answer when a command ended before GitHub replied, and then waited a day to ask again, so quick commands never showed a notice. It now asks on every command and waits at most a second after the command for the answer. When GitHub's limit for the address is used up, it says to set `GH_TOKEN` or `GITHUB_TOKEN`, and sends that token when set.

**Settings** › **About** shows Zeabur's version, and **How to upgrade** there and in the cluster page's notice opens the steps for how Zeabur was installed, with the commands to copy. The new [Upgrade Zeabur](https://oss.zeabur.com/docs/zeabur/upgrade/steps/) page covers zeabur-installer, Helm, Argo CD, Flux and `zeaburctl ui`.

#### A worker that joined before Longhorn is prepared for it

`plan` and `apply` on a joined node now find Longhorn's manager running there and install the iSCSI initiator and NFS client, start iscsid and load iscsi_tcp, as they do on a node that joined with `longhorn: true`. `doctor` on the server names every node whose Longhorn checks of packages, kernel modules or the NFS client fail, and `doctor` on the node checks them too.

#### Upgrading with `helm upgrade --reuse-values` kept the old image

A release chart sets `image.digest` in its values, and `--reuse-values` keeps the last release's values, so the Deployment kept running the previous version. The install guide now upgrades with `--reset-then-reuse-values`; use it when you upgrade from 0.1.1.

#### A chart's CRDs are updated when it upgrades

Before applying a catalog chart, `apply` applies the `crds/` of the chart and its subcharts with server-side apply, so an upgrade such as K8up's no longer leaves its CRDs behind. CRDs are never deleted.

#### The installer closes the CIS gaps kube-bench found that it can fix

`local-path-storage`'s `default` ServiceAccount no longer mounts a token (5.1.5). On RKE2, the server certificates are no longer world-readable (1.1.20), and the kubelet gets the same cipher list and `streaming-connection-idle-timeout=5m` as on K3s (4.2.12).

#### `zeaburctl doctor` no longer warns about Zeabur Identity where nothing needs it

On a personal installation, or a cluster opened only with `zeaburctl ui`, the Zeabur Identity checks are skipped. They still run when a project you can see runs the Zeabur dashboard, or when doctor cannot tell.

#### A container is "crash looping" only by the kubelet's own rule

A ready container that had restarted once showed as crash looping. The status light and the diagnosis now share the kubelet's back-off rule: the container is in `CrashLoopBackOff`, or it has restarted at least twice, its last run lasted under 10 minutes and its current run has not lasted 10 minutes yet. The exit code only picks the wording.

#### A GitHub App Secret with an `api-url` that is not https is refused where it is read

Zeabur answers with "`api-url` must be an https URL" and how to fix it, localized, instead of failing later in Zeabur Builder.

#### The interface answers sooner on a slow connection

A page that takes longer than 100 ms to open shows a progress bar over the old page, and the plan dialog shows its title and "Checking changes…" while its code loads. The project page no longer waits for incidents and suggestions. A read that succeeds takes one request to the Kubernetes API instead of two.

#### Zeabur is smaller and starts with less memory

Zeabur registers its routes without building its OpenAPI document (3.4 MB off the binary, and the 433 KB document no longer kept in memory). It embeds its web files once, brotli-compressed, and decodes a copy only for a browser that does not accept brotli (4.7 MB fewer embedded bytes). Idle HTTP connections close 30 seconds after their last use.

#### Smaller fixes in the interface

In Safari, opening a service's quick look no longer scrolls the project map. A menu pressed before its code arrived no longer closes straight away. The zh-TW and zh-CN interface strings were reworded in everyday speech.

#### The docs say what to use on Windows

Run the Linux `zeaburctl` in WSL; there is no Windows build yet.

#### The API refuses a malformed request body or path as a problem

A request body that wasn't valid JSON, didn't match the request, or came without `Content-Type: application/json` got a plain-text answer, and so did a path parameter that didn't parse. These now get an `application/problem+json` `400` like every other refusal. The detail says where the body is wrong, such as the field `changes[0].op` or a line and column. It no longer lists every value the field accepts. A method a path doesn't take, and `/api` itself, had empty answers. They now get a `405` problem with the `Allow` header, and a `404` problem. A request whose handler panicked got no answer and lost its connection. It now gets an `internal` problem, and the panic's message goes only to the log.

#### Zeabur ships with Zeabur Catalog v0.4.1

On a personal install with less than about 1.7 GiB of memory, VictoriaMetrics and VictoriaLogs are no longer installed. Templates show their summary in English, Traditional Chinese and Simplified Chinese, and the memory they used when idle. A component shows as tested only once the catalog's tests passed on its current file, on the server's Kubernetes distribution and platform. Switching between `single-node` and `multi-node` no longer changes the size of zot's and VictoriaMetrics' volumes, so both keep running. CrowdSec agents come back after their node restarts, and CrowdSec now needs cert-manager. An install created on `multi-node` runs three commands before its first upgrade, as the [catalog's v0.4.0 release notes](https://github.com/zeabur-oss/catalog/releases/tag/v0.4.0) say.

On `multi-node`, the database and storage operators, Reloader and k8up request about what they use, half or less of before, and VictoriaMetrics requests 224 MiB instead of 512 MiB while keeping two weeks of history ([v0.4.1 release notes](https://github.com/zeabur-oss/catalog/releases/tag/v0.4.1)).

#### apply says how to undo a failed chart without Helm on the server

When a chart's Helm release is left `failed`, `apply` now prints commands that run Helm inside the cluster, with helm-controller's own image and ServiceAccount: one shows the release's history. The other rolls it back, or uninstalls it when the first install failed. A server installed with zeabur-installer has no Helm, so before this you had to install it or copy the admin kubeconfig to another machine.

If you switched an install from `single-node` to `multi-node` on 0.2.0-beta.0 or beta.1, zot's release may already be `failed`, and the zot command in the catalog v0.4.0 notes doesn't fix it. Run the rollback that `apply` prints, then `apply` again.

#### apply restarts Longhorn's manager on a node it prepares

Longhorn's manager checks a node's packages and kernel modules only when it starts. When `apply` installed them on a node where Longhorn already ran, `doctor` kept reporting the node as not prepared until someone deleted its `longhorn-manager` Pod. `apply` now restarts that Pod after it prepares the node, on the server and on joined nodes. When the upgrade adds Longhorn to a cluster with other nodes, run the new installer's `apply` on each of them too.

#### plan warns when the memory reservation leaves a node short for its Pods

Zeabur 0.2 reserves memory for the system and K3s or RKE2, which shrinks what Pods can request. On a server that was nearly full, Pods could stay `Pending` with `Insufficient memory`, while `plan` reported nothing. On an upgrade, `plan` now compares what each node's Pods already request with what's left for Pods once the reservation applies, and warns and names the node when they don't fit. To make room, delete a service on that node or lower its replicas, move a service to another node, or add a node.

#### Zeabur Proxy leaves query concurrency to VictoriaMetrics and VictoriaLogs

Zeabur Proxy refused a third query from one person, or a seventeenth in all, while others were still running, so the usage tab could show "The metrics store did not respond." It now sends every allowed query on, and VictoriaMetrics and VictoriaLogs queue them under their own limits (`-search.maxConcurrentRequests`, `-search.maxQueueDuration`).

The proxy also no longer reads the namespace to cut history at its creation. The project's name is the boundary, so a new project with a deleted project's name shows its history until the storage's retention drops it. "Delete a project" says how to clear it first.

Logs and usage queries are no longer cut to the last week: they reach as far back as the storage keeps. The chart's `proxy.logs.maxHours` and `proxy.metrics.maxHours` are gone. The proxy also drops its own connection cap and header wait, since only Zeabur can reach it.

When it is stopped, as in a rolling update, the proxy takes no new connection and finishes the queries already under way instead of cutting them.

#### A long variable name on the Variables tab wraps instead of drawing over its value

In the list of a service's variables, a name wider than its column, such as `N8N_ENFORCE_SETTINGS_FILE_PERMISSIONS`, now breaks onto a second line and stays fully readable.

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
