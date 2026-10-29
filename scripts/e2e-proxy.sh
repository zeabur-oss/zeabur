#!/usr/bin/env bash
# Zeabur Proxy against the real VictoriaLogs and VictoriaMetrics: a canary written in one namespace
# never reaches a person whose rights are in another. Namespace B's name starts with A's. A reader with `view` in
# A asks the proxy for B's marker in every way below, and none of its answers carries it; a reader
# with `view` in B sees it, so the marker was there to leak. A pod in A that writes JSON naming B
# never shows up in B, and a pod in A reaches neither VictoriaLogs nor the proxy. For metrics, a
# pod in B named after B's marker is the canary: A's reader asks for B's series in every way below,
# and no answer names that pod. The metrics part is skipped when the proxy serves no metrics store.
#
# Needs a cluster where the chart runs with `proxy.enabled` and the catalog's logs capability
# (VictoriaLogs, vlagent, its NetworkPolicy) is installed; KUBECONFIG points at it. Run inside
# `nix develop`. It removes what it made unless ZEABUR_PROXY_E2E_KEEP=1.
#   KUBECONFIG=target/dev-cluster/kubeconfig scripts/e2e-proxy.sh
set -euo pipefail

release_namespace="${ZEABUR_NAMESPACE:-zeabur}"
proxy="${ZEABUR_PROXY_SERVICE:-zeabur-proxy}"
logs_url="${ZEABUR_LOGS_URL:-http://vlsingle-victoria-logs.observability.svc:9428}"
a="proxy-canary"
b="proxy-canary-b"
run=$(od -An -N4 -tx1 /dev/urandom | tr -d ' \n')
marker_a="canary-a-$run"
marker_b="canary-b-$run"
spoof="canary-spoof-$run"
port=18443
host="$proxy.$release_namespace.svc"
work=$(mktemp -d)

for tool in kubectl curl jq; do
  command -v "$tool" >/dev/null || { echo "error: $tool is not on PATH" >&2; exit 1; }
done

forward=""
finish() {
  local status=$?
  [[ -n "$forward" ]] && kill "$forward" 2>/dev/null || true
  if [[ -z "${ZEABUR_PROXY_E2E_KEEP:-}" ]]; then
    kubectl delete namespace "$a" "$b" --wait=false >/dev/null 2>&1 || true
  fi
  rm -rf "$work"
  exit "$status"
}
trap finish EXIT

retry() { # $1 = seconds, $2 = what, $@ = command
  local deadline=$((SECONDS + $1)) what="$2"
  shift 2
  until "$@" >/dev/null 2>&1; do
    if ((SECONDS >= deadline)); then
      echo "error: timed out waiting for $what" >&2
      return 1
    fi
    sleep 2
  done
}

passed=0
pass() { passed=$((passed + 1)); echo "ok   $*"; }
fail() { echo "FAIL $*" >&2; exit 1; }

# --- The namespaces, their writers and their readers ---------------------------------------------
kubectl delete namespace "$a" "$b" --wait >/dev/null 2>&1 || true
kubectl create namespace "$a" >/dev/null
kubectl create namespace "$b" >/dev/null
writer() { # namespace, name, shell loop body
  kubectl -n "$1" run "$2" --image=busybox:1.37 --restart=Never --command -- \
    sh -c "while true; do $3; sleep 2; done" >/dev/null
}
writer "$a" writer "echo $marker_a"
writer "$b" writer "echo $marker_b"
writer "$b" "$marker_b" "true"
# Lines that name B in every shape a collector might take a field from.
kubectl apply -f - >/dev/null <<EOF
apiVersion: v1
kind: Pod
metadata: {name: spoofer, namespace: $a}
spec:
  restartPolicy: Never
  containers:
    - name: c
      image: busybox:1.37
      command: [sh, -c]
      args:
        - |
          while true; do
            echo '{"kubernetes.pod_namespace":"$b","kubernetes.pod_name":"writer","_msg":"$spoof-1"}'
            echo '{"kubernetes":{"pod_namespace":"$b"},"_msg":"$spoof-2"}'
            echo '{"_stream":"{kubernetes.pod_namespace=\\"$b\\"}","_msg":"$spoof-3"}'
            echo '{"message":"$spoof-4","kubernetes.pod_namespace":"$b"}'
            echo 'kubernetes.pod_namespace=$b $spoof-5'
            sleep 2
          done
EOF
for ns in "$a" "$b"; do
  kubectl -n "$ns" create serviceaccount reader >/dev/null
  kubectl -n "$ns" create rolebinding reader --clusterrole=view --serviceaccount="$ns:reader" >/dev/null
