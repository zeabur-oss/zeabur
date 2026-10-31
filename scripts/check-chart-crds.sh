#!/usr/bin/env bash
# Checks the `crds/` files chart_crds selects (crates/zeabur-installer-host) against Helm: for
# every Helm release in Zeabur Catalog at the commit in flake.lock, with the values the catalogue
# renders it with, the objects `helm template --include-crds` adds to `helm template` must be
# the objects chart_crds returns. Needs network access to the chart registries.
#
#   nix develop -c scripts/check-chart-crds.sh
set -euo pipefail

: "${ZEABUR_CATALOG:?run inside nix develop}"
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

(cd "$root" && cargo build -q -p zeabur-installer-host --example chart_crds)
ours=$root/target/debug/examples/chart_crds

src=$work/catalog
cp -R "$ZEABUR_CATALOG" "$src"
chmod -R u+w "$src"

# A helm in front of the catalogue's own: it keeps each `template` call's arguments and values
# files, which the catalogue deletes after the call, and runs the real one.
calls=$work/calls
shim=$work/shim
mkdir -p "$calls" "$shim"
cat >"$shim/helm" <<'SHIM'
#!/usr/bin/env bash
if [[ ${1:-} == template ]]; then
  dir=$(mktemp -d "$CALLS/call.XXXXXX")
  args=()
  i=0
  while (($#)); do
    case $1 in
    -f | --values)
      i=$((i + 1))
      cp "$2" "$dir/values-$i.yaml"
      args+=("$1" "$dir/values-$i.yaml")
      shift 2
      ;;
    *)
      args+=("$1")
      shift
      ;;
    esac
  done
  printf '%s\0' "${args[@]}" >"$dir/args"
  set -- "${args[@]}"
fi
exec "$REAL_HELM" "$@"
SHIM
chmod +x "$shim/helm"

# Its exit status also covers kubeconform, which is not this check's concern.
# shellcheck disable=SC2016 # expanded by the inner shell
(cd "$src" && CALLS=$calls SHIM=$shim nix develop -c bash -c \
  'REAL_HELM=$(command -v helm) PATH=$SHIM:$PATH exec go run ./tools/cmd/catalog render -out "$0"' \
  "$work/render") >"$work/render.log" 2>&1 || tail -n 5 "$work/render.log" >&2

# Kind, namespace and name of each object in a YAML stream, sorted.
ids() { yq -N -r '. | select(.kind != null) | .kind + "/" + (.metadata.namespace // "") + "/" + .metadata.name' | sort -u; }

checked=0
failed=0
declare -A seen
for dir in "$calls"/call.*; do
  [[ -f $dir/args ]] || continue
  mapfile -d '' args <"$dir/args"
  chart=${args[2]}
  with=()
  files=()
  for ((i = 0; i < ${#args[@]}; i++)); do
    [[ ${args[i]} == --include-crds ]] && continue
    with+=("${args[i]}")
    [[ ${args[i]} == -f ]] && files+=("${args[i + 1]}")
  done
  # The same chart and values are rendered for each Kubernetes version and profile.
  key=$chart$(cat "${files[@]:-/dev/null}" | cksum)
  [[ -n ${seen[$key]:-} ]] && continue
  seen[$key]=1
  helm_ids=$(comm -13 \
    <(helm "${with[@]}" | ids) \
    <(helm "${with[@]}" --include-crds | ids))
  if ((${#files[@]})); then
    # shellcheck disable=SC2016 # yq variable, not shell
    yq eval-all -o=json '. as $i ireduce ({}; . * $i)' "${files[@]}" >"$dir/values.json"
  else
    echo '{}' >"$dir/values.json"
  fi
  our_ids=$("$ours" "$chart" "$dir/values.json" | ids)
  checked=$((checked + 1))
  if [[ $helm_ids != "$our_ids" ]]; then
    failed=$((failed + 1))
    echo "MISMATCH $(basename "$chart")" >&2
    diff <(echo "$helm_ids") <(echo "$our_ids") >&2 || true
  else
    echo "ok $(basename "$chart") ($(grep -c . <<<"$our_ids") objects)"
  fi
done
echo "$checked charts checked, $failed differ"
((checked > 0 && failed == 0))
