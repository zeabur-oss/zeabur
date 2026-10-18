# Install engawa in a cluster

engawa runs as one pod in its own namespace. People sign in through
[Genkan](https://github.com/engawahq/genkan) with their passkey, and every request engawa makes
to kube-apiserver carries that person's token: engawa's service account has no Role, no
ClusterRole and no token of its own. What someone can see and change in engawa is exactly what
their Kubernetes RBAC allows. The chart's one Role lets everyone signed in read engawa's settings
(`get` on the ConfigMap `engawa-settings`); turn it off with `metricsStore.everyoneReads=false`.

This guide installs engawa with its Helm chart (`deploy/chart/engawa`). The examples use these
names; replace them with yours:

| Name | In the examples |
|---|---|
| engawa's public URL | `https://engawa.example.com` |
| Genkan's issuer URL | `https://genkan.example.com` |
| engawa's namespace | `engawa` |

## What you need

- A cluster whose kube-apiserver trusts Genkan's tokens, with Genkan's sign-in page published
  over HTTPS: Genkan's [Get started](https://github.com/engawahq/genkan/blob/main/docs/get-started.md)
  sets this up on k3s. Run `genkanctl` and `kubectl` below with the cluster admin's kubeconfig.
- An ingress controller (an `Ingress` or a Gateway API `Gateway`) and a TLS certificate for
  engawa's host name, for example from cert-manager. Browsers offer passkeys only on pages they
  trust.
- `helm`, and this repository checked out (`nix develop` provides `helm`).

## 1. Build the image

engawa has no published image yet. Build it on a Linux machine with Docker (it embeds the web
interface) and load it into the cluster's nodes; on k3s:

```sh
./scripts/build-image.sh engawa:dev
docker save engawa:dev | sudo k3s ctr images import -
```

## 2. Register engawa at Genkan

engawa is a confidential client: it has a server side that keeps a secret. It needs tokens that
kube-apiserver accepts (`--audience kubernetes`), and it asks for admin rights when the cluster
refuses something to the everyday ones (`--allow-elevation`):

```sh
genkanctl client create engawa --type confidential --allow-elevation --audience kubernetes \
  --redirect-uri https://engawa.example.com/auth/callback | kubectl apply -f -
```

`genkanctl` prints the client secret once, on stderr. Store it as a Secret in engawa's namespace
right away, pasting it rather than passing it as an argument (other users on the machine can see
arguments in the process list):

```sh
kubectl create namespace engawa
read -rs CLIENT_SECRET     # paste the secret, then Enter
printf %s "$CLIENT_SECRET" | kubectl -n engawa create secret generic engawa-oidc \
  --from-file=client-secret=/dev/stdin
unset CLIENT_SECRET
```

Without `--allow-elevation`, set `elevation.enabled=false` below: people then see and change
only what their everyday rights allow. See Genkan's
[Registering clients](https://github.com/engawahq/genkan/blob/main/docs/clients.md) for the
options.

## 3. Install the chart

```sh
helm install engawa deploy/chart/engawa --namespace engawa \
  --set image.repository=docker.io/library/engawa --set image.tag=dev \
  --set publicUrl=https://engawa.example.com \
  --set issuer.url=https://genkan.example.com \
  --set issuer.clientSecret.existingSecret=engawa-oidc \
  --set ingress.enabled=true --set ingress.className=traefik \
  --set ingress.tls.secretName=engawa-tls
kubectl -n engawa rollout status deployment/engawa
```

Open `https://engawa.example.com` and sign in with your passkey.

`values.yaml` documents every value, and `values.schema.json` refuses a plain `http` URL, a URL
with a path, or a missing client secret before anything is installed. The ones you are most
likely to set:

| Value | What it does |
|---|---|
| `publicUrl` | The address people open. Sign-in returns to `<publicUrl>/auth/callback`. |
| `issuer.url`, `issuer.clientId` | Genkan's issuer URL and the client registered in step 2. |
| `issuer.clientSecret.existingSecret` | The Secret from step 2. The chart never takes the secret itself as a value, so it stays out of Helm's release records. |
| `issuer.caBundle.configMap` | A ConfigMap with the CA that signed Genkan's certificate, when a private CA did (mkcert, an internal CA). |
| `elevation.enabled` | Whether anything the cluster refuses to everyday rights (a change, a read, a terminal) offers "Retry as admin", which asks for the passkey again. While admin rights last, a few minutes, every request goes with them, as with `sudo`. |
| `ingress.*`, `httpRoute.*` | An Ingress, or a Gateway API HTTPRoute (`httpRoute.parentRefs` names the Gateway), for the public URL's host name. |

### The plan key

Every change in engawa is first shown as a plan, and the plan is carried out by a token signed
with a key. The chart creates that key on install as a Secret (`engawa-plan-key`), reads it back
on upgrades with Helm's `lookup`, and keeps it. Tools that render charts without a cluster
(`helm template`, Argo CD) see no existing Secret and would make a new key on each render: name
your own Secret in `planKey.existingSecret` there, holding 32 random bytes or 64 hexadecimal
digits under the key `plan-key`:

```sh
kubectl -n engawa create secret generic engawa-plan-key --from-literal=plan-key="$(openssl rand -hex 32)"
```

A new key only means plans not yet applied have to be reviewed again.

### One replica

The chart runs exactly one pod, replaced rather than rolled on upgrades. Sign-in sessions live
in the process's memory, because their refresh tokens must not rest in etcd: a second replica
would not know them. A restart therefore signs everyone out; signing in again takes one passkey
touch.

## Check what engawa may do

engawa's service account may do nothing a new service account cannot:

```sh
kubectl auth can-i impersonate users --as=system:serviceaccount:engawa:engawa    # no
kubectl get rolebindings,clusterrolebindings -A -o wide | grep -w engawa         # nothing
```

## Uninstall

```sh
helm uninstall engawa --namespace engawa
kubectl delete namespace engawa
kubectl -n genkan-system delete oauthclient engawa
```

engawa keeps no state outside its namespace: no custom resources, nothing cluster-wide. After
these commands the cluster is as it was before step 2. `scripts/e2e-l2.sh` checks this: it
records every cluster-scoped object and every namespace's labels and annotations before the
install and compares them after the removal.
