#!/usr/bin/env bash
# Lines added and removed against a base, for each area of the repository, as a Markdown table.
#   scripts/diff-size.sh [<base>]        # default origin/main, compared from the merge base
# An area is a crate (`crates/<name>`), or the first directory of a path (`web`, `docs`, `scripts`,
# ...), or `(root)` for a file at the top. Lock files, the generated API types and the OpenAPI
# document are `generated`, so they do not bury what a person wrote. A binary file counts as a file
# with no lines. The first line is a marker, so the workflow can find its comment again.
set -euo pipefail

base="${1:-origin/main}"

echo '<!-- diff-size -->'
echo '### Lines changed by area'
echo
rows=$(git diff --numstat "$base...HEAD" | awk -F'\t' '
  function area(path,   parts, n) {
    if (path ~ /(^|\/)(Cargo\.lock|bun\.lock)$/ || path == "web/src/api/zeabur.ts" \
        || path == "crates/zeabur-api/openapi.yaml") return "generated"
    n = split(path, parts, "/")
    if (n == 1) return "(root)"
    if (parts[1] == "crates" && n > 2) return "crates/" parts[2]
    return parts[1]
  }
  {
    name = area($3)
    plus[name] += ($1 == "-" ? 0 : $1)
    minus[name] += ($2 == "-" ? 0 : $2)
    files[name]++
  }
  END { for (name in files) printf "%s\t%d\t%d\t%d\n", name, files[name], plus[name], minus[name] }' | sort)
if [[ -z "$rows" ]]; then
  echo "No changes."
  exit 0
fi
echo "| Area | Files | Added | Removed |"
echo "|---|---:|---:|---:|"
awk -F'\t' '{ printf "| %s | %d | +%d | -%d |\n", $1, $2, $3, $4; f += $2; p += $3; m += $4 }
  END { printf "| **Total** | %d | +%d | -%d |\n", f, p, m }' <<<"$rows"
