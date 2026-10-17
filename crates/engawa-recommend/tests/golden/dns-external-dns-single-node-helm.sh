# Profile: single-node

# external-dns
cat > external-dns-values.yaml <<'ENGAWA_VALUES'
sources:
  # Hostnames from HTTPRoutes and Ingresses.
  - gateway-httproute
  - ingress
# Never delete records others may rely on.
policy: upsert-only
# Touch only records it created.
registry: txt
ENGAWA_VALUES
cat > external-dns-single-node.yaml <<'ENGAWA_VALUES'
resources:
  # A small polling loop; idles under 30Mi.
  requests: {cpu: 10m, memory: 32Mi}
  limits: {memory: 128Mi}
ENGAWA_VALUES
helm upgrade --install external-dns external-dns \
  --repo https://kubernetes-sigs.github.io/external-dns \
  --version 1.23.0 \
  --namespace external-dns --create-namespace \
  --values external-dns-values.yaml \
  --values external-dns-single-node.yaml \
  --set provider.name='<provider>' \
  --set domainFilters='<domain-filters>' \
  --set txtOwnerId='<owner-id>'
