#!/usr/bin/env bash
# Checks the Helm chart without a cluster: `helm lint`, then what the chart promises in every
# rendering (no rights for engawa itself, no service account token, one replica, a locked-down container),
# and that values.schema.json refuses values that would install a broken engawa.
# Run inside `nix develop`.
#   scripts/check-chart.sh
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
chart="$root/deploy/chart/engawa"
failures=0

fail() {
  echo "error: $*" >&2
  failures=$((failures + 1))
}

render() { # $@ = extra helm arguments
  helm template engawa "$chart" --namespace engawa "$@"
}

# Prints the value of a yq expression over every rendered document, one per line.
query() { # $1 = rendered YAML, $2 = yq expression
  yq eval-all "$2" <<<"$1"
}

expect() { # $1 = values name, $2 = rendered YAML, $3 = yq expression, $4 = expected output
  local got
  got=$(query "$2" "$3")
  [[ "$got" == "$4" ]] || fail "$1: $3 is '$got', expected '$4'"
}

for values in "$chart"/ci/*-values.yaml; do
  name=$(basename "$values" -values.yaml)
  helm lint --strict --quiet "$chart" -f "$values" >/dev/null || fail "$name: helm lint failed"
  out=$(render -f "$values")

  # engawa has no rights of its own: no ClusterRole, no binding to a ServiceAccount, and no token
  # in the pod. The one Role the chart may render lets people read engawa's settings: `get` on
  # the ConfigMap engawa-settings, nothing else.
  expect "$name" "$out" '[select(.kind == "ClusterRole" or .kind == "ClusterRoleBinding")] | length' 0
  expect "$name" "$out" '[select(.kind == "RoleBinding") | .subjects[] | select(.kind == "ServiceAccount")] | length' 0
  expect "$name" "$out" \
    'select(.kind == "Role") | .rules[] | [.apiGroups, .resources, .resourceNames, .verbs] | map(join(",")) | join("|")' \
    '|configmaps|engawa-settings|get'
  expect "$name" "$out" 'select(.kind == "ServiceAccount") | .automountServiceAccountToken' false
  dep='select(.kind == "Deployment")'
  pod="$dep | .spec.template.spec"
  container="$pod | .containers[0]"
  expect "$name" "$out" "$pod | .automountServiceAccountToken" false
  expect "$name" "$out" "[$pod | .volumes[].projected.sources[]?.serviceAccountToken] | length" 0
  # Sessions are in memory: exactly one replica, never two side by side.
  expect "$name" "$out" "$dep | .spec.replicas" 1
  expect "$name" "$out" "$dep | .spec.strategy.type" Recreate
  # https://kubernetes.io/docs/concepts/security/pod-security-standards/ (restricted)
  expect "$name" "$out" "$pod | .securityContext.runAsNonRoot" true
  expect "$name" "$out" "$pod | .securityContext.seccompProfile.type" RuntimeDefault
  expect "$name" "$out" "$container | .securityContext.runAsNonRoot" true
  expect "$name" "$out" "$container | .securityContext.readOnlyRootFilesystem" true
  expect "$name" "$out" "$container | .securityContext.allowPrivilegeEscalation" false
  expect "$name" "$out" "$container | .securityContext.capabilities.drop | join(\",\")" ALL
  expect "$name" "$out" "$container | [.livenessProbe.httpGet.path, .readinessProbe.httpGet.path] | join(\",\")" \
    /api/healthz,/api/healthz
  # The secrets reach the process as files, never as environment values.
  expect "$name" "$out" "[$container | .env[] | select(.valueFrom.secretKeyRef)] | length" 0
  # Every variable the chart sets is one engawa reads (crates/engawa/src/config.rs).
  for env in $(query "$out" "$container | .env[].name"); do
    grep -q "env = \"$env\"" "$root/crates/engawa/src/config.rs" ||
      fail "$name: engawa does not read $env"
  done
  # ENGAWA_ELEVATION is parsed as a bool: exactly true or false.
  expect "$name" "$out" "$container | .env[] | select(.name == \"ENGAWA_ELEVATION\") | .value | test(\"^(true|false)$\")" true
done

# The plan key: generated when no Secret is named, else the named one is mounted.
out=$(render -f "$chart/ci/minimal-values.yaml")
# 32 bytes, which is 44 characters of Base64 with one `=` of padding.
expect minimal "$out" 'select(.kind == "Secret") | .data["plan-key"] | test("^[A-Za-z0-9+/]{43}=$")' true
out=$(render -f "$chart/ci/full-values.yaml")
expect full "$out" '[select(.kind == "Secret")] | length' 0
expect full "$out" 'select(.kind == "Deployment") | .spec.template.spec.volumes[] | select(.name == "plan-key") | .secret.secretName' \
  engawa-plan-key

# Everyone signed in reads the settings unless that is turned off.
expect minimal "$(render -f "$chart/ci/minimal-values.yaml")" \
  'select(.kind == "RoleBinding") | .subjects[] | .kind + "/" + .name' Group/system:authenticated
expect "everyoneReads=false" "$(render -f "$chart/ci/minimal-values.yaml" --set metricsStore.everyoneReads=false)" \
  '[select(.kind == "Role" or .kind == "RoleBinding")] | length' 0

# Values that must be refused before anything is installed.
refuse() { # $1 = why, $@ = helm arguments
  local why="$1"
  shift
  if render -f "$chart/ci/minimal-values.yaml" "$@" >/dev/null 2>&1; then
    fail "accepted $why"
  fi
}
refuse "a plain http public URL" --set publicUrl=http://engawa.example.test
refuse "a public URL with a path" --set publicUrl=https://engawa.example.test/ui
refuse "an issuer with a trailing slash" --set issuer.url=https://genkan.example.test/
refuse "no client secret" --set issuer.clientSecret.existingSecret=
refuse "an HTTPRoute with no parent" --set httpRoute.enabled=true
refuse "a metrics store without a port" --set metricsStore.service=observability/vm
refuse "an unknown value" --set replicas=2
if render >/dev/null 2>&1; then fail "accepted no values at all"; fi

if ((failures > 0)); then
  echo "$failures chart check(s) failed" >&2
  exit 1
fi
echo "chart checks passed"
