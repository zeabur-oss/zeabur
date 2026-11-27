#!/usr/bin/env bash
# Prints every image a chart runs, one per line, sorted.
#
#   scripts/chart-images.sh <chart directory or .tgz>
set -euo pipefail

chart=${1:?usage: chart-images.sh <chart>}

# Turn on every optional workload, so that no image is left out. Zeabur Identity's chart pins
# its own image.
values=(
  --set global.domain=example.com
  --set identity.enabled=false
  --set issuer.url=https://identity.example.com
  --set issuer.clientSecret.existingSecret=zeabur-oidc
  --set proxy.enabled=true
  --set databaseUpdater.enabled=true
)

helm template zeabur "$chart" "${values[@]}" |
  yq --no-doc '.. | select(tag == "!!map") | .image | select(tag == "!!str")' |
  sort -u
