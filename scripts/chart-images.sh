#!/usr/bin/env bash
# Every image a chart runs with each optional workload on, one per line, sorted: the chart directory
# or a packaged .tgz. Run inside `nix develop`.
#   scripts/chart-images.sh deploy/chart/zeabur
set -euo pipefail

chart=${1:?usage: chart-images.sh <chart>}
helm template zeabur "$chart" --set publicUrl=https://zeabur.example.com \
  --set issuer.url=https://identity.example.com \
  --set issuer.clientSecret.existingSecret=zeabur-oidc --set proxy.enabled=true |
  yq -N '.. | select(tag == "!!map") | .image | select(tag == "!!str")' | sort -u
