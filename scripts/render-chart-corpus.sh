#!/usr/bin/env bash
# Renders the chart corpus in fixtures/charts/ from Zeabur Catalog
# (github.com/zeabur/catalog): every component's single-node profile, rendered by
# the catalogue's own `recs render` (helm template with the release values, the profile, CI
# test inputs and contributions; `urls` releases downloaded, `manifests` releases as written),
# for the newest Kubernetes version in its config/kubernetes.yaml. CustomResourceDefinitions
# and empty documents are removed; everything else is kept verbatim.
#
# Needs Nix with flakes and network access to the chart registries.
#
#   RECS_DIR=~/recs RECS_COMMIT=a0e1cb1d0f5e857459351133d7922cd057a2d19d \
#     scripts/render-chart-corpus.sh [OUT_DIR]
#
# RECS_DIR is a git clone of the catalogue (the pinned commit is exported from it) or an
# export of exactly RECS_COMMIT (`git archive RECS_COMMIT | tar -x -C RECS_DIR`). OUT_DIR
# defaults to fixtures/charts next to this script. The catalogue is rendered twice and the
# script fails if the two renders differ, so the corpus contains no random values.
set -euo pipefail

: "${RECS_DIR:?set RECS_DIR to a Zeabur Catalog checkout}"
: "${RECS_COMMIT:?set RECS_COMMIT to the pinned commit}"
PROFILE=single-node
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
out_dir=${1:-$script_dir/../fixtures/charts}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# The catalogue at the pinned commit.
src=$work/recs
mkdir -p "$src"
if git -C "$RECS_DIR" rev-parse --git-dir >/dev/null 2>&1; then
  git -C "$RECS_DIR" archive "$RECS_COMMIT" | tar -x -C "$src"
else
  echo "note: $RECS_DIR is not a git repository; assuming it is an export of $RECS_COMMIT" >&2
  cp -R "$RECS_DIR"/. "$src"
fi

# recs render: every component, both profiles, every Kubernetes version, twice. Its exit
# status also covers kubeconform, which is not this corpus's concern, so it is reported and
# missing files are caught below instead.
for run in 1 2; do
  if ! (cd "$src" && nix develop -c go run ./tools/cmd/catalog render -out "$work/render-$run") \
    >"$work/render-$run.log" 2>&1; then
    echo "warning: recs render run $run exited non-zero; see the tail of its log:" >&2
    tail -n 20 "$work/render-$run.log" >&2
  fi
done

yq() { (cd "$src" && nix develop -c yq "$@"); }
kube_version=$(yq -r '.versions[-1]' config/kubernetes.yaml)

if ! diff -r "$work/render-1/$kube_version/$PROFILE" "$work/render-2/$kube_version/$PROFILE" >&2; then
  echo "error: two renders differ; the output contains random or time-dependent values" >&2
  exit 1
fi

