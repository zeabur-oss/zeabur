# Install Zeabur in a cluster

Zeabur runs as one pod in its own namespace. People sign in through
[Zeabur Identity](https://github.com/zeabur/identity) with their passkey, and every request Zeabur makes
to kube-apiserver carries that person's token: Zeabur's service account has no Role, no
ClusterRole and no token of its own. What someone can see and change in Zeabur is exactly what
their Kubernetes RBAC allows. The chart's one Role lets everyone signed in read Zeabur's settings
(`get` on the ConfigMap `zeabur-settings`); turn it off with `metricsStore.everyoneReads=false`.
Its three ClusterRoles are the member roles, which admins give per project
([Members' rights](#members-rights)); the chart binds them to nobody.

This guide installs a release of Zeabur with its Helm chart (source in `deploy/chart/zeabur`).
The examples use these names; replace them with yours:

| Name | In the examples |
|---|---|
| Zeabur's public URL | `https://zeabur.example.com` |
| Zeabur Identity's issuer URL | `https://identity.example.com` |
| Zeabur's namespace | `zeabur` |

## What you need

- A cluster whose kube-apiserver trusts Zeabur Identity's tokens, with Zeabur Identity's sign-in page published
  over HTTPS: Zeabur Identity's [Get started](https://github.com/zeabur/identity/blob/main/docs/get-started.md)
  sets this up on k3s. Run `zeabur-identityctl` and `kubectl` below with the cluster admin's kubeconfig.
- An ingress controller (an `Ingress` or a Gateway API `Gateway`) and a TLS certificate for
  Zeabur's host name, for example from cert-manager. Browsers offer passkeys only on pages they
  trust.
- Helm 4, [cosign](https://docs.sigstore.dev/cosign/system_config/installation/) 3 and the
  [GitHub CLI](https://cli.github.com) (`gh`, signed in). This repository's `nix develop` shell
  provides all three.

## 1. Choose a release and verify it

There is nothing to build. Each [release](https://github.com/zeabur/zeabur/releases) publishes
the image `ghcr.io/zeabur/zeabur` (linux/amd64 and linux/arm64) and the chart
`oci://ghcr.io/zeabur/charts/zeabur`, whose default image is that image's digest. Both are
signed by the release workflow at the release's tag (keyless, with
[Sigstore](https://docs.sigstore.dev/)) and carry a
[build provenance attestation](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations/using-artifact-attestations-to-establish-provenance-for-builds).
The release lists their digests in `digests.txt`, covered by the signed `SHA256SUMS`:

```sh
version=0.0.2
identity="https://github.com/zeabur/zeabur/.github/workflows/release.yml@refs/tags/v$version"
issuer=https://token.actions.githubusercontent.com
gh release download "v$version" --repo zeabur/zeabur --dir "zeabur-$version" \
  --pattern digests.txt --pattern SHA256SUMS --pattern SHA256SUMS.sigstore.json
cd "zeabur-$version"
cosign verify-blob --bundle SHA256SUMS.sigstore.json --certificate-identity "$identity" \
  --certificate-oidc-issuer "$issuer" SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS                    # digests.txt: OK
image=$(awk '$1 == "index" { print $2 }' digests.txt)
chart=$(awk '$1 == "chart" { print $2 }' digests.txt)
for ref in "$image" "$chart"; do
  cosign verify --certificate-identity "$identity" --certificate-oidc-issuer "$issuer" "$ref" >/dev/null
  gh attestation verify "oci://$ref" --repo zeabur/zeabur --cert-identity "$identity"
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
[Registering clients](https://github.com/zeabur/identity/blob/main/docs/clients.md) for the
options.

## 3. Install the chart

Name the cluster as people know it, so those with several can tell which one they are in. The
kubeconfig's name for the cluster, without what k3d, kind, EKS and GKE add around it:

```sh
cluster=$(kubectl config view --minify -o jsonpath='{.clusters[0].name}')
cluster=${cluster#k3d-}; cluster=${cluster#kind-}             # k3d-shop, kind-shop
cluster=${cluster##*:cluster/}; cluster=${cluster%%.*.eksctl.io} # EKS ARN, eksctl
case $cluster in gke_*) cluster=${cluster##*_} ;; esac          # gke_<project>_<zone>_<name>
echo "$cluster"
```

```sh
helm install zeabur "oci://$chart" --namespace zeabur \
  --set clusterName="$cluster" \
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
| `clusterName` | What the interface calls the cluster; an administrator can rename it in Settings. Empty: a name read from the nodes where the person may list them (k3d, kind, minikube, EKS with eksctl, GKE, AKS, or the first control plane node), else the public URL's host. |
| `issuer.url`, `issuer.clientId` | Zeabur Identity's issuer URL and the client registered in step 2. |
| `issuer.clientSecret.existingSecret` | The Secret from step 2. The chart never takes the secret itself as a value, so it stays out of Helm's release records. |
| `issuer.caBundle.configMap` | A ConfigMap with the CA that signed Zeabur Identity's certificate, when a private CA did (mkcert, an internal CA). |
| `elevation.enabled` | Whether anything the cluster refuses to everyday rights (a change, a read, a terminal) offers "Retry as admin", which asks for the passkey again. While admin rights last, a few minutes, every request goes with them, as with `sudo`. |
| `ingress.*`, `httpRoute.*` | An Ingress, or a Gateway API HTTPRoute (`httpRoute.parentRefs` names the Gateway), for the public URL's host name. |

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
`terminal.toolsDeadlineMinutes` and `terminal.toolsResources` bound each tools pod.

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
| Developer | Chosen projects | Deploy, change variables, restart, read logs, open terminals, read Secret values | Other projects, nodes, members |
| Viewer | Chosen projects | See status, logs and events | Change anything, open a terminal, read Secret values |

Every member may also list nodes and namespaces, so the interface can show node names and every
project's name; a project without a role says an admin can give one.

Developer and Viewer are the ClusterRoles `zeabur-project-developer` and `zeabur-project-viewer`
(rules in `deploy/chart/zeabur/files/member-roles.yaml`). Giving a role makes a RoleBinding
`zeabur-member-<user>` to user `zeabur:<user>` in the project's namespace, labelled
`zeabur.com/member: <user>`, as the admin. Changing a role replaces it, and removing a member
removes their bindings first. Each rule names its verbs and resources, without `*`. Neither role
has RBAC objects, namespace changes, `nodes/proxy`, or `escalate`, `bind` and `impersonate`, so
no member can widen their own rights
([RBAC good practices](https://kubernetes.io/docs/concepts/security/rbac-good-practices/#privilege-escalation-risks)).

Compared with Kubernetes' built-in roles, Viewer reads what `view` reads that Zeabur shows, and
like `view` no Secrets. Developer is narrower than `edit`: no ServiceAccount writes or
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

The member baseline, `get` and `list` on `nodes` and `namespaces`, is the ClusterRole
`zeabur-member`. Zeabur binds it per member, with a ClusterRoleBinding `zeabur-member-<user>` made
on invitation or on a change of access: Zeabur Identity puts no group on every member, and
`system:authenticated` would grant it to every ServiceAccount too. A member made before this
release gets it when an admin first changes their access.

`memberRoles.enabled=false` installs no ClusterRole; Zeabur then offers only Admin or nothing,
and RBAC you write yourself decides the rest. The ClusterRole names are fixed: install one Zeabur
per cluster.

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
