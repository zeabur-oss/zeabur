#!/usr/bin/env bash
# Checks the Helm chart without a cluster: `helm lint`, then what the chart promises in every
# rendering (no rights for Zeabur itself beyond pods in the terminal namespace, its token only at a
# path of its own, one replica, a locked-down container),
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

# Zeabur Identity's chart, which deploy/chart/zeabur/Chart.yaml names.
[[ -d "$chart/charts" ]] || helm dependency build "$chart" >/dev/null

# Everything the chart renders, Zeabur Identity's objects and the install-time Jobs included.
render_all() { # $@ = extra helm arguments
  helm template zeabur "$chart" --namespace zeabur "$@"
}

# Zeabur's own objects, without Zeabur Identity's or the install-time Jobs and their rights.
render() { # $@ = extra helm arguments
  render_all "$@" | yq 'select((.metadata.labels."helm.sh/chart" // "zeabur-") | test("^zeabur-"))
    | select(.metadata.annotations."helm.sh/hook" == null)
    | select(.metadata.annotations."argocd.argoproj.io/hook" == null)'
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
  terminals=$(yq '.terminal.namespace // "zeabur-terminals"' "$values")

  # Zeabur's own rights are pods in the terminal namespace only: one RoleBinding to its
  # ServiceAccount, no ClusterRoleBinding, and its token only at a path of its own. The other Roles
  # let people read Zeabur's settings (`get` on the ConfigMap zeabur-settings) and the S3
  # destinations (the ConfigMaps of their namespace). The ClusterRoles are the member roles, which
  # Zeabur binds per member, the rule sets the project roles aggregate, and Zeabur Proxy's logs
  # and metrics readers.
  expect "$name" "$out" '[select(.kind == "ClusterRoleBinding")] | length' 0
  expect "$name" "$out" '[select(.kind == "ClusterRole") | .metadata.name] | sort | join(",")' \
    zeabur-gateway-routes-reader,zeabur-member,zeabur-project-builds-read,zeabur-project-builds-write,zeabur-project-developer,zeabur-project-developer-rules,zeabur-project-viewer,zeabur-project-viewer-rules,zeabur-proxy-logs-reader,zeabur-proxy-metrics-reader
  # A project role is exactly what its own label selects: nothing else of the chart's widens it.
  expect "$name" "$out" \
    '[select(.kind == "ClusterRole" and .aggregationRule) | .metadata.name + "=" + (.aggregationRule.clusterRoleSelectors[].matchLabels | keys | join("+")) + ((.rules // []) | length | tostring)] | join(",")' \
    "zeabur-project-developer=zeabur.com/aggregate-to-zeabur-project-developer0,zeabur-project-viewer=zeabur.com/aggregate-to-zeabur-project-viewer0"
  expect "$name" "$out" \
    "[select(.kind == \"ClusterRole\") | .metadata.name as \$n | .metadata.labels | keys | .[] | select(test(\"aggregate-to\")) | \$n + \">\" + sub(\".*aggregate-to-\"; \"\")] | join(\",\")" \
    "zeabur-project-builds-read>zeabur-project-developer,zeabur-project-builds-read>zeabur-project-viewer,zeabur-project-builds-write>zeabur-project-developer,zeabur-project-developer-rules>zeabur-project-developer,zeabur-project-viewer-rules>zeabur-project-viewer,zeabur-proxy-logs-reader>zeabur-project-developer,zeabur-proxy-logs-reader>view,zeabur-proxy-metrics-reader>zeabur-project-developer,zeabur-proxy-metrics-reader>zeabur-project-viewer,zeabur-proxy-metrics-reader>view,zeabur-gateway-routes-reader>view"
  expect "$name" "$out" \
    'select(.kind == "ClusterRole" and .metadata.name == "zeabur-gateway-routes-reader") | .rules[] | [.apiGroups, .resources, .verbs] | map(join(",")) | join("|")' \
    'gateway.networking.k8s.io|httproutes|get,list,watch'
  expect "$name" "$out" \
    'select(.kind == "ClusterRole" and .metadata.name == "zeabur-proxy-logs-reader") | .rules[] | [.apiGroups, .resources, .verbs] | map(join(",")) | join("|")' \
    'proxy.zeabur.com|logs|get'
  expect "$name" "$out" \
    'select(.kind == "ClusterRole" and .metadata.name == "zeabur-proxy-metrics-reader") | .rules[] | [.apiGroups, .resources, .verbs] | map(join(",")) | join("|")' \
    'proxy.zeabur.com|metrics|get'
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
    "zeabur-project-namespace,Fail,namespaces/UPDATE,zeabur-system-hosts,Fail,ingresses/CREATE/UPDATE,httproutes/CREATE/UPDATE"
  expect "$name" "$out" 'select(.kind == "ValidatingAdmissionPolicyBinding") | [.spec.policyName, (.spec.validationActions | join("/")), .spec.paramRef.namespace // ""] | join(",")' \
    "zeabur-project-namespace,Deny,,zeabur-system-hosts,Deny,zeabur,zeabur-system-hosts,Deny,zeabur-identity"
  # Secret values and shells in containers belong to developers only.
  expect "$name" "$out" \
    '[select(.kind == "ClusterRole") | select(.rules | any_c(.resources | any_c(test("^(secrets|pods/(exec|attach|portforward|ephemeralcontainers))$")))) | .metadata.name] | join(",")' \
    zeabur-project-developer-rules
  expect "$name" "$out" \
    "[select(.kind == \"RoleBinding\") | (.metadata.namespace + \"/\" + .metadata.name + \">\" + .roleRef.name) as \$b | .subjects[] | select(.kind == \"ServiceAccount\") | \$b + \">\" + .namespace + \"/\" + .name] | join(\",\")" \
    "zeabur/zeabur-catalog>zeabur-catalog>zeabur/zeabur,$terminals/zeabur-terminals>zeabur-terminals>zeabur/zeabur"
  expect "$name" "$out" \
    "[select(.kind == \"Role\" and .metadata.namespace != \"$terminals\") | .metadata.namespace + \"/\" + .metadata.name + \":\" + (.rules[] | [.apiGroups, .resources, .resourceNames // [], .verbs] | map(join(\",\")) | join(\"|\"))] | sort | join(\" \")" \
    'kube-public/zeabur-info-reader:|configmaps|zeabur-info|get zeabur-s3-destinations/zeabur-s3-destinations-reader:|configmaps||get,list zeabur/zeabur-catalog:|configmaps|zeabur-catalog|get,patch zeabur/zeabur-settings-reader:|configmaps|zeabur-settings|get'
  expect "$name" "$out" \
    "[select(.kind == \"Role\" and .metadata.namespace == \"$terminals\") | .metadata.name + \":\" + (.rules[] | [.apiGroups, .resources, .resourceNames // [], .verbs] | map(join(\",\")) | join(\"|\"))] | join(\" \")" \
    'zeabur-terminals:|pods||create,get,list,delete zeabur-terminals:|pods/exec||create,get'
  expect "$name" "$out" \
    "select(.kind == \"Namespace\" and .metadata.name == \"$terminals\") | .metadata.labels[\"pod-security.kubernetes.io/enforce\"]" \
    restricted
  expect "$name" "$out" \
    'select(.kind == "Namespace" and .metadata.name == "zeabur-api-tokens") | .metadata.labels["pod-security.kubernetes.io/enforce"]' \
    restricted
  expect "$name" "$out" '[select(.kind == "ServiceAccount") | .automountServiceAccountToken == false] | all' true
  expect "$name" "$out" '[select(.kind == "Deployment") | .spec.template.spec.automountServiceAccountToken == false] | all' true
  dep='select(.kind == "Deployment" and .metadata.name == "zeabur")'
  pod="$dep | .spec.template.spec"
  container="$pod | .containers[0]"
  expect "$name" "$out" "$pod | .automountServiceAccountToken" false
  expect "$name" "$out" "[$pod | .volumes[] | select(.projected.sources[]?.serviceAccountToken) | .name] | join(\",\")" terminal-token
  expect "$name" "$out" "$container | .volumeMounts[] | select(.name == \"terminal-token\") | .mountPath" \
    /var/run/zeabur/terminal-token
  expect "$name" "$out" "$container | .env[] | select(.name == \"ZEABUR_TERMINAL_NAMESPACE\") | .value" "$terminals"
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
  # Every variable the chart sets is one Zeabur reads (crates/zeabur/src/config.rs), except the
  # proxy variables, which crates/zeabur-http reads (Proxies::from_env).
  for env in $(query "$out" "$container | .env[].name"); do
    case "$env" in HTTP_PROXY | HTTPS_PROXY | NO_PROXY) continue ;; esac
    rg -qF "env = \"$env\"" "$root/crates/zeabur/src/config.rs" ||
      fail "$name: Zeabur does not read $env"
  done
  # ZEABUR_MEMBER_ROLES is parsed as a bool: exactly true or false.
  expect "$name" "$out" "$container | .env[] | select(.name == \"ZEABUR_MEMBER_ROLES\") | .value | test(\"^(true|false)$\")" true
  expect "$name" "$out" "$container | .env[] | select(.name == \"ZEABUR_UPDATE_CHECK\") | .value | test(\"^(true|false)$\")" true
  expect "$name" "$out" "$container | .env[] | select(.name == \"ZEABUR_TEMPLATE_FETCH\") | .value | test(\"^(true|false)$\")" true
