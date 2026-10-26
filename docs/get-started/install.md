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
  over HTTPS: Zeabur Identity's [Get started](https://oss.zeabur.com/docs/identity/get-started)
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
version=0.1.0
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
kube-apiserver accepts (`--audience kubernetes`):

```sh
zeabur-identityctl client create zeabur --type confidential --audience kubernetes --display-name Zeabur \
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

See Zeabur Identity's
[Registering clients](https://oss.zeabur.com/docs/identity/clients) for the
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
| `extraTrustedCA.configMap` or `extraTrustedCA.secret` | PEM certificates Zeabur trusts for every HTTPS connection it makes: Zeabur Identity, registries, object stores and GitHub. Use it when one of them, or a proxy that inspects TLS, has a certificate a private CA signed. See [A private CA](#a-private-ca). |
| `ingress.*`, `httpRoute.*` | An Ingress, or a Gateway API HTTPRoute (`httpRoute.parentRefs` names the Gateway), for the public URL's host name. |
| `domains.suffixes`, `domains.clusterIssuer` | How a new service's public URL is made; see [Public URLs for services](#public-urls-for-services). |

### A private CA

Put the CA certificates in a ConfigMap (or a Secret) in Zeabur's namespace, then name it:

```sh
kubectl -n zeabur create configmap corp-ca --from-file=ca.crt=corp-ca.pem
helm upgrade zeabur "oci://$chart" --version "$version" -n zeabur --reuse-values \
  --set extraTrustedCA.configMap=corp-ca
