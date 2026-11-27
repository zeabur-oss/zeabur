#!/usr/bin/env bash
# Collect notices from the exact modules linked into the updater image.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
destination="${1:?usage: database-updater-licenses.sh <absolute-directory>}"
mkdir -p "$destination"
install -m 0444 "$root/LICENSE" "$destination/Zeabur-LICENSE"
install -m 0444 "$root/updater/NOTICE.md" "$destination/NOTICE.md"
goroot=$(go env GOROOT)
license="$goroot/LICENSE"
[[ -f "$license" ]] || license="$goroot/../LICENSE"
install -m 0444 "$license" "$destination/Go-LICENSE"
cd "$root/updater"
modules=$(CGO_ENABLED=0 go list -mod=readonly -deps -f '{{with .Module}}{{.Path}}{{"\t"}}{{.Dir}}{{end}}' . | sort -u)
while IFS=$'\t' read -r module directory; do
  [[ -n "$directory" && "$directory" != "$root/updater" ]] || continue
  folder="$destination/$module"
  mkdir -p "$folder"
  find "$directory" -maxdepth 1 -type f \
    \( -iname 'LICENSE*' -o -iname 'COPYING*' -o -iname 'NOTICE*' \) \
    -exec install -m 0444 -t "$folder" {} +
done <<<"$modules"