done

# The update check is on unless turned off, and Zeabur knows its own Deployment's name.
update_env='select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec.containers[0].env[] | select(.name == "ZEABUR_UPDATE_CHECK") | .value'
expect minimal "$(render -f "$chart/ci/minimal-values.yaml")" "$update_env" true
expect minimal "$(render -f "$chart/ci/minimal-values.yaml")" \
  'select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec.containers[0].env[] | select(.name == "ZEABUR_CATALOG_URL") | .value' https://oss-catalog.zeabur.com/v1/
expect "updateCheck=false" "$(render -f "$chart/ci/minimal-values.yaml" --set updateCheck.enabled=false)" "$update_env" false

# Template updates are on unless turned off.
fetch_env='select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec.containers[0].env[] | select(.name == "ZEABUR_TEMPLATE_FETCH*") | .name + "=" + .value'
expect minimal "$(render -f "$chart/ci/minimal-values.yaml")" "$fetch_env" ZEABUR_TEMPLATE_FETCH=true
expect "templateFetch=false" "$(render -f "$chart/ci/minimal-values.yaml" --set templateFetch.enabled=false)" "$fetch_env" ZEABUR_TEMPLATE_FETCH=false
expect minimal "$(render -f "$chart/ci/minimal-values.yaml")" \
  'select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec.containers[0].env[] | select(.name == "ZEABUR_DEPLOYMENT") | .value' zeabur