done
token_a=$(kubectl -n "$a" create token reader --duration=1h)
token_b=$(kubectl -n "$b" create token reader --duration=1h)

# --- The proxy, over TLS that trusts only its CA ---------------------------------------------------
secret=$(kubectl -n "$release_namespace" get deployment "$proxy" \
  -o jsonpath='{.spec.template.spec.volumes[?(@.name=="tls")].secret.secretName}')
kubectl -n "$release_namespace" get secret "$secret" -o jsonpath='{.data.ca\.crt}' | base64 -d >"$work/ca.crt"
kubectl -n "$release_namespace" port-forward "svc/$proxy" "$port:443" >/dev/null 2>&1 &
forward=$!
retry 30 "the proxy's port-forward" curl -sf --cacert "$work/ca.crt" \
  --resolve "$host:$port:127.0.0.1" "https://$host:$port/healthz"

# Prints the status and keeps it in $work/status, the body in $work/body.
ask() { # token (empty: none), method, path, [curl args]
  local token=$1 method=$2 path=$3
  shift 3
  local auth=()
  [[ -n "$token" ]] && auth=(-H "Authorization: Bearer $token")
  curl -s -o "$work/body" -w '%{http_code}' --max-time 40 --cacert "$work/ca.crt" \
    --resolve "$host:$port:127.0.0.1" -X "$method" "${auth[@]}" "$@" "https://$host:$port$path" \
    >"$work/status" || echo 000 >"$work/status"
  cat "$work/status"
}
query() { # token namespace logsql [extra curl args]
  local token=$1 ns=$2 q=$3
  shift 3
  ask "$token" GET "/logs/select/logsql/query?namespace=$ns&limit=1000" -G \
    --data-urlencode "query=$q" "$@"
}

holds() { # token namespace marker: whether the namespace's history has the marker yet
  [[ $(query "$1" "$2" "$3") == 200 ]] && grep -q "$3" "$work/body"
}
retry 180 "B's marker in VictoriaLogs" holds "$token_b" "$b" "$marker_b"
retry 60 "A's marker in VictoriaLogs" holds "$token_a" "$a" "$marker_a"
pass "B's reader sees B's marker; A's reader sees A's marker"

# --- A's reader asks for B's marker -----------------------------------------------------------------
clean() { # what, expected status pattern
  local status
  status=$(cat "$work/status")
  [[ "$status" =~ ^($2)$ ]] || fail "$1: status $status, wanted $2: $(head -c 300 "$work/body")"
  ! grep -q "$marker_b" "$work/body" || fail "$1: B's marker leaked"
  pass "$1 ($status)"
}
ns_b="{kubernetes.pod_namespace=\"$b\"}"
for q in \
  "$marker_b" \
  "$ns_b" \
  "_stream:$ns_b" \
  "$ns_b $marker_b" \
  "kubernetes.pod_namespace:$b" \
  "kubernetes.pod_namespace:=$b" \
  "kubernetes.pod_namespace:~\"$a.*\"" \
  "_stream:~\"$a\"" \
  "* | union ($ns_b)" \
  "* | join by (kubernetes.container_name) ($ns_b | stats by (kubernetes.container_name) count_uniq(_msg) as n, values(_msg) as leaked)" \
  "_msg:in($ns_b | fields _msg)" \
  "* | stats by (kubernetes.pod_namespace, _msg) count()" \
  "* | filter _msg:in($ns_b | keep _msg)" \
  "{kubernetes.pod_namespace=~\"$a.*\"} OR $ns_b" \
  "* | extract \"<x>\" | union (* | limit 1)"; do
  query "$token_a" "$a" "$q" >/dev/null
  clean "query $q" "200|400|422"
done

refused() { # what, expected status, method, path, [curl args]
  ask "$token_a" "$3" "$4" "${@:5}" >/dev/null
  clean "$1" "$2"
}
refused "B as the namespace" 403 GET "/logs/select/logsql/query?namespace=$b&query=$marker_b"
refused "B's hits" 403 GET "/logs/select/logsql/hits?namespace=$b&query=*"
refused "A and B" 400 GET "/logs/select/logsql/query?namespace=$a&namespace=$b"
refused "B then A" 400 GET "/logs/select/logsql/query?namespace=$b&namespace=$a"
refused "A, encoded B" 403 GET "/logs/select/logsql/query?namespace=proxy-canary%2Db&query=*"
refused "A's name with a space" 400 GET "/logs/select/logsql/query?namespace=$a%20&query=*"
refused "A's name in capitals" 400 GET "/logs/select/logsql/query?namespace=PROXY-CANARY&query=*"
refused "a stream filter of its own" 400 GET \
  "/logs/select/logsql/query?namespace=$a&extra_stream_filters=%7Bkubernetes.pod_namespace%3D%22$b%22%7D"
