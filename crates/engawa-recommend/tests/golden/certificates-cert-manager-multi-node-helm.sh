# Profile: multi-node

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
cat > cert-manager-multi-node.yaml <<'ENGAWA_VALUES'
# Two replicas; leader election keeps one active.
replicaCount: 2
podDisruptionBudget:
  # Keep a controller during drains.
  enabled: true
webhook:
  # Gates every cert-manager write; stay available during drains.
  replicaCount: 2
  podDisruptionBudget:
    enabled: true
cainjector:
  # Same reason as the controller.
  replicaCount: 2
  podDisruptionBudget:
    enabled: true
ENGAWA_VALUES
helm upgrade --install cert-manager oci://quay.io/jetstack/charts/cert-manager \
  --version v1.21.2 \
  --namespace cert-manager --create-namespace \
  --values cert-manager-values.yaml \
  --values cert-manager-multi-node.yaml