# The plan key: generated when no Secret is named, else the named one is mounted.
out=$(render -f "$chart/ci/minimal-values.yaml")
# 32 bytes, which is 44 characters of Base64 with one `=` of padding.
expect minimal "$out" 'select(.kind == "Secret" and .metadata.name == "zeabur-plan-key") | .data["plan-key"] | test("^[A-Za-z0-9+/]{43}=$")' true
out=$(render -f "$chart/ci/full-values.yaml")
expect full "$out" '[select(.kind == "Secret")] | length' 0
expect full "$out" 'select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec.volumes[] | select(.name == "plan-key") | .secret.secretName' \
  zeabur-plan-key

# The image: the published one at the chart's version, or exactly the digest a release pins
# (the `publish` job of release.yml).
image='select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec.containers[0].image'
chart_version=$(yq '.version' "$chart/Chart.yaml")
expect minimal "$(render -f "$chart/ci/minimal-values.yaml")" "$image" "ghcr.io/zeabur-oss/zeabur:$chart_version"
digest="sha256:$(printf '%064d' 0)"
expect digest "$(render -f "$chart/ci/minimal-values.yaml" --set image.tag=ignored --set image.digest="$digest")" \
  "$image" "ghcr.io/zeabur-oss/zeabur@$digest"
# Include the independently permissioned updater even though it is opt-in.
expected_images="ghcr.io/zeabur-oss/database-updater:$chart_version
ghcr.io/zeabur-oss/proxy:$chart_version
ghcr.io/zeabur-oss/zeabur:$chart_version"
images=$("$root/scripts/chart-images.sh" "$chart")
[[ "$images" == "$expected_images" ]] || fail "the chart runs $images, not the three images built by the release"

# Everyone signed in reads the settings unless that is turned off.
expect minimal "$(render -f "$chart/ci/minimal-values.yaml")" \
  'select(.kind == "RoleBinding" and .metadata.name == "zeabur-settings-reader") | .subjects[] | .kind + "/" + .name' \
  Group/system:authenticated
expect "everyoneReads=false" "$(render -f "$chart/ci/minimal-values.yaml" --set metricsStore.everyoneReads=false)" \
  '[select((.kind == "Role" or .kind == "RoleBinding") and .metadata.name == "zeabur-settings-reader")] | length' 0

# Without the member roles there is no ClusterRole, and Zeabur offers only admin or no rights.
out=$(render -f "$chart/ci/minimal-values.yaml" --set memberRoles.enabled=false)
expect "memberRoles=false" "$out" '[select(.kind == "ClusterRole" or .metadata.name == "zeabur-project-namespace")] | length' 0
expect "memberRoles=false" "$out" \
  'select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec.containers[0].env[] | select(.name == "ZEABUR_MEMBER_ROLES") | .value' false

# Without systemHosts, projects may use any domain.
out=$(render -f "$chart/ci/minimal-values.yaml" --set systemHosts.enabled=false)
expect "systemHosts=false" "$out" '[select(.metadata.name | test("^zeabur-system-hosts"))] | length' 0

# The terminal settings reach Zeabur as its flags; empty images stay Zeabur's pinned defaults.
terminal_env='select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec.containers[0].env[] | select(.name | test("^ZEABUR_(DEBUG|TOOLS)_")) | .name + "=" + .value'
expect minimal "$(render -f "$chart/ci/minimal-values.yaml")" "[$terminal_env] | join(\" \")" \
  "ZEABUR_TOOLS_DEADLINE_MINUTES=30 ZEABUR_TOOLS_GRACE_SECONDS=60 ZEABUR_TOOLS_REQUESTS=cpu=10m,memory=64Mi ZEABUR_TOOLS_LIMITS=cpu=1,memory=512Mi"
