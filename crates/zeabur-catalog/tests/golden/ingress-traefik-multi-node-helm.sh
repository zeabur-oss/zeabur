# On K3s, first turn off the packaged traefik: add it to `disable` in
# /etc/rancher/k3s/config.yaml on every server, then restart k3s.
# On RKE2, set `ingress-controller: none` in /etc/rancher/rke2/config.yaml on
# every server, then restart rke2-server.
# That deletes the Traefik CRDs and any IngressRoute or Middleware objects;
# Ingress traffic stops until this one runs.
# https://docs.k3s.io/installation/packaged-components
# https://docs.rke2.io/advanced
# Profile: multi-node

# gateway-api-crds
if ! kubectl get crd httproutes.gateway.networking.k8s.io >/dev/null 2>&1; then
  kubectl apply --server-side -f https://github.com/kubernetes-sigs/gateway-api/releases/download/v1.6.3/standard-install.yaml
fi

# traefik
cat > traefik-values.yaml <<'ZEABUR_VALUES'
providers:
  kubernetesGateway:
    # Route web services with HTTPRoute.
    enabled: true
  kubernetesIngress:
    # Many charts still ship Ingress objects.
    enabled: true
  kubernetesCRD:
    # Middleware objects, such as the CrowdSec bouncer.
    enabled: true
    # No references to other namespaces.
    allowCrossNamespace: false
    # Only the traefik namespace can set the default TLS store.
    defaultTLSResourcesNamespace: traefik
gatewayClass:
  # Gateways reference GatewayClass "traefik".
  enabled: true
  name: traefik
gateway:
  # Gateways and listeners are created per domain, not by the chart.
  enabled: false
api:
  # Unauthenticated surface; not needed.
  dashboard: false
service:
  spec:
    # Skip source NAT so Traefik sees client addresses (CrowdSec, rate
    # limits, logs); only nodes running Traefik serve traffic.
    externalTrafficPolicy: Local
accessLog:
  # JSON access logs for metrics, log search and CrowdSec.
  enabled: true
  format: json
  fields:
    # Keep only the fields below; secrets live in headers and queries.
    defaultMode: drop
    names:
      RequestHost: keep
      RequestMethod: keep
      RequestPath: keep
      RequestProtocol: keep
      DownstreamStatus: keep
      DownstreamContentSize: keep
      RequestContentSize: keep
      Duration: keep
      RouterName: keep
      StartUTC: keep
    headers:
      # Headers carry cookies and bearer tokens.
      defaultMode: drop
    queryParameters:
      # Query strings carry OAuth codes and signed-URL secrets.
      defaultMode: drop
ZEABUR_VALUES
cat > traefik-multi-node.yaml <<'ZEABUR_VALUES'
deployment:
  # One pod per node, so every node keeps client addresses under the
  # Local traffic policy; k3s ServiceLB source-NATs on nodes without one.
  # From a Deployment, annotate it helm.sh/resource-policy=keep before the
  # upgrade and delete it after, or ingress drops for a few seconds.
  kind: DaemonSet
tolerations:
  # The taints k3s ServiceLB tolerates, so Traefik runs wherever it listens.
  - key: node-role.kubernetes.io/control-plane
    operator: Exists
    effect: NoSchedule
  - key: CriticalAddonsOnly
    operator: Exists
metrics:
  prometheus:
    # Request rate and latency metrics.
    service:
      enabled: true
resources:
  # Headroom for higher request rates.
  requests: {cpu: 100m, memory: 128Mi}
  limits: {memory: 512Mi}
ZEABUR_VALUES
helm upgrade --install traefik oci://ghcr.io/traefik/helm/traefik \
  --version 42.1.3 \
  --namespace traefik --create-namespace \
  --values traefik-values.yaml \
  --values traefik-multi-node.yaml
