# Profile: single-node

kubectl create namespace crowdsec --dry-run=client -o yaml | kubectl apply -f -
# The Traefik bouncer uses this key to call the CrowdSec local API.
kubectl -n crowdsec create secret generic crowdsec-bouncer-key \
  --from-literal=key="$(openssl rand -hex 64)" \
  --dry-run=client -o yaml | kubectl apply -f -
# The local API (LAPI) signs agent sessions with this shared secret.
# Agents register with the local API using this token.
kubectl -n crowdsec create secret generic crowdsec-lapi \
  --from-literal=csLapiSecret="$(openssl rand -hex 64)" \
  --from-literal=registrationToken="$(openssl rand -hex 64)" \
  --dry-run=client -o yaml | kubectl apply -f -

# crowdsec
cat > crowdsec-values.yaml <<'ZEABUR_VALUES'
# The runtime on k3s and mainstream distributions.
container_runtime: containerd
secrets:
  externalSecret:
    # Inputs, not chart-generated, so every render is identical.
    name: crowdsec-lapi
agent:
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
    # Registers the Traefik bouncer (input bouncer-key).
    - name: BOUNCER_KEY_traefik
      valueFrom:
        secretKeyRef: {name: crowdsec-bouncer-key, key: key}
  persistentVolume:
    data:
      # SQLite decisions and bouncer registrations.
      enabled: true
    config:
      # Keeps machine credentials stable across restarts.
      enabled: true
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
      crowdsecLapiScheme: http
      crowdsecLapiHost: crowdsec-service.crowdsec.svc.cluster.local:8080
      # Read from the Secret in the Middleware's namespace.
      crowdsecLapiKey: urn:k8s:secret:crowdsec-bouncer-key:key
      # Fail open: if LAPI is unreachable, keep serving traffic.
      updateMaxFailure: -1
ZEABUR_MANIFESTS