expect full "$(render -f "$chart/ci/full-values.yaml")" "[$terminal_env] | join(\" \")" \
  "ZEABUR_DEBUG_IMAGE=registry.example.test/busybox:1.37.0 ZEABUR_TOOLS_IMAGE=registry.example.test/k8s:1.36.5 ZEABUR_TOOLS_DEADLINE_MINUTES=15 ZEABUR_TOOLS_GRACE_SECONDS=30 ZEABUR_TOOLS_REQUESTS=cpu=20m,memory=96Mi ZEABUR_TOOLS_LIMITS=cpu=500m,memory=256Mi"

# The cluster's name reaches Zeabur only when one is given.
cluster_env='[select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec.containers[0].env[] | select(.name == "ZEABUR_CLUSTER_NAME") | .value] | join(",")'
expect minimal "$(render -f "$chart/ci/minimal-values.yaml")" "$cluster_env" ""
expect full "$(render -f "$chart/ci/full-values.yaml")" "$cluster_env" shop-prod

# New public URLs: the suffixes and a ClusterIssuer reach Zeabur only when given.
domain_env='[select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec.containers[0].env[] | select(.name == "ZEABUR_DOMAIN_SUFFIXES" or .name == "ZEABUR_CLUSTER_ISSUER") | .value] | join(",")'
expect minimal "$(render -f "$chart/ci/minimal-values.yaml")" "$domain_env" letsencrypt
expect full "$(render -f "$chart/ci/full-values.yaml")" "$domain_env" "apps.example.test,example.test,letsencrypt"
expect sslip "$(render -f "$chart/ci/minimal-values.yaml" --set global.domain=192.0.2.10.sslip.io)" \
  "$domain_env" "192.0.2.10.sslip.io,letsencrypt"

# A node's address becomes its sslip.io name; an IPv6 address has "-" for ":". `helm template` reads no
# nodes, so a copy of the chart renders the helper for each address.
sslip_chart="$(mktemp -d)/zeabur"
cp -R "$chart" "$sslip_chart"
printf '%s\n' 'names: {{ range list "192.0.2.10" "2001:DB8::10" }}{{ include "zeabur.sslipDomain" . }} {{ end }}' \
  >"$sslip_chart/templates/sslip-names.yaml"
got=$(helm template zeabur "$sslip_chart" -f "$chart/ci/minimal-values.yaml" --show-only templates/sslip-names.yaml | yq .names)
[[ "$got" == "192.0.2.10.sslip.io 2001-db8--10.sslip.io" ]] || fail "sslip.io names: got '$got'"
rm -rf "$(dirname "$sslip_chart")"

# A client secret from a Secret of one's own: Identity gets the given hash, and the chart makes no Secret.
own_client='[(select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec.volumes[] | select(.name == "client-secret") | .secret.secretName), (select(.kind == "OAuthClient") | .spec.secretHash), (select(.kind == "Secret" and (.metadata.name | test("-client$"))) | .metadata.name)] | join(",")'
client_hash="sha256:$(printf '%064d' 0)"
expect "own client secret" "$(render -f "$chart/ci/minimal-values.yaml" --set issuer.clientSecret.existingSecret=zeabur-client \
  --set issuer.clientSecret.hash="$client_hash")" "$own_client" "zeabur-client,$client_hash"

# Extra CAs reach Zeabur only when given, from a ConfigMap or a Secret.
extra_ca='select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec | [(.containers[0].env[] | select(.name == "ZEABUR_EXTRA_CA_FILE") | .value), (.volumes[] | select(.name == "extra-ca") | .secret.secretName // .configMap.name)] | join(",")'
expect minimal "$(render -f "$chart/ci/minimal-values.yaml")" "$extra_ca" ""
expect full "$(render -f "$chart/ci/full-values.yaml")" "$extra_ca" "/etc/zeabur/extra-ca/ca.crt,corp-ca"
expect configMap "$(render -f "$chart/ci/minimal-values.yaml" --set extraTrustedCA.configMap=corp-ca)" \
  "$extra_ca" "/etc/zeabur/extra-ca/ca.crt,corp-ca"

