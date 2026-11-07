# Profile: single-node

kubectl create namespace crowdsec --dry-run=client -o yaml | kubectl apply -f -
kubectl label namespace crowdsec --overwrite \
  pod-security.kubernetes.io/enforce=privileged \
  pod-security.kubernetes.io/audit=privileged \
  pod-security.kubernetes.io/warn=privileged
# The Traefik bouncer uses this key to call the CrowdSec local API.
kubectl -n crowdsec create secret generic crowdsec-bouncer-key \
  --from-literal=key="$(openssl rand -hex 64)" \
  --dry-run=client -o yaml | kubectl apply -f -
# The local API (LAPI) signs agent sessions with this shared secret.
# The chart's LAPI reads this token. Auto-registration is off, so nothing uses it.
kubectl -n crowdsec create secret generic crowdsec-lapi \
  --from-literal=csLapiSecret="$(openssl rand -hex 64)" \
  --from-literal=registrationToken="$(openssl rand -hex 64)" \
  --dry-run=client -o yaml | kubectl apply -f -

# ca
kubectl create namespace crowdsec --dry-run=client -o yaml | kubectl apply -f -
kubectl -n crowdsec apply --server-side -f - <<'ZEABUR_MANIFESTS'
apiVersion: cert-manager.io/v1
kind: Issuer
metadata: {name: crowdsec-selfsigned, namespace: crowdsec}
spec: {selfSigned: {}}
---
apiVersion: cert-manager.io/v1
kind: Certificate
metadata: {name: crowdsec-ca, namespace: crowdsec}
spec:
  isCA: true
  commonName: crowdsec-ca
  secretName: crowdsec-ca
  # Ten years, like the certificates it signs.
  duration: 87600h
  renewBefore: 720h
  privateKey: {algorithm: ECDSA, size: 256}
  issuerRef: {name: crowdsec-selfsigned, kind: Issuer}
---
apiVersion: cert-manager.io/v1
kind: Issuer
metadata: {name: crowdsec-ca, namespace: crowdsec}
spec:
  ca: {secretName: crowdsec-ca}
ZEABUR_MANIFESTS

# crowdsec
cat > crowdsec-values.yaml <<'ZEABUR_VALUES'
# Pinned by digest; a tag alone can move.
image:
  repository: crowdsecurity/crowdsec
  tag: v1.8.2@sha256:65b74942f7e7ed5c60fde4ac15fc4ba715904c398c8b16dd451f4907d90347d4
# The runtime on k3s and mainstream distributions.
container_runtime: containerd
tls:
  # Agents sign in with a cert-manager client certificate instead of
  # registering a machine name, which a restarted Pod would reuse
  # (crowdsecurity/helm-charts#252).
  enabled: true
  certManager:
    # The ca release.
    issuerRef: {name: crowdsec-ca, kind: Issuer}
    # Ten years: CrowdSec agents load their client certificate once
    # and never reload it (crowdsecurity/crowdsec#2810). Shorten this
    # once they do.
    duration: 87600h
    renewBefore: 720h
    secretTemplate:
      # The chart writes an empty secretTemplate without one, which
      # fails schema validation.
      labels: {app.kubernetes.io/part-of: crowdsec}
config:
  # Replaces the chart's default, which turns on token registration.
  # Merged into the image's config.yaml, which keeps its flush max_items
  # and max_age.
  config.yaml.local: |
    db_config:
      flush:
        agents_autodelete:
          # Deletes the machine a restarted agent leaves behind.
          cert: 7d
secrets:
  externalSecret:
    # Inputs, not chart-generated, so every render is identical.
    name: crowdsec-lapi
agent:
  wait_for_lapi:
    image:
      # The agent's image has nc; the chart's busybox has no digest.
      repository: crowdsecurity/crowdsec
      tag: v1.8.2@sha256:65b74942f7e7ed5c60fde4ac15fc4ba715904c398c8b16dd451f4907d90347d4
  acquisition:
    # Traefik's JSON access log.
    - namespace: traefik
      podName: traefik-*
      program: traefik
  env:
    # Parsers and scenarios for Traefik plus generic HTTP attacks.
    - name: COLLECTIONS
      value: crowdsecurity/traefik
lapi:
  env:
    # No data leaves the cluster unless the user opts in.
    - name: DISABLE_ONLINE_API
      value: "true"
    # Registers the Traefik bouncer (input bouncerKey).
    - name: BOUNCER_KEY_traefik
      valueFrom:
        secretKeyRef: {name: crowdsec-bouncer-key, key: key}
  persistentVolume:
    data:
      # SQLite decisions and bouncer registrations.
      enabled: true
    config:
      # Agents sign in with certificates, and a kept credentials file with a login would clash.
      enabled: false
ZEABUR_VALUES
cat > crowdsec-single-node.yaml <<'ZEABUR_VALUES'
agent:
  resources:
    # Chart asks 250Mi; an idle agent uses under 100Mi.
    requests: {cpu: 20m, memory: 48Mi}
    limits: {memory: 192Mi}
lapi:
  resources:
    # Chart asks 500Mi; LAPI on SQLite idles far below.
    requests: {cpu: 20m, memory: 64Mi}
    limits: {memory: 256Mi}
ZEABUR_VALUES
helm upgrade --install crowdsec oci://ghcr.io/crowdsecurity/helm-charts/crowdsec \
  --version 0.24.2 \
  --namespace crowdsec --create-namespace \
  --values crowdsec-values.yaml \
  --values crowdsec-single-node.yaml

# bouncer-middleware
kubectl create namespace crowdsec --dry-run=client -o yaml | kubectl apply -f -
kubectl -n crowdsec apply --server-side -f - <<'ZEABUR_MANIFESTS'
# Referenced from Traefik's entrypoints as crowdsec-bouncer@kubernetescrd.
apiVersion: traefik.io/v1alpha1
kind: Middleware
metadata:
  name: bouncer
  namespace: crowdsec
spec:
  plugin:
    crowdsec-bouncer:
      enabled: true
      # Local copy of decisions; requests never wait on LAPI.
      crowdsecMode: stream
      crowdsecLapiScheme: https
      crowdsecLapiHost: crowdsec-service.crowdsec.svc.cluster.local:8080
      # The CA from the ca release, which signs the LAPI certificate.
      crowdsecLapiTlsCertificateAuthority: urn:k8s:secret:crowdsec-lapi-tls:ca.crt
      # Read from the Secret in the Middleware's namespace.
      crowdsecLapiKey: urn:k8s:secret:crowdsec-bouncer-key:key
      # Fail open: if LAPI is unreachable, keep serving traffic.
      updateMaxFailure: -1
ZEABUR_MANIFESTS
