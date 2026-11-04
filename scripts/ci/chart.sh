#!/usr/bin/env bash
# The Helm chart: lint, what every rendering promises, and the values it refuses.
# shellcheck source-path=SCRIPTDIR
source "$(dirname "$0")/lib.sh"

./scripts/check-chart.sh
