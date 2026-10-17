# On k3s, first disable the packaged metrics-server: add it to `disable` in
# /etc/rancher/k3s/config.yaml on every server, then restart k3s.
# The metrics API is unavailable until this one runs.
# https://docs.k3s.io/installation/packaged-components
# Profile: single-node

# metrics-server
cat > metrics-server-values.yaml <<'ENGAWA_VALUES'
args:
  # 15s is fresh enough for live views; the default is 60s.
  - --metric-resolution=15s
ENGAWA_VALUES
cat > metrics-server-single-node.yaml <<'ENGAWA_VALUES'
resources:
  # The chart's 200Mi request is sized for 100+ nodes.
  requests: {cpu: 20m, memory: 48Mi}
  limits: {memory: 128Mi}
ENGAWA_VALUES
helm upgrade --install metrics-server metrics-server \
  --repo https://kubernetes-sigs.github.io/metrics-server \
  --version 3.14.0 \
  --namespace kube-system --create-namespace \
  --values metrics-server-values.yaml \
  --values metrics-server-single-node.yaml