# The outbound proxy reaches Zeabur only when enabled, with the cluster's own names and ranges
# bypassed, and leaves Zeabur Proxy alone.
outbound_env='select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec.containers[0].env | map(select(.name == "HTTP_PROXY" or .name == "HTTPS_PROXY" or .name == "NO_PROXY") | .name + "=" + .value) | join(" ")'
expect minimal "$(render -f "$chart/ci/minimal-values.yaml")" "$outbound_env" ""
expect "outboundProxy off" "$(render -f "$chart/ci/minimal-values.yaml" --set outboundProxy.httpsProxy=http://p.example.test:3128)" "$outbound_env" ""
expect full "$(render -f "$chart/ci/full-values.yaml")" "$outbound_env" \
  "HTTPS_PROXY=http://proxy.example.test:3128 NO_PROXY=localhost,.svc,.cluster.local,10.42.0.0/16,10.43.0.0/16,.corp.example.test"
expect "outboundProxy leaves proxy.*" "$(render -f "$chart/ci/full-values.yaml" --set proxy.enabled=false)" \
  '[select(.metadata.name | test("proxy"))] | map(.kind) | join(",")' ClusterRole,ClusterRole

# Zeabur Proxy: off unless asked for; on, a locked-down pod with no token that only Zeabur reaches,
# over TLS whose CA (and only the CA) Zeabur trusts.
expect minimal "$(render -f "$chart/ci/minimal-values.yaml")" \
  '[select(.metadata.name | test("proxy"))] | map(.kind) | join(",")' ClusterRole,ClusterRole
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
    rg -qF "env = \"$env\"" "$root/crates/zeabur-proxy/src/main.rs" ||
      fail "$1: Zeabur Proxy does not read $env"
  done
  expect "$1" "$2" "$zeabur | .volumes[] | select(.name == \"proxy-ca\") | .secret.items | map(.key) | join(\",\")" ca.crt
  expect "$1" "$2" "$zeabur | .containers[0].env[] | select(.name == \"ZEABUR_PROXY_URL\") | .value" \
    https://zeabur-proxy.zeabur.svc
  local policy='select(.kind == "NetworkPolicy" and .metadata.name == "zeabur-proxy") | .spec'
  expect "$1" "$2" "$policy | .policyTypes | join(\",\")" Ingress,Egress
  expect "$1" "$2" "$policy | .ingress[] | .from[] | .podSelector.matchLabels | to_entries | map(.key + \"=\" + .value) | join(\",\")" \
    app.kubernetes.io/name=zeabur,app.kubernetes.io/instance=zeabur
  expect "$1" "$2" "$policy | .egress | length" 4
  # The metrics store Zeabur reads by default, and only its pods.
  expect "$1" "$2" "$container | .env[] | select(.name == \"ZEABUR_PROXY_METRICS_URL\") | .value" \
    http://vmsingle-vm-victoria-metrics-k8s-stack.observability.svc:8428
  expect "$1" "$2" "$policy | .egress[2] | [.to[0].namespaceSelector.matchLabels[], (.to[0].podSelector.matchLabels | to_entries | map(.key + \"=\" + .value) | join(\"+\")), (.ports[0].port | tostring)] | join(\",\")" \
    observability,app.kubernetes.io/instance=vm-victoria-metrics-k8s-stack+app.kubernetes.io/name=vmsingle,8428
}
proxy_check proxy "$(render -f "$chart/ci/minimal-values.yaml" --set proxy.enabled=true)"
expect proxy "$(render -f "$chart/ci/minimal-values.yaml" --set proxy.enabled=true)" \
  'select(.kind == "Secret" and .metadata.name == "zeabur-proxy-tls") | .data | keys | join(",")' ca.crt,tls.crt,tls.key
proxy_check full "$(render -f "$chart/ci/full-values.yaml")"
expect full "$(render -f "$chart/ci/full-values.yaml")" \
  'select(.kind == "NetworkPolicy") | .spec.egress[3].to[].ipBlock.cidr' 192.0.2.10/32
expect proxy "$(render -f "$chart/ci/minimal-values.yaml" --set proxy.enabled=true --set metricsStore.service=monitoring/prometheus:9090)" \
  'select(.kind == "Deployment" and .metadata.name == "zeabur-proxy") | .spec.template.spec.containers[0].env[] | select(.name == "ZEABUR_PROXY_METRICS_URL") | .value' \
  http://prometheus.monitoring.svc:9090
expect proxy "$(render -f "$chart/ci/minimal-values.yaml" --set proxy.enabled=true --set proxy.metrics.enabled=false)" \
  '[select(.kind == "Deployment" and .metadata.name == "zeabur-proxy") | .spec.template.spec.containers[0].env[] | select(.name == "ZEABUR_PROXY_METRICS_URL")] | length' 0

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
refuse "no domain where the nodes cannot be read" --set global.domain=
refuse "a domain that is not a domain" --set global.domain=Example_Test
refuse "no issuer without Zeabur Identity" --set identity.enabled=false
refuse "no client secret without Zeabur Identity" --set identity.enabled=false \
  --set issuer.url=https://identity.example.test