# Removes CustomResourceDefinition documents and documents with nothing but blank lines and
# comments; prints every other document verbatim after a `---` line.
filter() {
  awk '
    function flush() {
      if (doc != "" && !crd && content) printf "---\n%s", doc
      doc = ""; crd = 0; content = 0
    }
    # A new input file starts a new document, even if the previous file did not end with `---`.
    FNR == 1 && NR > 1 { flush() }
    /^---([ \t].*)?$/ { flush(); next }
    {
      doc = doc $0 "\n"
      if ($0 ~ /^kind:[ \t]*"?CustomResourceDefinition"?[ \t]*$/) crd = 1
      if ($0 !~ /^[ \t]*(#.*)?$/) content = 1
    }
    END { flush() }
  ' "$@"
}

# The licence of each component's sources, as read in its chart's or manifests' repository (the
# LICENSE file GitHub reports for it). A component without an entry fails the run, so a new one
# is looked up before it joins the corpus. Releases written inline in the catalogue are the
# catalogue's (Apache-2.0).
licence() {
  local gh=https://github.com recs="[Apache-2.0](https://github.com/zeabur/catalog/blob/main/LICENSE) (catalogue)"
  case $1 in
  certificates/cert-manager) echo "[Apache-2.0]($gh/cert-manager/cert-manager/blob/master/LICENSE)" ;;
  config-reload/reloader) echo "[Apache-2.0]($gh/stakater/Reloader/blob/master/LICENSE)" ;;
  container-registry/zot)
    echo "tls: $recs; zot: [Apache-2.0]($gh/project-zot/helm-charts/blob/main/LICENSE)"
    ;;
  databases/cloudnative-pg) echo "[Apache-2.0]($gh/cloudnative-pg/charts/blob/main/LICENSE)" ;;
  databases/dragonfly-operator)
    echo "[Apache-2.0]($gh/dragonflydb/dragonfly-operator/blob/main/LICENSE)"
    ;;
  databases/mariadb-operator)
    echo "[MIT]($gh/mariadb-operator/mariadb-operator/blob/main/LICENSE), notice in \`LICENSE-mariadb-operator.txt\`"
    ;;
  databases/moco) echo "[Apache-2.0]($gh/cybozu-go/moco/blob/main/LICENSE)" ;;
  databases/percona-mongodb) echo "[Apache-2.0]($gh/percona/percona-helm-charts/blob/main/LICENSE)" ;;
  dns/external-dns) echo "[Apache-2.0]($gh/kubernetes-sigs/external-dns/blob/master/LICENSE.md)" ;;
  event-autoscaling/keda | event-autoscaling/keda-http-add-on)
    echo "[Apache-2.0]($gh/kedacore/charts/blob/main/LICENSE)"
    ;;
  gotify-notifications/alertmanager-gotify-bridge) echo "$recs" ;;
  image-updates/keel) echo "[MPL-2.0]($gh/keel-hq/keel/blob/master/LICENSE)" ;;
  ingress/traefik)
    echo "gateway-api-crds: [Apache-2.0]($gh/kubernetes-sigs/gateway-api/blob/main/LICENSE); traefik: [Apache-2.0]($gh/traefik/traefik-helm-chart/blob/master/LICENSE)"
    ;;
  intrusion-prevention/crowdsec)
    echo "crowdsec: [MIT]($gh/crowdsecurity/helm-charts/blob/main/LICENSE), notice in \`LICENSE-crowdsec-helm-charts.txt\`; bouncer-middleware: $recs"
    ;;
  logs/victoria-logs) echo "$recs" ;;
  metrics/metrics-server) echo "[Apache-2.0]($gh/kubernetes-sigs/metrics-server/blob/master/LICENSE)" ;;
  metrics/victoria-metrics | observability-operator/victoria-metrics-operator)
    echo "[Apache-2.0]($gh/VictoriaMetrics/helm-charts/blob/master/LICENSE)"
    ;;
  node-updates/system-upgrade-controller)
    echo "[Apache-2.0]($gh/rancher/system-upgrade-controller/blob/master/LICENSE)"
    ;;
  object-storage/seaweedfs-operator)
    echo "[Apache-2.0]($gh/seaweedfs/seaweedfs-operator/blob/master/LICENSE)"
    ;;
  remote-access/tailscale-operator) echo "[BSD-3-Clause]($gh/tailscale/tailscale/blob/main/LICENSE)" ;;
  replicated-storage/longhorn)
    echo "longhorn: [Apache-2.0]($gh/longhorn/charts/blob/v1.13.x/LICENSE); ui-policy: $recs"
    ;;
  resource-recommendations/vertical-pod-autoscaler)
    echo "[Apache-2.0]($gh/kubernetes/autoscaler/blob/master/LICENSE)"
    ;;
  runtime-security/falco) echo "[Apache-2.0]($gh/falcosecurity/charts/blob/master/LICENSE)" ;;
  uptime-probes/blackbox-exporter)
    echo "[Apache-2.0]($gh/prometheus-community/helm-charts/blob/main/LICENSE)"
    ;;
  volume-backup/k8up) echo "[Apache-2.0]($gh/k8up-io/k8up/blob/master/LICENSE)" ;;
  volume-backup/velero) echo "[Apache-2.0]($gh/vmware-tanzu/helm-charts/blob/main/LICENSE)" ;;
  vulnerability-scanning/trivy-operator)
    echo "[Apache-2.0]($gh/aquasecurity/trivy-operator/blob/main/LICENSE), NOTICE in \`LICENSE-trivy-operator-NOTICE.txt\`"
    ;;
  *) return 1 ;;
  esac
}

