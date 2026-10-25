# On K3s, first turn off the packaged metrics-server: add it to `disable` in
# /etc/rancher/k3s/config.yaml on every server, then restart k3s.
# On RKE2, add rke2-metrics-server to `disable` in /etc/rancher/rke2/config.yaml on
# every server, then restart rke2-server.
# The metrics API is unavailable until this one runs.
# https://docs.k3s.io/installation/packaged-components
# https://docs.rke2.io/advanced
# Profile: single-node

# metrics-server
cat > metrics-server-values.yaml <<'ZEABUR_VALUES'
# Pinned by digest; a tag alone can move.
image:
  repository: registry.k8s.io/metrics-server/metrics-server
  tag: v0.9.0@sha256:d9862115e7c7881280d3d75ca26bda8ffc0fc213315979575bf23ce9826205c0
args:
  # 15s is fresh enough for live views; the default is 60s.
  - --metric-resolution=15s
ZEABUR_VALUES
cat > metrics-server-single-node.yaml <<'ZEABUR_VALUES'
resources:
  # The chart's 200Mi request is sized for 100+ nodes.
  requests: {cpu: 20m, memory: 48Mi}
  limits: {memory: 128Mi}
ZEABUR_VALUES
helm upgrade --install metrics-server metrics-server \
  --repo https://kubernetes-sigs.github.io/metrics-server \
  --version 3.14.0 \
  --namespace kube-system --create-namespace \
  --values metrics-server-values.yaml \
  --values metrics-server-single-node.yaml