refuse "a client Secret without its hash" --set issuer.clientSecret.existingSecret=zeabur-client
refuse "a client secret hash that is not sha256" --set issuer.clientSecret.existingSecret=zeabur-client \
  --set issuer.clientSecret.hash="$(printf '%064d' 0)"
refuse "an HTTPRoute with no parent" --set httpRoute.enabled=true
refuse "a metrics store without a port" --set metricsStore.service=observability/vm
refuse "a plain http catalog" --set catalog.url=http://catalog.example.test/v1/
refuse "an unknown value" --set replicas=2
refuse "an outbound proxy with no address" --set outboundProxy.enabled=true
refuse "an outbound proxy over https" --set outboundProxy.enabled=true --set outboundProxy.httpsProxy=https://p.example.test
refuse "an extra CA from both a ConfigMap and a Secret" --set extraTrustedCA.configMap=a --set extraTrustedCA.secret=b
refuse "a cluster name longer than 63 characters" --set clusterName="$(printf 'x%.0s' {1..64})"
refuse "a cluster name on two lines" --set-json 'clusterName="a\nb"'
refuse "a domain suffix that is not a domain" --set 'domains.suffixes={Apps_Example}'
refuse "a domain suffix with a wildcard" --set 'domains.suffixes={*.apps.example.test}'
refuse "a ClusterIssuer that is not a name" --set 'domains.clusterIssuer=Lets Encrypt'
refuse "a tools pod without a deadline" --set terminal.toolsDeadlineMinutes=0
refuse "a terminal namespace that is not a name" --set terminal.namespace=Tools
refuse "a tools resource Zeabur does not set" --set terminal.toolsResources.limits.gpu=1
refuse "an image digest without its algorithm" --set image.digest="$(printf '%064d' 0)"
refuse "a logs URL with https or a path" --set proxy.logs.url=https://vl.example.test:9428/select
refuse "metrics through the proxy from a named port" --set proxy.enabled=true --set metricsStore.service=monitoring/prometheus:web
refuse "an installation ID that is not a UUID" --set telemetry.enabled=true --set telemetry.installationId=machine-1
refuse "an installation ID that is not a UUID" --set telemetry.installationId=machine-1
refuse "an apiServer address that is not a CIDR" --set 'proxy.apiServer[0]=kubernetes.default'
if render >/dev/null 2>&1; then fail "accepted no values at all"; fi

# The experience program is on by default, with an installation ID the chart makes and keeps in a
# ConfigMap; a set ID replaces it, and telemetry.enabled=false sends nothing.
env='select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec.containers[0].env[]'
telemetry_env="[$env | select(.name == \"ZEABUR_TELEMETRY_ENABLED\" or .name == \"ZEABUR_INSTALLATION_ID\")]"
id_map='select(.kind == "ConfigMap" and .metadata.name == "zeabur-installation-id")'
id=0a1b2c3d-0000-4000-8000-000000000000
out=$(render -f "$chart/ci/minimal-values.yaml")
expect telemetry "$out" "$telemetry_env | map(.value // .valueFrom.configMapKeyRef.name + \"/\" + .valueFrom.configMapKeyRef.key) | join(\",\")" \
  "true,zeabur-installation-id/id"
[[ "$(query "$out" "$id_map | .data.id")" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$ ]] ||
  fail "telemetry: the chart's installation ID is not a random UUID"
out=$(render -f "$chart/ci/minimal-values.yaml" --set telemetry.installationId=$id)
expect telemetry "$out" "$telemetry_env | map(.value) | join(\",\")" "true,$id"
expect telemetry "$out" "[$id_map] | length" 0
out=$(render -f "$chart/ci/minimal-values.yaml" --set telemetry.enabled=false)
expect telemetry "$out" "$telemetry_env | length" 0
expect telemetry "$out" "[$id_map] | length" 0

# With Zeabur Identity, the chart registers Zeabur as its client: the redirect URI at the public
# URL, the hash of the client secret Zeabur reads, and the addresses under global.domain.
out=$(render_all -f "$chart/ci/minimal-values.yaml")
client='select(.kind == "OAuthClient")'
expect identity "$out" "$client | [.metadata.namespace, .metadata.name, .spec.type, .spec.redirectUris[0], .spec.homeUrl, (.spec.audiences | join(\",\"))] | join(\" \")" \
  "zeabur-identity zeabur Confidential https://zeabur.example.test/auth/callback https://zeabur.example.test/ kubernetes"
secret=$(query "$out" 'select(.kind == "Secret" and .metadata.name == "zeabur-client") | .data["client-secret"]' | base64 -d)
sum=$(printf %s "$secret" | sha256sum)
[[ "$(query "$out" "$client | .spec.secretHash")" == "sha256:${sum%% *}" ]] ||
  fail "identity: the OAuthClient's secretHash is not the hash of the client secret Zeabur reads"
