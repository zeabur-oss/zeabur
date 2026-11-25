#!/usr/bin/env bash
# Fails on patterns this repository replaced. Each rule has a violating example, which must match
# its regex, and a reason that names the right way. Use this table only for what types and clippy
# (clippy.toml) cannot catch.
set -euo pipefail
cd "$(dirname "$0")/../.."

failures=0

# $1 = id, $2 = rg globs (space-separated), $3 = regex, $4 = a violating line, $5 = the right way
rule() {
  local id=$1 regex=$3 example=$4 reason=$5 globs=() glob list
  read -r -a list <<<"$2"
  for glob in "${list[@]}"; do globs+=(-g "$glob"); done
  if ! rg -q -e "$regex" <<<"$example"; then
    echo "patterns.sh: rule $id does not match its own example" >&2
    failures=$((failures + 1))
  fi
  if rg -n -e "$regex" "${globs[@]}" -g '!scripts/ci/patterns.sh' .; then
    echo "^ $id: $reason" >&2
    failures=$((failures + 1))
  fi
}

rule text-scraping 'scripts/**' '\b(grep|sed|awk|cut)\b' \
  "version=\$(helm show chart deploy/chart/zeabur | sed -n 's/^version: //p')" \
  'read structured output with jq, yq or kubectl -o jsonpath, search text with rg, and move a long script with branches to a Rust test or Bun'
rule upload-secret-ca 'crates/zeabur-api/src/**' '\.get\("ca\.crt"\)' \
  '    values.get("ca.crt").cloned()' \
  "an upload Secret holds credentials only; read the build registry's CA that the catalog component names in provides.registry.ca (images.rs)"

registry="k3d 5.9.0 can report success for an import that never ran; push the image to the dev cluster's registry (xtask/src/dev_cluster/images.rs)"
rule image-import-script 'scripts/** .github/**' '^[^#]*(k3d image import|ctr[^|]*images? import)' \
  'k3d image import zeabur:dev -c zeabur-dev' "$registry"
rule image-import-xtask 'xtask/**/*.rs' '"images?",\s*"import"|"ctr"' \
  '.args(["image", "import"])' "$registry"

if ((failures > 0)); then
  exit 1
fi
