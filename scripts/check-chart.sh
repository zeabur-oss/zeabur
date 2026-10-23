#!/usr/bin/env bash
# Checks the Helm chart without a cluster: `helm lint`, then what the chart promises in every
# rendering (no rights for Zeabur itself, no service account token, one replica, a locked-down container),
# and that values.schema.json refuses values that would install a broken Zeabur.
# Run inside `nix develop`.
#   scripts/check-chart.sh
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
chart="$root/deploy/chart/zeabur"
failures=0

fail() {
  echo "error: $*" >&2
  failures=$((failures + 1))
}

render() { # $@ = extra helm arguments
  helm template zeabur "$chart" --namespace zeabur "$@"
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

  # Zeabur has no rights of its own: no binding to a ServiceAccount, no ClusterRoleBinding, and no
  # token in the pod. The one Role the chart may render lets people read Zeabur's settings: `get`
  # on the ConfigMap zeabur-settings, nothing else. The ClusterRoles are the member roles, which
  # Zeabur binds per member, the rule sets the project roles aggregate, and Zeabur Proxy's logs
  # reader.
  expect "$name" "$out" '[select(.kind == "ClusterRoleBinding")] | length' 0
  expect "$name" "$out" '[select(.kind == "ClusterRole") | .metadata.name] | sort | join(",")' \
    zeabur-gateway-routes-reader,zeabur-member,zeabur-project-builds-read,zeabur-project-builds-write,zeabur-project-developer,zeabur-project-developer-rules,zeabur-project-viewer,zeabur-project-viewer-rules,zeabur-proxy-logs-reader
  # A project role is exactly what its own label selects: nothing else of the chart's widens it.
  expect "$name" "$out" \
    '[select(.kind == "ClusterRole" and .aggregationRule) | .metadata.name + "=" + (.aggregationRule.clusterRoleSelectors[].matchLabels | keys | join("+")) + (.rules | length | tostring)] | join(",")' \
    "zeabur-project-developer=zeabur.com/aggregate-to-zeabur-project-developer0,zeabur-project-viewer=zeabur.com/aggregate-to-zeabur-project-viewer0"
  expect "$name" "$out" \
    "[select(.kind == \"ClusterRole\") | .metadata.name as \$n | .metadata.labels | keys | .[] | select(test(\"aggregate-to\")) | \$n + \">\" + sub(\".*aggregate-to-\"; \"\")] | join(\",\")" \
    "zeabur-project-builds-read>zeabur-project-developer,zeabur-project-builds-read>zeabur-project-viewer,zeabur-project-builds-write>zeabur-project-developer,zeabur-project-developer-rules>zeabur-project-developer,zeabur-project-viewer-rules>zeabur-project-viewer,zeabur-proxy-logs-reader>zeabur-project-developer,zeabur-proxy-logs-reader>view,zeabur-gateway-routes-reader>view"
  expect "$name" "$out" \
    'select(.kind == "ClusterRole" and .metadata.name == "zeabur-gateway-routes-reader") | .rules[] | [.apiGroups, .resources, .verbs] | map(join(",")) | join("|")' \
    'gateway.networking.k8s.io|httproutes|get,list,watch'
  expect "$name" "$out" \
    'select(.kind == "ClusterRole" and .metadata.name == "zeabur-proxy-logs-reader") | .rules[] | [.apiGroups, .resources, .verbs] | map(join(",")) | join("|")' \
    'proxy.zeabur.com|logs|get'
  # Viewers read no logs: neither the pods' nor the log history's.
  expect "$name" "$out" \
    '[select(.kind == "ClusterRole" and .metadata.name == "zeabur-project-viewer-rules") | .rules[].resources[] | select(. == "pods/log")] | length' 0
  roles='select(.kind == "ClusterRole") | .rules[]'
  # Every rule names its groups, resources and verbs.
  expect "$name" "$out" "[$roles | (.apiGroups + .resources + .verbs)[] | select(test(\"^[*]$\"))] | length" 0
  # No RBAC objects, no kubelet, no widening of one's own rights, no namespace made or deleted.
  expect "$name" "$out" "[$roles | select(.apiGroups[] == \"rbac.authorization.k8s.io\")] | length" 0
  expect "$name" "$out" "[$roles | select(.resources[] == \"nodes/proxy\")] | length" 0
  expect "$name" "$out" "[$roles | .verbs[] | select(. == \"escalate\" or . == \"bind\" or . == \"impersonate\")] | length" 0
  # The one namespace write is the Developer's patch of their project's Namespace, which the
  # admission policy keeps to its details.
  expect "$name" "$out" \
    "[select(.kind == \"ClusterRole\") | .metadata.name as \$n | .rules[] | select(.resources[] == \"namespaces\") | .verbs[] | select(. != \"get\" and . != \"list\") | \$n + \" \" + .] | join(\",\")" \
    "zeabur-project-developer-rules patch"
  vap='select(.kind == "ValidatingAdmissionPolicy")'
  expect "$name" "$out" "$vap | [.metadata.name, .spec.failurePolicy, (.spec.matchConstraints.resourceRules[] | .resources + .operations | join(\"/\"))] | join(\",\")" \
    "zeabur-project-namespace,Fail,namespaces/UPDATE"
  expect "$name" "$out" 'select(.kind == "ValidatingAdmissionPolicyBinding") | [.spec.policyName, (.spec.validationActions | join("/"))] | join(",")' \
    "zeabur-project-namespace,Deny"
  # Secret values and shells in containers belong to developers only.
  expect "$name" "$out" \
    '[select(.kind == "ClusterRole") | select(.rules | any_c(.resources | any_c(test("^(secrets|pods/(exec|attach|portforward|ephemeralcontainers))$")))) | .metadata.name] | join(",")' \
    zeabur-project-developer-rules
  expect "$name" "$out" '[select(.kind == "RoleBinding") | .subjects[] | select(.kind == "ServiceAccount")] | length' 0
  expect "$name" "$out" \
    'select(.kind == "Role") | .rules[] | [.apiGroups, .resources, .resourceNames, .verbs] | map(join(",")) | join("|")' \
    '|configmaps|zeabur-settings|get'
  expect "$name" "$out" '[select(.kind == "ServiceAccount") | .automountServiceAccountToken == false] | all' true
  expect "$name" "$out" '[select(.kind == "Deployment") | .spec.template.spec.automountServiceAccountToken == false] | all' true
  dep='select(.kind == "Deployment" and .metadata.name == "zeabur")'
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
  # Every variable the chart sets is one Zeabur reads (crates/zeabur/src/config.rs).
  for env in $(query "$out" "$container | .env[].name"); do
    grep -q "env = \"$env\"" "$root/crates/zeabur/src/config.rs" ||
      fail "$name: Zeabur does not read $env"
  done
  # ZEABUR_ELEVATION is parsed as a bool: exactly true or false.
  expect "$name" "$out" "$container | .env[] | select(.name == \"ZEABUR_ELEVATION\") | .value | test(\"^(true|false)$\")" true
  expect "$name" "$out" "$container | .env[] | select(.name == \"ZEABUR_MEMBER_ROLES\") | .value | test(\"^(true|false)$\")" true
done

# The plan key: generated when no Secret is named, else the named one is mounted.
out=$(render -f "$chart/ci/minimal-values.yaml")
# 32 bytes, which is 44 characters of Base64 with one `=` of padding.
expect minimal "$out" 'select(.kind == "Secret") | .data["plan-key"] | test("^[A-Za-z0-9+/]{43}=$")' true
out=$(render -f "$chart/ci/full-values.yaml")
expect full "$out" '[select(.kind == "Secret")] | length' 0
expect full "$out" 'select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec.volumes[] | select(.name == "plan-key") | .secret.secretName' \
  zeabur-plan-key

# The image: the published one at the chart's appVersion, or exactly the digest a release pins
# (scripts/release-publish.sh).
image='select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec.containers[0].image'
app_version=$(yq '.appVersion' "$chart/Chart.yaml")
expect minimal "$(render -f "$chart/ci/minimal-values.yaml")" "$image" "ghcr.io/zeabur/zeabur:$app_version"
digest="sha256:$(printf '%064d' 0)"
expect digest "$(render -f "$chart/ci/minimal-values.yaml" --set image.tag=ignored --set image.digest="$digest")" \
  "$image" "ghcr.io/zeabur/zeabur@$digest"

# Everyone signed in reads the settings unless that is turned off.
expect minimal "$(render -f "$chart/ci/minimal-values.yaml")" \
  'select(.kind == "RoleBinding") | .subjects[] | .kind + "/" + .name' Group/system:authenticated
expect "everyoneReads=false" "$(render -f "$chart/ci/minimal-values.yaml" --set metricsStore.everyoneReads=false)" \
  '[select(.kind == "Role" or .kind == "RoleBinding")] | length' 0

# Without the member roles there is no ClusterRole, and Zeabur offers only admin or no rights.
out=$(render -f "$chart/ci/minimal-values.yaml" --set memberRoles.enabled=false)
expect "memberRoles=false" "$out" '[select(.kind == "ClusterRole" or .kind == "ValidatingAdmissionPolicy" or .kind == "ValidatingAdmissionPolicyBinding")] | length' 0
expect "memberRoles=false" "$out" \
  'select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec.containers[0].env[] | select(.name == "ZEABUR_MEMBER_ROLES") | .value' false

# The terminal settings reach Zeabur as its flags; empty images stay Zeabur's pinned defaults.
terminal_env='select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec.containers[0].env[] | select(.name | test("^ZEABUR_(DEBUG|TOOLS)_")) | .name + "=" + .value'
expect minimal "$(render -f "$chart/ci/minimal-values.yaml")" "[$terminal_env] | join(\" \")" \
  "ZEABUR_TOOLS_DEADLINE_MINUTES=30 ZEABUR_TOOLS_GRACE_SECONDS=60 ZEABUR_TOOLS_REQUESTS=cpu=10m,memory=64Mi ZEABUR_TOOLS_LIMITS=cpu=1,memory=512Mi"
expect full "$(render -f "$chart/ci/full-values.yaml")" "[$terminal_env] | join(\" \")" \
  "ZEABUR_DEBUG_IMAGE=registry.example.test/busybox:1.37.0 ZEABUR_TOOLS_IMAGE=registry.example.test/k8s:1.36.5 ZEABUR_TOOLS_NAMESPACE=zeabur-tools ZEABUR_TOOLS_DEADLINE_MINUTES=15 ZEABUR_TOOLS_GRACE_SECONDS=30 ZEABUR_TOOLS_REQUESTS=cpu=20m,memory=96Mi ZEABUR_TOOLS_LIMITS=cpu=500m,memory=256Mi"

# The cluster's name reaches Zeabur only when one is given.
cluster_env='[select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec.containers[0].env[] | select(.name == "ZEABUR_CLUSTER_NAME") | .value] | join(",")'
expect minimal "$(render -f "$chart/ci/minimal-values.yaml")" "$cluster_env" ""
expect full "$(render -f "$chart/ci/full-values.yaml")" "$cluster_env" shop-prod

# New public URLs: a suffix and a ClusterIssuer reach Zeabur only when given.
domain_env='[select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec.containers[0].env[] | select(.name == "ZEABUR_DOMAIN_SUFFIX" or .name == "ZEABUR_CLUSTER_ISSUER") | .value] | join(",")'
expect minimal "$(render -f "$chart/ci/minimal-values.yaml")" "$domain_env" ""
expect full "$(render -f "$chart/ci/full-values.yaml")" "$domain_env" apps.example.test,letsencrypt

# Zeabur Proxy: off unless asked for; on, a locked-down pod with no token that only Zeabur reaches,
# over TLS whose CA (and only the CA) Zeabur trusts.
expect minimal "$(render -f "$chart/ci/minimal-values.yaml")" \
  '[select(.metadata.name | test("proxy"))] | map(.kind) | join(",")' ClusterRole
proxy_check() { # $1 = values name, $2 = rendered YAML
  local pod='select(.kind == "Deployment" and .metadata.name == "zeabur-proxy") | .spec.template.spec'
  local container="$pod | .containers[0]"
  local zeabur='select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec'
  expect "$1" "$2" "$pod | .automountServiceAccountToken" false
  expect "$1" "$2" "$pod | .serviceAccountName" zeabur-proxy
  expect "$1" "$2" "$pod | .securityContext.seccompProfile.type" RuntimeDefault
  expect "$1" "$2" "$container | [.securityContext.runAsNonRoot, .securityContext.readOnlyRootFilesystem, .securityContext.allowPrivilegeEscalation] | map(tostring) | join(\",\")" \
    true,true,false
  expect "$1" "$2" "$container | .securityContext.capabilities.drop | join(\",\")" ALL
  expect "$1" "$2" "$container | .resources.limits.memory" 64Mi
  expect "$1" "$2" "$container | [.livenessProbe.httpGet.scheme, .readinessProbe.httpGet.scheme] | join(\",\")" HTTPS,HTTPS
  for env in $(query "$2" "$container | .env[].name"); do
    grep -q "env = \"$env\"" "$root/crates/zeabur-proxy/src/main.rs" ||
      fail "$1: Zeabur Proxy does not read $env"
  done
  expect "$1" "$2" "$zeabur | .volumes[] | select(.name == \"proxy-ca\") | .secret.items | map(.key) | join(\",\")" ca.crt
  expect "$1" "$2" "$zeabur | .containers[0].env[] | select(.name == \"ZEABUR_PROXY_URL\") | .value" \
    https://zeabur-proxy.zeabur.svc
  local policy='select(.kind == "NetworkPolicy" and .metadata.name == "zeabur-proxy") | .spec'
  expect "$1" "$2" "$policy | .policyTypes | join(\",\")" Ingress,Egress
  expect "$1" "$2" "$policy | .ingress[] | .from[] | .podSelector.matchLabels | to_entries | map(.key + \"=\" + .value) | join(\",\")" \
    app.kubernetes.io/name=zeabur,app.kubernetes.io/instance=zeabur
  expect "$1" "$2" "$policy | .egress | length" 3
}
proxy_check proxy "$(render -f "$chart/ci/minimal-values.yaml" --set proxy.enabled=true)"
expect proxy "$(render -f "$chart/ci/minimal-values.yaml" --set proxy.enabled=true)" \
  'select(.kind == "Secret" and .metadata.name == "zeabur-proxy-tls") | .data | keys | join(",")' ca.crt,tls.crt,tls.key
proxy_check full "$(render -f "$chart/ci/full-values.yaml")"
expect full "$(render -f "$chart/ci/full-values.yaml")" \
  'select(.kind == "NetworkPolicy") | .spec.egress[2].to[].ipBlock.cidr' 192.0.2.10/32

# Values that must be refused before anything is installed.
refuse() { # $1 = why, $@ = helm arguments
  local why="$1"
  shift
  if render -f "$chart/ci/minimal-values.yaml" "$@" >/dev/null 2>&1; then
    fail "accepted $why"
  fi
}
refuse "a plain http public URL" --set publicUrl=http://zeabur.example.test
refuse "a public URL with a path" --set publicUrl=https://zeabur.example.test/ui
refuse "an issuer with a trailing slash" --set issuer.url=https://identity.example.test/
refuse "no client secret" --set issuer.clientSecret.existingSecret=
refuse "an HTTPRoute with no parent" --set httpRoute.enabled=true
refuse "a metrics store without a port" --set metricsStore.service=observability/vm
refuse "an unknown value" --set replicas=2
refuse "a cluster name longer than 63 characters" --set clusterName="$(printf 'x%.0s' {1..64})"
refuse "a cluster name on two lines" --set-json 'clusterName="a\nb"'
refuse "a domain suffix that is not a domain" --set domains.suffix=Apps_Example
refuse "a domain suffix with a wildcard" --set 'domains.suffix=*.apps.example.test'
refuse "a ClusterIssuer that is not a name" --set 'domains.clusterIssuer=Lets Encrypt'
refuse "a tools pod without a deadline" --set terminal.toolsDeadlineMinutes=0
refuse "a tools namespace that is not a name" --set terminal.toolsNamespace=Tools
refuse "a tools resource Zeabur does not set" --set terminal.toolsResources.limits.gpu=1
refuse "an image digest without its algorithm" --set image.digest="$(printf '%064d' 0)"
refuse "a logs URL with https or a path" --set proxy.logs.url=https://vl.example.test:9428/select
refuse "an apiServer address that is not a CIDR" --set 'proxy.apiServer[0]=kubernetes.default'
if render >/dev/null 2>&1; then fail "accepted no values at all"; fi

if ((failures > 0)); then
  echo "$failures chart check(s) failed" >&2
  exit 1
fi
echo "chart checks passed"