refused "an extra filter of its own" 400 GET "/logs/select/logsql/query?namespace=$a&extra_filters=*"
refused "a tenant" 400 GET "/logs/select/logsql/query?namespace=$a&AccountID=1"
refused "POST" "400|404|405" POST "/logs/select/logsql/query?namespace=$a&query=*"
refused "a dot segment" "400|404" GET "/logs/select/logsql/query/../hits?namespace=$a" --path-as-is
refused "an encoded dot segment" "400|404" GET "/logs/select/logsql/query/%2e%2e/hits?namespace=$a" --path-as-is
refused "a doubled slash" "400|404" GET "//logs/select/logsql/query?namespace=$a" --path-as-is
refused "tail" "400|404" GET "/logs/select/logsql/tail?namespace=$a&query=*"
refused "field values" "400|404" GET \
  "/logs/select/logsql/field_values?namespace=$a&field=kubernetes.pod_namespace"
refused "the backend's own path" "400|404" GET "/select/logsql/query?query=$marker_b"
refused "another backend" "400|404" GET "/traces/api/v1/query?namespace=$a&query=up"
query "$token_a" "$a" "$marker_b" -H "AccountID: 1" -H "ProjectID: 1" >/dev/null
clean "tenant headers" 200
ask "$token_a" GET "/logs/select/logsql/hits?namespace=$a" -G \
  --data-urlencode "query=$ns_b" >/dev/null
clean "A's hits for B's stream" 200
[[ $(jq -s '[.[].hits[]?.total // 0] | add // 0' "$work/body") == 0 ]] ||
  fail "A's hits counted B's lines: $(cat "$work/body")"
pass "A's hits for B's stream count nothing"

[[ $(ask "" GET "/logs/select/logsql/query?namespace=$a") == 401 ]] || fail "no token: $(cat "$work/status")"
pass "no token (401)"
[[ $(ask "not-a-token" GET "/logs/select/logsql/query?namespace=$a") == 401 ]] ||
  fail "a forged token: $(cat "$work/status")"
pass "a forged token (401)"
[[ $(ask "$token_b" GET "/logs/select/logsql/query?namespace=$a&query=*") == 403 ]] ||
  fail "B's reader in A: $(cat "$work/status")"
pass "B's reader in A (403)"

# --- Metrics: A's reader reads A's usage and none of B's --------------------------------------------
metrics() { # token namespace endpoint [curl args]
  local token=$1 ns=$2 endpoint=$3
  shift 3
  ask "$token" GET "/metrics/api/v1/$endpoint?namespace=$ns" -G "$@"
}
if [[ $(metrics "$token_a" "$a" labels) =~ ^(404|502)$ ]]; then
  echo "skip metrics: the proxy serves no metrics store ($(cat "$work/status"))"
