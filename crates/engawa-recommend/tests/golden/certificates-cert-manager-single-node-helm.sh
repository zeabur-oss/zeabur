# Profile: single-node

# cert-manager
cat > cert-manager-values.yaml <<'ENGAWA_VALUES'
crds:
  # Upgrade CRDs with the chart, whatever the installer.
  enabled: true
  # Keep CRDs on uninstall; deleting them deletes every Certificate.
  keep: true
enableCertificateOwnerRef: true # Delete the TLS Secret with its Certificate.
config:
  # Typed controller config for the setting below.
  apiVersion: controller.config.cert-manager.io/v1alpha1
  kind: ControllerConfiguration
  # Issue certificates for Gateway listeners.
  enableGatewayAPI: true
ENGAWA_VALUES
cat > cert-manager-single-node.yaml <<'ENGAWA_VALUES'
prometheus:
  # Metrics on the pod port; no Prometheus operator assumed.
  enabled: true
  servicemonitor:
    enabled: false
# Idles at 20-40Mi; small requests fit a 2 GB node.
resources:
  requests: {cpu: 10m, memory: 32Mi}
  limits: {memory: 128Mi}
webhook:
  # Same sizing as the controller.
  resources:
    requests: {cpu: 10m, memory: 24Mi}
    limits: {memory: 64Mi}
cainjector:
  # Caches watched Secrets; the limit bounds it.
  resources:
    requests: {cpu: 10m, memory: 32Mi}
    limits: {memory: 128Mi}
startupapicheck:
  # Short-lived post-install Job.
  resources:
    requests: {cpu: 10m, memory: 16Mi}
    limits: {memory: 64Mi}
ENGAWA_VALUES
helm upgrade --install cert-manager oci://quay.io/jetstack/charts/cert-manager \
  --version v1.21.2 \
  --namespace cert-manager --create-namespace \
  --values cert-manager-values.yaml \
  --values cert-manager-single-node.yaml
