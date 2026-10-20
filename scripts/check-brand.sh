#!/usr/bin/env bash
# Fails if a tracked file still names the project by its old names. It is Zeabur (the product),
# Zeabur Identity (sign-in) and Zeabur Catalog (the component catalog).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

# The brackets keep this line from matching itself.
if git grep -n -I -i -E '[e]ngawa|[g]enkan'; then
  echo "error: the project is Zeabur; rename the lines above" >&2
  exit 1
fi
