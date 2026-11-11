#!/usr/bin/env bash
# Two rules for the web interface. Needs only grep.
# - A query key is built only in web/src/api. Callers use the query options and the helpers
#   (`refresh`, `refreshProject`, `cached`, ...) of web/src/api/queries.ts, so a key changes in one
#   place. Tests may read a key.
# - A cast to `Record<string, never>` hides the real type of what it casts (a manifest goes in a
#   `create` change through `objectBody`). Tests may keep one.
set -euo pipefail

cd "$(dirname "$0")/.."

status=0
sources() {
  grep -rn "$@" --include='*.ts' --include='*.tsx' --exclude='*.test.ts' --exclude='*.test.tsx' \
    web/src || true
}

hits=$(sources -w --exclude-dir=api queryKey)
if [[ -n "$hits" ]]; then
  echo "$hits" >&2
  echo "error: queryKey outside web/src/api; add a query option or a helper there" >&2
  status=1
fi

hits=$(sources -F 'Record<string, never>' --exclude=zeabur.ts)
if [[ -n "$hits" ]]; then
  echo "$hits" >&2
  echo "error: a cast to Record<string, never>; give the value its real type" >&2
  status=1
fi

if [[ $status -eq 0 ]]; then echo "web/src keeps its query keys in web/src/api and casts nothing to never"; fi
exit $status