```

`extraTrustedCA.key` names the key in it, `ca.crt` by default, and the file may hold several
certificates. Zeabur reads the file when it starts, so after you change the ConfigMap, restart it
with `kubectl -n zeabur rollout restart deployment/zeabur`. The certificates are added to the
Mozilla CA bundle. Zeabur still refuses loopback and link-local addresses.

zeabur-installer also sets `issuer.caBundle` (the local CA that signs Zeabur Identity's
certificate) and `builds.caBundle` (Zot's CA). Each names a ConfigMap the installer keeps up to
date, and Zeabur trusts them on top of `extraTrustedCA`, so you do not need to set them yourself.

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
person who opened it. The debug container is created as that person, so it needs `patch` on
`pods/ephemeralcontainers` plus `get` on `pods/exec`. Tools pods, for a project and for the
whole cluster (administrators, from the Components page), all go in `terminal.namespace`
(`zeabur-terminals`), which the chart creates. Zeabur asks kube-apiserver whether the person may
`create` `pods/exec` in the project (for the cluster: whether they are a cluster administrator),
then creates the pod and runs the terminal with its own ServiceAccount, which the chart lets
create, read, delete and exec into pods in that namespace and nothing else. Give nobody else
rights there: each pod holds its person's token, so only Zeabur and cluster administrators can
read it. `terminal.toolsDeadlineMinutes` and `terminal.toolsResources` bound each tools pod.

### Use, disk and history

Use history, node disk use and volume use are read from the metrics store
(`metricsStore.service`, or the one chosen in Settings) as the person, so they need `get` on
that Service's `services/proxy`. The store needs node-exporter for node disk and the kubelets'
metrics for volume use; VictoriaMetrics' k8s-stack chart and kube-prometheus-stack collect both
by default. Zeabur Catalog's single-node VictoriaMetrics profile turns node-exporter off, so
there the cluster page says node disk use is not collected. Nobody needs `nodes/proxy`.

### Log history

The installer sets `proxy.enabled=true` when it installs VictoriaLogs. With it, the Logs tab also shows lines from replicas that are gone, read from
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
(`./scripts/build-image.sh zeabur:dev`), load it into the cluster's nodes (on K3s:
`docker save zeabur:dev | sudo k3s ctr images import -`; on RKE2:
`docker save zeabur:dev | sudo /var/lib/rancher/rke2/bin/ctr --address /run/k3s/containerd/containerd.sock -n k8s.io images import -`) and install the chart from this
repository, `deploy/chart/zeabur`, with `--set image.repository=docker.io/library/zeabur
--set image.tag=dev`.

## Members' rights

On the Members page an admin makes each member an Admin, or a Developer or Viewer in chosen
projects:

| Role | Where | Can | Cannot |
|---|---|---|---|
| Admin | The cluster | Everything: an admin account, whose tokens carry Zeabur Identity's `admins` group, bound to `cluster-admin` | — |
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
Developers cannot change Namespace labels, including setting a Pod Security level to `privileged`.

A project Zeabur creates enforces, audits, and warns at Pod Security's `baseline` level (the
`pod-security.kubernetes.io/enforce`, `audit`, and `warn` labels), so Developers can run root
images such as `nginx:latest` without admin rights. For a new Docker image service, Zeabur reads
the image config's `User`: a numeric UID other than 0, such as `101` or `1000:1000`, selects
Strict; no user, `root`, `0`, or a user name such as `nginx` selects Compatible. The form
explains the choice, and either profile can be selected before creating the service.
For Git deployments, Railpack builds default to Strict (their images run as UID 1000), and
Dockerfile builds default to Compatible; the choice can be changed. Compose imports use Compatible.

Strict applies `restricted` settings to the service's pod template: `runAsNonRoot`, the
`RuntimeDefault` seccomp profile, `allowPrivilegeEscalation: false`, all capabilities dropped,
and no service account token mount. Compatible uses `baseline`. Switching an existing service
between Strict and Compatible changes only that service and needs no admin rights. It leaves
the namespace labels unchanged.

A namespace an admin has deliberately set to enforce `restricted` still refuses root
containers. The service settings ask an admin to change the project's Pod Security level to
`baseline`. To change it, an admin runs this once per namespace:

```sh
kubectl label namespace <ns> pod-security.kubernetes.io/enforce=baseline pod-security.kubernetes.io/audit=baseline pod-security.kubernetes.io/warn=baseline --overwrite
```

RKE2's CIS default remains `restricted` for unlabelled namespaces; a project's own `baseline`
labels override it. `kube-system` and Zeabur's own namespaces are unchanged.

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
Developers may also restore volumes: `create` and `delete` on `restores.k8up.io` in the project's
namespace, nothing cluster-wide. Two warnings come with it. An in-place restore overwrites the
volume, and files added after the backup are removed. Whoever may restore can read every backup in
the repository the project's Schedule names, so give each project its own backup path and password.
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

`memberRoles.enabled=false` installs no ClusterRole and no admission policy; Zeabur then offers
only Admin or nothing, and RBAC you write yourself decides the rest. The ClusterRole names are fixed: install one Zeabur
per cluster.

## Admin accounts and sessions

Zeabur Identity has two kinds of account. An **admin account** signs in with a passkey only,
never GitHub or Google, and its tokens carry the `admins` group, which Zeabur Identity binds to
`cluster-admin`: an admin acts with full rights at once, with no second confirmation. A
**regular account** signs in any way the server offers and has the rights its roles give. The
Admin role on the Members page is the admin account: giving it turns the person's account into
an admin account (their GitHub or Google session ends at its next refresh), and taking it away
makes the account regular again. A regular account that lacks a right sees which rule is missing
and asks an admin.

There is no sudo mode. The admin account plays that part, so admins keep a regular account for
everyday work; teams that want more turn on AAL3 (below).

The first admin comes from the setup link (`zeabur-identityctl setup-link`). On the command
line, `zeabur-identityctl invite --username alice --admin` invites an admin and
`zeabur-identityctl account-type alice admin` (or `regular`) changes an existing account; see
Zeabur Identity's [Admin accounts](https://oss.zeabur.com/docs/identity/managing-users#admin-accounts).

| Account | Access token | Session ends when idle for | Session ends at the latest |
|---|---|---|---|
| Regular | 10 minutes | 1 hour | 12 hours after sign-in |
| Admin, one team | 5 minutes | 1 hour | 12 hours after sign-in |
| Admin, several teams or AAL3 | 5 minutes | 15 minutes | 12 hours after sign-in |

Zeabur's own session ends at the shorter of its idle limit (1 hour) and the idle time Zeabur
Identity gives the refresh token, and 12 hours after sign-in at the latest. A page left open
does not keep it alive: after 60 seconds without input, the browser marks its automatic requests
as background, and they do not count as activity. A terminal stays open while used and closes
after 15 minutes idle.

### Sign-in settings

The Settings page has two sign-in settings. Both change admin accounts only. They live in the
ConfigMap `zeabur-identity-settings` in Zeabur Identity's namespace, which Zeabur Identity reads
at every sign-in and refresh. There is no Helm value for them, since Zeabur Identity is not part
of this chart: use the Settings page, or kubectl on that ConfigMap.

| Key | Values | Default | What it does |
|---|---|---|---|
| `teams` | `single`, `multiple` | `single` | With `multiple`, idle admin sessions end after 15 minutes. |
| `admin-aal3` | `false`, `true` | `false` | With `true`, admin accounts accept only device-bound passkeys, and idle admin sessions end after 15 minutes. |

```sh
kubectl -n zeabur-identity patch configmap zeabur-identity-settings --type merge -p '{"data":{"teams":"multiple"}}'
```

A missing key gives the default; a value Zeabur Identity does not understand gives the stricter
setting.

With AAL3, a device-bound passkey is one whose authenticator reports it cannot be backed up
(the WebAuthn backup eligibility flag, BE, unset), such as a security key or a passkey that
stays on one device. Synced passkeys, such as those in iCloud Keychain or Google Password
Manager, are refused at sign-in with a plain message. The authenticator reports BE itself and
Zeabur Identity asks for no attestation, so nothing verifies it: AAL3 here keeps synced
passkeys out, it does not prove what hardware holds the key. Zeabur refuses to turn AAL3 on
unless the admin making the change has a device-bound passkey; other admins without one are
locked out until `zeabur-identityctl setup-link --username <admin> --reset`. Regular accounts
never change. See Zeabur Identity's
[AAL3 for admins](https://oss.zeabur.com/docs/identity/managing-users#aal3-for-admins).

### kubectl

An admin's kubeconfig context is a plain kubelogin (`oidc-login`) context with a refresh token,
like a regular one: plain `kubectl` works, with the limits above, and nobody re-confirms with a
passkey every few minutes. kubectl has no extra confirmation; an admin account's protection
there is the passkey sign-in and the session limits. kubelogin keeps the refresh token in
`~/.kube/cache/oidc-login`; add `--token-cache-storage=keyring` to the context's exec args to
keep it in the OS keyring instead, or `--token-cache-storage=none` to keep nothing and sign in
for each command.

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

Zeabur's service account may use pods in the terminal namespace and nothing else:

```sh
kubectl auth can-i impersonate users --as=system:serviceaccount:zeabur:zeabur    # no
kubectl auth can-i create pods -n default --as=system:serviceaccount:zeabur:zeabur  # no
kubectl get rolebindings,clusterrolebindings -A -o wide | grep -w zeabur/zeabur
# only zeabur-terminals/zeabur-terminals
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
resources, and only those bindings cluster-wide, which the last command removes. The sign-in
settings, if anyone changed them, stay in Zeabur Identity's ConfigMap `zeabur-identity-settings`,
which Zeabur Identity keeps using. After these
commands the cluster is as it was before step 2. `scripts/e2e-l2.sh` checks this after removing an installer-made install: it
records every cluster-scoped object and every namespace's labels and annotations before the
install and compares them after the removal.