expect identity "$out" '[select(.kind == "Deployment" and .metadata.name == "zeabur") | .spec.template.spec.containers[0].env[] | select(.name == "ZEABUR_PUBLIC_URL" or .name == "ZEABUR_ISSUER_URL") | .value] | join(",")' \
  "https://zeabur.example.test,https://identity.example.test"
expect identity "$out" '[select(.kind == "Ingress") | .metadata.annotations["cert-manager.io/cluster-issuer"] + " " + .spec.tls[0].hosts[0]] | sort | join(",")' \
  "letsencrypt identity.example.test,letsencrypt zeabur.example.test"
expect full "$(render_all -f "$chart/ci/full-values.yaml")" '[select(.kind == "OAuthClient" or .metadata.namespace == "zeabur-identity")] | length' 0

# The components are installed once, after `helm install`, in order, by a Job that is not a Helm
# hook, so its failure does not fail the release. Its cluster-admin binding is gone when it
# succeeds, and an upgrade renders none of it.
hook='select(.metadata.annotations."helm.sh/hook")'
expect components "$out" "[$hook | .kind + \"/\" + .metadata.name] | sort | join(\",\")" \
  "Job/zeabur-identity-setup,Role/zeabur-identity-setup,RoleBinding/zeabur-identity-setup,ServiceAccount/zeabur-identity-setup"
expect components "$out" "[select(.metadata.name == \"zeabur-components\") | .metadata.annotations.\"argocd.argoproj.io/hook-delete-policy\"] | unique | join(\",\")" \
  BeforeHookCreation,HookSucceeded
expect components "$out" 'select(.kind == "ClusterRoleBinding" and .metadata.name == "zeabur-components") | .roleRef.name' cluster-admin
expect components "$(render_all -f "$chart/ci/full-values.yaml")" \
  'select(.kind == "Job" and .metadata.name == "zeabur-components") | .spec.template.spec | [(.initContainers[], .containers[]) | .args | join(" ")] | join(",")' \
  "components install cert-manager --timeout=4m,components install local-path --timeout=4m --revoke=zeabur-components"
expect components "$out" \
  'select(.kind == "Job" and .metadata.name == "zeabur-components") | .spec.template.metadata.labels."app.kubernetes.io/name"' \
  zeabur-components
for args in "--set components.install=null" "--is-upgrade"; do
  # shellcheck disable=SC2086 # $args is several helm arguments.
  expect "components ($args)" "$(render_all -f "$chart/ci/minimal-values.yaml" $args)" \
    '[select(.metadata.name == "zeabur-components")] | length' 0
done

# Each components container reaches the catalog as Zeabur does: the same extra CAs and proxy.
outbound='.spec.template.spec | (.initContainers // []) + .containers | map([(.env | map(select(.name | test("^(ZEABUR_EXTRA_CA_FILE|HTTPS?_PROXY|NO_PROXY)$")) | .name + "=" + .value) | join(" ")), (.volumeMounts // [] | map(select(.name == "extra-ca") | .mountPath) | join(" "))] | join(" ")) | unique | join(",")'
extra_ca_volume='.spec.template.spec.volumes // [] | map(select(.name == "extra-ca")) | to_json'
for args in "-f $chart/ci/minimal-values.yaml" \
  "-f $chart/ci/full-values.yaml --set outboundProxy.enabled=true --set outboundProxy.httpsProxy=http://proxy.example.test:3128" \
  "-f $chart/ci/minimal-values.yaml --set extraTrustedCA.configMap=corp-ca"; do
  # shellcheck disable=SC2086 # $args is several helm arguments.
  rendered=$(render_all $args)
  for expression in "$outbound" "$extra_ca_volume"; do
    server=$(query "$rendered" "select(.kind == \"Deployment\" and .metadata.name == \"zeabur\") | $expression")
    expect "components ($args)" "$rendered" \
      "select(.kind == \"Job\" and .metadata.name == \"zeabur-components\") | $expression" "$server"
  done
done

# The security contexts, the ServiceAccount's annotations and the priority class: the defaults
# are the locked-down contexts the checks above assert, and each value reaches every pod it names.
sec_args=(-f "$chart/ci/minimal-values.yaml" --set proxy.enabled=true
  --set podSecurityContext.runAsUser=1000 --set podSecurityContext.runAsGroup=null
  --set securityContext.readOnlyRootFilesystem=false
  --set-string 'serviceAccount.annotations.example\.test/role=zeabur'
  --set priorityClassName=system-cluster-critical)
