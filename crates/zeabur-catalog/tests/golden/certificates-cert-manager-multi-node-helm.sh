# Profile: multi-node

# cert-manager
cat > cert-manager-values.yaml <<'ZEABUR_VALUES'
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
ZEABUR_VALUES
cat > cert-manager-multi-node.yaml <<'ZEABUR_VALUES'
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
ZEABUR_VALUES
helm upgrade --install cert-manager oci://quay.io/jetstack/charts/cert-manager \
  --version v1.21.2 \
  --namespace cert-manager --create-namespace \
  --values cert-manager-values.yaml \
  --values cert-manager-multi-node.yaml

# issuers
kubectl create namespace cert-manager --dry-run=client -o yaml | kubectl apply -f -
kubectl -n cert-manager apply --server-side -f - <<'ZEABUR_MANIFESTS'
# Try here first; production rate limits are far lower. Staging
# certificates are not trusted by browsers.
apiVersion: cert-manager.io/v1
kind: ClusterIssuer
metadata:
  name: letsencrypt-staging
spec:
  acme:
    server: https://acme-staging-v02.api.letsencrypt.org/directory
    # No email: ACME accounts need no contact, and Let's Encrypt sends
    # no expiry emails since 2025. Setting spec.acme.email later updates
    # the registered account.
    # Account key, created by cert-manager in its own namespace.
    privateKeySecretRef:
      name: letsencrypt-staging-account-key
    solvers:
      # HTTP-01 through Traefik's Ingress support; needs no Gateway, and
      # also answers for names served by Gateway listeners. The name
      # must reach port 80 of the cluster from the internet.
      - http01:
          ingress:
            ingressClassName: traefik
---
# Trusted certificates; same settings as staging.
apiVersion: cert-manager.io/v1
kind: ClusterIssuer
metadata:
  name: letsencrypt
spec:
  acme:
    server: https://acme-v02.api.letsencrypt.org/directory
    privateKeySecretRef:
      name: letsencrypt-account-key
    solvers:
      - http01:
          ingress:
            ingressClassName: traefik
ZEABUR_MANIFESTS