else
  # A query starts once what it reads back from its first point is after the namespace's creation:
  # for a bare selector at a 60 s step, about 8 minutes.
  in_store() { # token namespace marker: whether the namespace's series name the marker yet
    local end
    end=$(date +%s)
    [[ $(metrics "$1" "$2" query_range --data-urlencode "query=container_memory_working_set_bytes" \
      --data-urlencode "start=$((end - 120))" --data-urlencode "end=$end" --data-urlencode "step=60") == 200 ]] &&
      grep -q "$3" "$work/body"
  }
  retry 720 "B's canary pod in the metrics store" in_store "$token_b" "$b" "$marker_b"
  retry 60 "A's writer in the metrics store" in_store "$token_a" "$a" '"writer"'
  pass "B's reader sees B's canary pod; A's reader sees A's writer"
  # The series and label APIs start on the second UTC day after the creation.
  if [[ $(metrics "$token_a" "$a" "label/namespace/values") != 400 ]] || ! grep -q before_namespace "$work/body"; then
    fail "A's namespace values on its first day: $(cat "$work/status") $(head -c 300 "$work/body")"
  fi
  pass "A's label values wait for its second day (400)"
  now=$(date +%s)
  for q in \
    "container_memory_working_set_bytes" \
    "{__name__=~\".+\"}" \
    "{namespace=\"$b\"}" \
    "{namespace=~\"$a.*\"}" \
    "{namespace!=\"$a\"}" \
    "{pod=\"$marker_b\"}" \
    "count by (namespace, pod) ({__name__=~\"container_.+\"})" \
    "sum by (pod) (rate(container_cpu_usage_seconds_total{namespace=\"$b\"}[5m]))" \
    "max_over_time(container_memory_working_set_bytes{namespace=\"$b\"}[1h:1m])" \
    "container_memory_working_set_bytes{namespace=\"$b\"} offset 1m" \
    "container_memory_working_set_bytes{namespace=\"$b\"} or vector(1)" \
    "union(container_memory_working_set_bytes{namespace=\"$b\"}, up)" \
    "WITH (x = {namespace=\"$b\"}) x" \
    "topk(5, container_memory_working_set_bytes) @ end()"; do
    metrics "$token_a" "$a" query --data-urlencode "query=$q" >/dev/null
    clean "metrics query $q" "200|400|422"
    metrics "$token_a" "$a" query_range --data-urlencode "query=$q" \
      --data-urlencode "start=$((now - 3600))" --data-urlencode "end=$now" --data-urlencode "step=60" \
      >/dev/null
    clean "metrics range query $q" "200|400|422"
  done
  for match in "{__name__=~\".+\"}" "{namespace=\"$b\"}" "{pod=\"$marker_b\"}"; do
    metrics "$token_a" "$a" series --data-urlencode "match[]=$match" >/dev/null
    clean "series $match" 400
    metrics "$token_a" "$a" labels --data-urlencode "match[]=$match" >/dev/null
    clean "labels $match" 400
    metrics "$token_a" "$a" "label/pod/values" --data-urlencode "match[]=$match" >/dev/null
    clean "pod values $match" 400
  done
  refused "B's metrics" 403 GET "/metrics/api/v1/query?namespace=$b&query=up"
  refused "B's pod names" 403 GET "/metrics/api/v1/label/pod/values?namespace=$b"
  refused "a label filter of its own" 400 GET \
    "/metrics/api/v1/query?namespace=$a&query=up&extra_label=namespace%3D$b"
  refused "a series filter of its own" 400 GET \
    "/metrics/api/v1/query?namespace=$a&query=up&extra_filters%5B%5D=%7Bnamespace%3D%22$b%22%7D"
  refused "export" "400|404" GET "/metrics/api/v1/export?namespace=$a&match%5B%5D=%7Bnamespace%3D%22$b%22%7D"
  refused "federate" "400|404" GET "/metrics/federate?namespace=$a&match%5B%5D=%7Bnamespace%3D%22$b%22%7D"
  refused "TSDB status" "400|404" GET "/metrics/api/v1/status/tsdb?namespace=$a"
  refused "vmalert's alerts" "400|404" GET "/metrics/api/v1/alerts?namespace=$a"
  refused "a POSTed query" "400|404|405" POST "/metrics/api/v1/query?namespace=$a" \
    --data-urlencode "query={namespace=\"$b\"}"
fi

# --- The spoofer's lines stay in A ------------------------------------------------------------------
sleep 10
query "$token_b" "$b" "$spoof" >/dev/null
[[ $(cat "$work/status") == 200 ]] || fail "B's reader, spoof: $(cat "$work/status")"
! grep -q "$spoof" "$work/body" || fail "a line written in A shows up in B: $(grep "$spoof" "$work/body" | head -3)"
pass "the spoofer's lines do not show up in B"

# --- A pod reaches neither VictoriaLogs nor the proxy -----------------------------------------------
probe() { # namespace, name, labels, url
  kubectl -n "$1" run "$2" --image=curlimages/curl:8.11.1 --restart=Never --labels="$3" \
    --command -- sleep 600 >/dev/null
  kubectl -n "$1" wait --for=condition=Ready "pod/$2" --timeout=120s >/dev/null
  kubectl -n "$1" exec "$2" -- curl -sk -o /dev/null -w '%{http_code}' --max-time 5 "$4" 2>/dev/null || true
}
# The same probe as Zeabur's pod does reach the proxy, so a refusal below is the policy's.
code=$(probe "$release_namespace" proxy-canary-probe \
  "app.kubernetes.io/name=zeabur,app.kubernetes.io/instance=zeabur" "https://$host/healthz")
kubectl -n "$release_namespace" delete pod proxy-canary-probe --now >/dev/null
[[ "$code" == 200 ]] || fail "a pod labelled as Zeabur's did not reach the proxy ($code)"
pass "a pod labelled as Zeabur's reaches the proxy"
for target in "$logs_url/select/logsql/query?query=*&limit=1" "https://$host/healthz"; do
  for labels in "app=user" "app.kubernetes.io/name=zeabur-proxy" "app.kubernetes.io/name=zeabur"; do
    name="probe-$(printf %s "$target$labels" | cksum | cut -d' ' -f1)"
    code=$(probe "$a" "$name" "$labels" "$target")
    [[ "$code" == 000 ]] || fail "a pod in A labelled $labels reached $target ($code)"
    pass "a pod in A labelled $labels does not reach $target"
  done
done

echo "zeabur proxy canary: $passed passed"
