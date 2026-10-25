# Install Zeabur in a cluster

Zeabur runs as one pod in its own namespace. People sign in through
[Zeabur Identity](https://github.com/zeabur-oss/identity) with their passkey, and every request Zeabur makes
to kube-apiserver carries that person's token: Zeabur's service account has no Role, no
ClusterRole and no token of its own. What someone can see and change in Zeabur is exactly what
their Kubernetes RBAC allows. The chart's one Role lets everyone signed in read Zeabur's settings
(`get` on the ConfigMap `zeabur-settings`); turn it off with `metricsStore.everyoneReads=false`.
Its three ClusterRoles are the member roles, which admins give per project
([Members' rights](#members-rights)); the chart binds them to nobody. Its admission policy keeps
a Developer's change of their project's Namespace to the project's details.

This guide installs a release of Zeabur with its Helm chart (source in `deploy/chart/zeabur`).
The examples use these names; replace them with yours:

| Name | In the examples |
|---|---|
| Zeabur's public URL | `https://zeabur.example.com` |
| Zeabur Identity's issuer URL | `https://identity.example.com` |
| Zeabur's namespace | `zeabur` |

## What you need

- Kubernetes 1.30 or later, for the member roles' admission policy (`ValidatingAdmissionPolicy`
  is GA from 1.30). With `memberRoles.enabled=false` the chart installs none.
- A cluster whose kube-apiserver trusts Zeabur Identity's tokens, with Zeabur Identity's sign-in page published
  over HTTPS: Zeabur Identity's [Get started](https://github.com/zeabur-oss/identity/blob/main/docs/get-started.md)
  sets this up on k3s. Run `zeabur-identityctl` and `kubectl` below with the cluster admin's kubeconfig.
- An ingress controller (an `Ingress` or a Gateway API `Gateway`) and a TLS certificate for
  Zeabur's host name, for example from cert-manager. Browsers offer passkeys only on pages they
  trust.
- Helm 4, [cosign](https://docs.sigstore.dev/cosign/system_config/installation/) 3 and the
  [GitHub CLI](https://cli.github.com) (`gh`, signed in). This repository's `nix develop` shell
  provides all three.

## 1. Choose a release and verify it

There is nothing to build. Each [release](https://github.com/zeabur-oss/zeabur/releases) publishes
the image `ghcr.io/zeabur-oss/zeabur` (linux/amd64 and linux/arm64) and the chart
`oci://ghcr.io/zeabur-oss/charts/zeabur`, whose default image is that image's digest. Both are
signed by the release workflow at the release's tag (keyless, with
[Sigstore](https://docs.sigstore.dev/)) and carry a
[build provenance attestation](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations/using-artifact-attestations-to-establish-provenance-for-builds).
The release lists their digests in `digests.txt`, covered by the signed `SHA256SUMS`:

```sh
version=0.0.2
identity="https://github.com/zeabur-oss/zeabur/.github/workflows/release.yml@refs/tags/v$version"
issuer=https://token.actions.githubusercontent.com
gh release download "v$version" --repo zeabur-oss/zeabur --dir "zeabur-$version" \
  --pattern digests.txt --pattern SHA256SUMS --pattern SHA256SUMS.sigstore.json
cd "zeabur-$version"
cosign verify-blob --bundle SHA256SUMS.sigstore.json --certificate-identity "$identity" \
  --certificate-oidc-issuer "$issuer" SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS                    # digests.txt: OK
image=$(awk '$1 == "index" { print $2 }' digests.txt)
chart=$(awk '$1 == "chart" { print $2 }' digests.txt)
for ref in "$image" "$chart"; do
  cosign verify --certificate-identity "$identity" --certificate-oidc-issuer "$issuer" "$ref" >/dev/null
  gh attestation verify "oci://$ref" --repo zeabur-oss/zeabur --cert-identity "$identity"
done
helm show values "oci://$chart" | grep -A6 '^image:'              # digest: the image above
```

Install the chart by `$chart`, its digest, as below: a tag can be moved, a digest cannot.

## 2. Register Zeabur at Zeabur Identity

Zeabur is a confidential client: it has a server side that keeps a secret. It needs tokens that
kube-apiserver accepts (`--audience kubernetes`), and it asks for admin rights when the cluster
refuses something to the everyday ones (`--allow-elevation`):

```sh
zeabur-identityctl client create zeabur --type confidential --allow-elevation --audience kubernetes --display-name Zeabur \
  --redirect-uri https://zeabur.example.com/auth/callback --home-url https://zeabur.example.com/ \
  | kubectl apply -f -
```

`zeabur-identityctl` prints the client secret once, on stderr. Store it as a Secret in Zeabur's namespace
right away, pasting it rather than passing it as an argument (other users on the machine can see
arguments in the process list):

```sh
kubectl create namespace zeabur
read -rs CLIENT_SECRET     # paste the secret, then Enter
printf %s "$CLIENT_SECRET" | kubectl -n zeabur create secret generic zeabur-oidc \
  --from-file=client-secret=/dev/stdin
unset CLIENT_SECRET
```

Without `--allow-elevation`, set `elevation.enabled=false` below: people then see and change
only what their everyday rights allow. See Zeabur Identity's
[Registering clients](https://github.com/zeabur-oss/identity/blob/main/docs/clients.md) for the
options.

## 3. Install the chart

Name the cluster as people know it (`clusterName`), so those with several can tell which one
they are in.

```sh
helm install zeabur "oci://$chart" --namespace zeabur \
  --set clusterName=shop-prod \
  --set publicUrl=https://zeabur.example.com \
  --set issuer.url=https://identity.example.com \
  --set issuer.clientSecret.existingSecret=zeabur-oidc \
  --set ingress.enabled=true --set ingress.className=traefik \
  --set ingress.tls.secretName=zeabur-tls
kubectl -n zeabur rollout status deployment/zeabur
```

Open `https://zeabur.example.com` and sign in with your passkey.

`values.yaml` documents every value, and `values.schema.json` refuses a plain `http` URL, a URL
with a path, or a missing client secret before anything is installed. The ones you are most
likely to set:

| Value | What it does |
|---|---|
| `publicUrl` | The address people open. Sign-in returns to `<publicUrl>/auth/callback`. |
| `clusterName` | What the interface calls the cluster; an administrator can rename it in Settings. Empty: the public URL's host. |
| `issuer.url`, `issuer.clientId` | Zeabur Identity's issuer URL and the client registered in step 2. |
| `issuer.clientSecret.existingSecret` | The Secret from step 2. The chart never takes the secret itself as a value, so it stays out of Helm's release records. |
| `issuer.accountUrl` | Where people manage their passkeys and linked accounts; the Account page links there. Empty: the `manage` URL the issuer publishes at `/.well-known/passkey-endpoints`, else no link. |
| `issuer.caBundle.configMap` | A ConfigMap with the CA that signed Zeabur Identity's certificate, when a private CA did (mkcert, an internal CA). |
| `elevation.enabled` | Whether anything the cluster refuses to everyday rights (a change, a read, a terminal) offers "Retry as admin", which asks for the passkey again. Only people who have elevated groups in Zeabur Identity get the offer. While admin rights last, a few minutes, every request goes with them, as with `sudo`. |
| `ingress.*`, `httpRoute.*` | An Ingress, or a Gateway API HTTPRoute (`httpRoute.parentRefs` names the Gateway), for the public URL's host name. |
| `domains.suffixes`, `domains.clusterIssuer` | How a new service's public URL is made; see [Public URLs for services](#public-urls-for-services). |

### The plan key

Every change in Zeabur is first shown as a plan, and the plan is carried out by a token signed
with a key. The chart creates that key on install as a Secret (`zeabur-plan-key`), reads it back
on upgrades with Helm's `lookup`, and keeps it. Tools that render charts without a cluster
(`helm template`, Argo CD) see no existing Secret and would make a new key on each render: name
your own Secret in `planKey.existingSecret` there, holding 32 random bytes or 64 hexadecimal
digits under the key `plan-key`:

```sh
kubectl -n zeabur create secret generic zeabur-plan-key --from-literal=plan-key="$(openssl rand -hex 32)"
```

A new key only means plans not yet applied have to be reviewed again.

### Public URLs for services

When someone deploys an image with a public URL, Zeabur plans an Ingress for it, with the
cluster's default IngressClass, made with their own rights like every other change.

- **`domains.suffixes`**: the domains new URLs are suggested under, the first one by default,
  such as `apps.example.com`. A service `web` in project `shop` is offered
  `web-shop.apps.example.com`. Point a wildcard DNS record, `*.apps.example.com`, at the ingress
  controller's address. The **Domains** section of the cluster page checks each record, shows the
  one to add, and can change the list later. Empty (the default): people type the whole host and
  set up its DNS themselves.
- **`domains.clusterIssuer`**: the cert-manager ClusterIssuer that signs new URLs' certificates,
  such as `letsencrypt`. The Ingress then carries the annotation
  `cert-manager.io/cluster-issuer: <issuer>` and a `tls` block for the host, with the Secret
  `<service>-tls`; both are in the plan before anyone confirms it. cert-manager's ingress-shim
  then requests the certificate ([cert-manager: securing Ingress resources][cm-ingress]). Empty
  (the default): new URLs are HTTP only, and the form says so.

Zeabur does not look for a ClusterIssuer itself: project members usually may not list
ClusterIssuers, which are cluster-scoped, and Zeabur's service account reads nothing on their
behalf. Name the one to use here, after installing cert-manager and creating it.

[cm-ingress]: https://cert-manager.io/docs/usage/ingress/

### One replica

The chart runs exactly one pod, replaced rather than rolled on upgrades. Sign-in sessions live
in the process's memory, because their refresh tokens must not rest in etcd: a second replica
would not know them. A restart therefore signs everyone out; signing in again takes one passkey
touch.

### Terminals

A terminal in a container without a shell adds a debug container to the pod (busybox, pinned
by digest; `terminal.debugImage`). A tools terminal starts a short-lived pod with `kubectl` and
`helm` (`alpine/k8s`, pinned by digest; `terminal.toolsImage`) that acts with the token of the
person who opened it. Both are created as that person, so they need `patch` on
`pods/ephemeralcontainers`, or `create` and `delete` on `pods`, plus `get` on `pods/exec`.
With `terminal.toolsNamespace`, tools pods go in a namespace of their own instead of the
project's; give people those rights there, and nobody else `pods/exec` in it.
Administrators also open a tools terminal for the whole cluster, from the Components page; its
pods go in `terminal.clusterToolsNamespace` (`zeabur-cluster-tools`, created by the first plan
that needs it), never in `terminal.toolsNamespace`, since they hold an administrator's token.
`terminal.toolsDeadlineMinutes` and `terminal.toolsResources` bound each tools pod.

### Use, disk and history

Use history, node disk use and volume use are read from the metrics store
(`metricsStore.service`, or the one chosen in Settings) as the person, so they need `get` on
that Service's `services/proxy`. The store needs node-exporter for node disk and the kubelets'
metrics for volume use; VictoriaMetrics' k8s-stack chart and kube-prometheus-stack collect both
by default. Zeabur Catalog's single-node VictoriaMetrics profile turns node-exporter off, so
there the cluster page says node disk use is not collected. Nobody needs `nodes/proxy`.

### Log history

With `proxy.enabled=true`, the Logs tab also shows lines from replicas that are gone, read from
VictoriaLogs (Zeabur Catalog's logs capability; `proxy.logs`). VictoriaLogs holds every
namespace's lines, so Zeabur never reads it. Zeabur Proxy does, as the person: it reads only
the project's namespace, and only for people who may `get` `logs.proxy.zeabur.com` there.
The chart's `zeabur-proxy-logs-reader` gives that to project Developers and to Kubernetes'
`view`, `edit` and `admin`, who already read the pods' logs. Viewers read no logs.

Gateway API has no roles for Kubernetes' `view`, so the chart's `zeabur-gateway-routes-reader`
adds reading HTTPRoutes to it, as `view` already reads Ingresses.

Zeabur reaches the proxy over TLS with a certificate the chart makes and keeps across upgrades.
To use your own, for example a cert-manager Certificate, name its Secret in
`proxy.tls.existingSecret`; it needs `ca.crt`. The proxy picks up a renewed certificate on its
own; Zeabur reads the CA when it starts. The proxy's NetworkPolicy lets it reach DNS,
VictoriaLogs and kube-apiserver only. Where kube-apiserver is not at the addresses of
`default/kubernetes`, list its CIDRs in `proxy.apiServer`.

### Reading images' ports

When someone deploys an image, Zeabur reads its configuration from the registry to suggest the
port it listens on (its `EXPOSE`), so the server needs outbound HTTPS to the registries people
use. Public images on Docker Hub, GitHub and Quay are read without credentials; private ones
with the credentials typed in the form or the project's pull Secret, read as the person. The
server never connects to loopback, link-local (such as a cloud's metadata service), multicast
or unspecified addresses. Zeabur Builder's registry (`builds.registry`) is read over plain HTTP;
every other registry over HTTPS. When the registry cannot be reached the form says so and keeps
the port empty.

### An image of your own

To run a build of your own instead, build the image on a Linux machine with Docker
(`./scripts/build-image.sh zeabur:dev`), load it into the cluster's nodes (on k3s:
`docker save zeabur:dev | sudo k3s ctr images import -`) and install the chart from this
repository, `deploy/chart/zeabur`, with `--set image.repository=docker.io/library/zeabur
--set image.tag=dev`.

## Members' rights

On the Members page an admin makes each member an Admin, or a Developer or Viewer in chosen
projects:

| Role | Where | Can | Cannot |
|---|---|---|---|
| Admin | The cluster | What Zeabur Identity's `admins` group may do, with admin rights when asked for | — |
| Developer | Chosen projects | Deploy, change variables, restart, read logs, open terminals, read Secret values, change the project's name, description and icon | Other projects, nodes, members, adopting or removing projects |
| Viewer | Chosen projects | See status and events | Read logs, change anything, open a terminal, read Secret values |

Every member may also list nodes, namespaces and StorageClasses, and get a PersistentVolume by
name, so the interface can show node names, every project's name and details, the storage a new
volume can use and the node a volume is kept on; a project without a role says an admin can give
one. Listing
namespaces returns their annotations too, so keep secrets out of a project's description.

A Developer may `patch` their project's Namespace, and RBAC cannot narrow that to some fields.
The chart's ValidatingAdmissionPolicy `zeabur-project-namespace` keeps such a patch to the three
detail annotations (`zeabur.com/display-name`, `zeabur.com/description`, `zeabur.com/icon`):
labels decide Pod Security admission and which NetworkPolicies of other projects select the
namespace, and adopting is a label. The policy skips whoever may `zeabur-manage` the Namespace
(a custom verb that `cluster-admin`'s `*` covers) or patch every namespace, so admins and
controllers work as before. It needs Kubernetes 1.30 or later.

A project Zeabur creates enforces, audits and warns at Pod Security's `restricted` level (the
`pod-security.kubernetes.io/enforce`, `audit` and `warn` labels), and a new service is Strict:
its pod template sets `runAsNonRoot`, the `RuntimeDefault` seccomp profile and
`allowPrivilegeEscalation: false`, drops every capability, and does not mount the service
account's token. A service built from Git is Strict too: Zeabur Builder's Railpack images run as
uid 1000, and a Dockerfile needs a numeric user other than root, such as `USER 1000`. A service
whose image must run as root can be Compatible (`baseline`), chosen when it is created or in its
settings; the same plan lowers the project's `enforce` label to `baseline` and leaves `audit`
and `warn` at `restricted`. That is a label, so the policy above leaves it to whoever may
`zeabur-manage` the Namespace: a Developer asks an admin, or takes admin rights. Zeabur does not raise the label back by itself; when the last Compatible service is made
Strict, the plan offers to, and refuses while any workload in the namespace would break
`restricted`. A namespace without an `enforce` label is never lowered: the cluster's default
applies there.

Developer and Viewer are the ClusterRoles `zeabur-project-developer` and `zeabur-project-viewer`.
Each aggregates the rule sets `deploy/chart/zeabur/files/member-role-sets.yaml` names for it,
whose rules are in `deploy/chart/zeabur/files/member-roles.yaml`: its own, and Zeabur Builder's.
Both see a project's builds; Developers may also build again and turn on automatic updates
(`patch` on `builds.builder.zeabur.com`). To leave that to admins, remove
`zeabur-project-builds-write` from the developer's line. Deploying a build changes the
workload's image, which Developers may do anyway. Giving a role makes a RoleBinding
`zeabur-member-<user>` to user `zeabur:<user>` in the project's namespace, labelled
`zeabur.com/member: <user>`, as the admin. Changing a role replaces it, and removing a member
removes their bindings first. Each rule names its verbs and resources, without `*`. Neither role
has RBAC objects, namespace creation or deletion, `nodes/proxy`, or `escalate`, `bind` and
`impersonate`, so no member can widen their own rights
([RBAC good practices](https://kubernetes.io/docs/concepts/security/rbac-good-practices/#privilege-escalation-risks)).
Neither role has `velero.io`: Velero writes a Restore's objects with its own service account
(`cluster-admin` by default) into each namespace the Restore names, RoleBindings and Secrets
included, and RBAC on the Restore cannot limit those namespaces. Cluster backups and restores
stay with admins; the restore button is closed to anyone who may not create a Restore.

Compared with Kubernetes' built-in roles, Viewer reads what `view` reads that Zeabur shows, and
like `view` no Secrets. Unlike `view`, it reads no logs (`pods/log`): logs often carry what
Secrets hold. Developer is narrower than `edit`: no ServiceAccount writes or
impersonation, no service or pod proxy, no `deletecollection`. What the Developer's sensitive
rights mean:

| Right | Means |
|---|---|
| `secrets` | Reads every Secret value in the project, as a list returns them ([listing Secrets](https://kubernetes.io/docs/concepts/security/rbac-good-practices/#listing-secrets)). |
| `create` on `pods` | Runs a pod as any ServiceAccount of the project, with that account's rights ([workload creation](https://kubernetes.io/docs/concepts/security/rbac-good-practices/#workload-creation)). |
| `pods/exec`, `pods/attach` | A shell in a container: whatever it can read, its mounted Secrets and token included. |
| `pods/ephemeralcontainers` | A debug container next to the app's, the same reach as exec. |
| `pods/portforward` | Connects to any port of a pod from their machine, through the kubelet rather than the pod network, so NetworkPolicies do not stop it. |

Removing a role ends new requests at once. A terminal already open keeps running until it ends,
as kube-apiserver checks a stream when it starts.

The member baseline, `get` and `list` on `nodes`, `namespaces` and `storageclasses`, and `get`
on `persistentvolumes`, is the ClusterRole `zeabur-member`. Zeabur binds it per member, with a
ClusterRoleBinding `zeabur-member-<user>` made on invitation or on a change of access: Zeabur
Identity puts no group on every member, and `system:authenticated` would grant it to every
ServiceAccount too.

Upgrading from a release without roles means inviting members again: Zeabur does not migrate the
members made before roles. Remove each of them on the Members page and invite them anew; inviting
someone who is still a member only gives them a setup link.

`memberRoles.enabled=false` installs no ClusterRole and no admission policy; Zeabur then offers
only Admin or nothing, and RBAC you write yourself decides the rest. The ClusterRole names are fixed: install one Zeabur
per cluster.

## Tokens for scripts and CI

Zeabur has no API tokens of its own. Its state is Kubernetes objects, so a script uses a
ServiceAccount token with kube-apiserver, and RBAC decides what it may do. To give a CI job the
Developer role in the project `shop` (the same three ClusterRoles a Developer member gets):

```sh
kubectl -n shop create serviceaccount ci
for role in zeabur-project-developer zeabur-project-builds-read zeabur-project-builds-write; do
  kubectl -n shop create rolebinding "ci-$role" --clusterrole="$role" --serviceaccount=shop:ci
done
kubectl -n shop create token ci --duration=1h       # a short-lived token
```

The job then runs, for example, `kubectl -n shop rollout restart deployment/api` or
`kubectl -n shop set image deployment/api api=<image>`. Zeabur shows these changes as it shows its
own. If the CI cannot request a new token for each run, put a long-lived token in a Secret of type
`kubernetes.io/service-account-token`
([Kubernetes docs](https://kubernetes.io/docs/tasks/configure-pod-container/configure-service-account/#manually-create-an-api-token-for-a-serviceaccount)).
Delete that Secret to revoke the token.

## Check what Zeabur may do

Zeabur's service account may do nothing a new service account cannot:

```sh
kubectl auth can-i impersonate users --as=system:serviceaccount:zeabur:zeabur    # no
kubectl get rolebindings,clusterrolebindings -A -o wide | grep -w zeabur/zeabur  # nothing
```

To see what a member may do, ask as them:

```sh
kubectl auth can-i --list -n shop --as=zeabur:bob
```

## Uninstall

```sh
helm uninstall zeabur --namespace zeabur
kubectl delete namespace zeabur
kubectl -n zeabur-identity delete oauthclient zeabur
kubectl delete rolebindings,clusterrolebindings -A -l zeabur.com/member
```

Zeabur keeps no state outside its namespace but the member bindings admins made: no custom
resources, and only those bindings cluster-wide, which the last command removes. After these
commands the cluster is as it was before step 2. `scripts/e2e-l2.sh` checks this: it
records every cluster-scoped object and every namespace's labels and annotations before the
install and compares them after the removal.