rendered=$(render_all "${sec_args[@]}")
for workload in 'Deployment" and .metadata.name == "zeabur' 'Deployment" and .metadata.name == "zeabur-proxy' 'Job" and .metadata.name == "zeabur-components'; do
  pod_spec="select(.kind == \"$workload\") | .spec.template.spec"
  expect "pod security context" "$rendered" "$pod_spec | .securityContext | [.runAsUser, .runAsGroup, .runAsNonRoot, .fsGroup] | join(\",\")" "1000,,true,65532"
  expect "priorityClassName" "$rendered" "$pod_spec | .priorityClassName" system-cluster-critical
done
for workload in 'Deployment" and .metadata.name == "zeabur' 'Deployment" and .metadata.name == "zeabur-proxy'; do
  expect "container security context" "$rendered" \
    "select(.kind == \"$workload\") | .spec.template.spec.containers[0].securityContext | [.readOnlyRootFilesystem, .allowPrivilegeEscalation, .runAsNonRoot] | join(\",\")" "false,false,true"
done
expect "serviceAccount.annotations" "$rendered" \
  'select(.kind == "ServiceAccount" and .metadata.name == "zeabur") | .metadata.annotations["example.test/role"]' zeabur
expect "serviceAccount.annotations" "$rendered" \
  '[select(.kind == "ServiceAccount" and .metadata.name != "zeabur") | .metadata.annotations["example.test/role"]] | map(select(. != null)) | length' 0
rendered=$(render_all -f "$chart/ci/minimal-values.yaml" --set proxy.enabled=true)
expect "priorityClassName unset" "$rendered" '[select(.kind == "Deployment" or .kind == "Job") | .spec.template.spec.priorityClassName] | map(select(. != null)) | length' 0
expect "serviceAccount.annotations unset" "$rendered" '[select(.kind == "ServiceAccount" and (.metadata.name | test("^zeabur(-proxy)?$"))) | .metadata.annotations] | map(select(. != null)) | length' 0

out=$(render -f "$chart/ci/minimal-values.yaml" --set databaseUpdater.enabled=true \
  --set databaseUpdater.image.repository=example.test/updater --set databaseUpdater.image.digest=sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  --set-json 'nodeSelector={"pool":"platform"}' \
  --set-json 'tolerations=[{"key":"platform","operator":"Exists","effect":"NoSchedule"}]' \
  --set-json 'affinity={"nodeAffinity":{"requiredDuringSchedulingIgnoredDuringExecution":{"nodeSelectorTerms":[{"matchExpressions":[{"key":"kubernetes.io/os","operator":"In","values":["linux"]}]}]}}}')
updater='select(.kind == "Deployment" and .metadata.name == "zeabur-database-updater")'
expect database-updater-placement "$out" "$updater | .spec.template.spec.nodeSelector.pool" platform
expect database-updater-placement "$out" "$updater | .spec.template.spec.tolerations[0] | [.key, .operator, .effect] | join(\",\")" 'platform,Exists,NoSchedule'
expect database-updater-placement "$out" "$updater | .spec.template.spec.affinity.nodeAffinity.requiredDuringSchedulingIgnoredDuringExecution.nodeSelectorTerms[0].matchExpressions[0] | [.key, .operator, .values[0]] | join(\",\")" 'kubernetes.io/os,In,linux'
expect database-updater "$out" "$updater | .spec.template.spec.containers[0].image" \
  example.test/updater@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
expect database-updater "$out" "$updater | [.spec.replicas, .spec.strategy.type, .spec.template.spec.serviceAccountName, .spec.template.spec.automountServiceAccountToken] | join(\",\")" \
  '1,Recreate,zeabur-database-updater,false'
expect database-updater "$out" 'select(.kind == "ConfigMap" and .metadata.name == "zeabur-info") | [.data.databaseUpdaterNamespace, .data.databaseUpdaterDeployment] | join(",")' \
  zeabur,zeabur-database-updater
expect database-updater "$out" 'select(.kind == "ClusterRoleBinding" and .metadata.name == "zeabur-database-updater") | .subjects[].name' zeabur-database-updater
expect database-updater "$out" 'select(.kind == "Role" and .metadata.name == "zeabur-database-updater-reader") | .rules[] | [.apiGroups[], .resources[], .resourceNames[], .verbs[]] | join(",")' \
  apps,deployments,zeabur-database-updater,get
expect database-updater "$out" '[select(.kind == "ClusterRole" and .metadata.name == "zeabur-database-updater") | .rules[] | select(.verbs[] == "patch") | .resources[]] | join(",")' clusters
expect database-updater "$out" '[select(.kind == "ClusterRoleBinding") | .subjects[] | select(.kind == "ServiceAccount" and .name == "zeabur")] | length' 0
expect database-updater-disabled "$(render -f "$chart/ci/minimal-values.yaml")" \
  '[select(.metadata.name == "zeabur-database-updater" or .metadata.name == "zeabur-database-updater-reader")] | length' 0

if ((failures > 0)); then
  echo "$failures chart check(s) failed" >&2
  exit 1
fi
echo "chart checks passed"