# Keeps the licence notices (LICENSE-*.txt), which are written by hand, not rendered.
mkdir -p "$out_dir"
rm -f "$out_dir"/*.yaml "$out_dir/README.md"
rows=$work/rows
failed=()
: >"$rows"
for path in "$src"/capabilities/*/*.yaml; do
  capability=$(basename "$(dirname "$path")")
  component=$(basename "$path" .yaml)
  rel=capabilities/$capability/$component.yaml
  if [[ $(yq -r ".defaults.\"$PROFILE\"" "$rel") == unsupported ]]; then
    continue
  fi
  file=${capability}__${component}.yaml
  rendered=()
  missing=0
  while IFS= read -r release; do
    f=$work/render-1/$kube_version/$PROFILE/$capability/$component/$release.yaml
    if [[ -f $f ]]; then rendered+=("$f"); else missing=1; fi
  done < <(yq -r '.releases[].name' "$rel")
  if ((missing)); then
    failed+=("$capability/$component")
    continue
  fi
  if ! licence_cell=$(licence "$capability/$component"); then
    failed+=("$capability/$component (no licence entry)")
    continue
  fi
  filter "${rendered[@]}" >"$out_dir/$file"
  versions=$(yq -r '[.releases[] | .name + "@" + (.version // "inline")] | join(", ")' "$rel")
  docs=$(grep -c '^---$' "$out_dir/$file" || true)
  bytes=$(wc -c <"$out_dir/$file")
  printf '| %s/%s | %s | %s | %s | %s | %s |\n' "$capability" "$component" "\`$file\`" \
    "$versions" "$licence_cell" "$docs" "$bytes" >>"$rows"
done

total=$(cat "$out_dir"/*.yaml | wc -c)
{
  cat <<EOF
# Chart corpus

Rendered Kubernetes manifests of real Helm charts, used to measure how much of real-world
input the projection covers. One file per component of the
[Zeabur Catalog](https://github.com/zeabur/catalog), named
\`<capability>__<component>.yaml\`, each holding every release of the component in install
order as one multi-document YAML stream.

- Source: \`zeabur/catalog\` at commit \`$RECS_COMMIT\`.
- Profile: \`$PROFILE\`, Kubernetes version \`$kube_version\` (the newest in its
  \`config/kubernetes.yaml\`).
- Rendered by the catalogue's own \`recs render\`: \`helm template --include-crds\` with the
  release's values, the profile, the components' CI test inputs (\`testValue\`/\`default\`) and
  values contributed by other components (CrowdSec's Traefik middleware). \`urls\` releases are
  the downloaded upstream manifests; \`manifests\` releases are the inline objects.
- CustomResourceDefinition documents and empty documents are removed. Everything else is
  verbatim, including Helm's \`# Source:\` comments.
- Deterministic: the catalogue is rendered twice and the script fails if the renders differ.
  The catalogue forbids generated secrets and certificates in charts (\`recs render\` checks
  it too), so no value had to be replaced.

Each file keeps the licence of the chart or manifests it was rendered from, named in the table
with the licence file of their repository. The MIT notices of CrowdSec's charts and the MariaDB
operator are in \`LICENSE-crowdsec-helm-charts.txt\` and \`LICENSE-mariadb-operator.txt\`,
Trivy Operator's NOTICE (Apache-2.0, section 4(d)) in \`LICENSE-trivy-operator-NOTICE.txt\`;
the Apache-2.0 text is in the \`LICENSE\` at the root.

Regenerate with Nix and network access to the chart registries:

\`\`\`sh
git clone https://github.com/zeabur/catalog ~/recs
RECS_DIR=~/recs RECS_COMMIT=$RECS_COMMIT scripts/render-chart-corpus.sh
\`\`\`

| Component | File | Releases (name@version) | Licence | Documents | Bytes |
| --- | --- | --- | --- | --- | --- |
EOF
  cat "$rows"
  echo
  echo "Total: $total bytes."
} >"$out_dir/README.md"

echo "rendered $(wc -l <"$rows") components into $out_dir ($total bytes)"
if ((${#failed[@]})); then
  echo "error: no output for: ${failed[*]}" >&2
  exit 1
fi
