#!/usr/bin/env bash
# Copies the CRDs of the Zeabur Builder release in xtask/pins/zeabur-builder to
# crates/zeabur-plan/tests/zeabur-builder-crds.yaml, which the Build that Zeabur plans is checked
# against (crates/zeabur-plan/tests/built_service.rs). Run it after moving the pin.
set -euo pipefail
cd "$(dirname "$0")/.."

pin=$(tr -d '[:space:]' <xtask/pins/zeabur-builder)
out=crates/zeabur-plan/tests/zeabur-builder-crds.yaml
{
  echo "# zeabur-oss/builder $pin deploy/chart/crds/crds.yaml, copied by scripts/builder-crds.sh"
  curl -fsSL "https://raw.githubusercontent.com/zeabur-oss/builder/$pin/deploy/chart/crds/crds.yaml"
} >"$out.tmp"
mv "$out.tmp" "$out"
