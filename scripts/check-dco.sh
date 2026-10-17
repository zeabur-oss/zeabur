#!/usr/bin/env bash
# Fails if any commit in the range lacks a DCO sign-off that matches its author.
# Usage: scripts/check-dco.sh <base-rev> <head-rev>
set -euo pipefail

base="$1"
head="$2"
status=0
while read -r sha; do
  author="$(git log -1 --format='%an <%ae>' "$sha")"
  if ! git log -1 --format='%B' "$sha" | grep -qxF "Signed-off-by: $author"; then
    echo "error: $sha is missing 'Signed-off-by: $author' (see CONTRIBUTING.md)" >&2
    status=1
  fi
done < <(git rev-list --no-merges "$base..$head")
exit $status
