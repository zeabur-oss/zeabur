#!/usr/bin/env bash
# Fails if the web's generated sentences (web/src/i18n/generated) are out of date with the
# catalogs in crates/zeabur-gaps/messages, or if a sentence zeabur-modules sends, in its code or
# its fixtures, has no words in a catalog. zeabur-gaps' own tests check its keys and templates.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

(cd web && bun scripts/gen-messages.ts --check)

keys="$(
  {
    grep -rhoE 'Message::new\("[^"]+"' crates/zeabur-modules/src | sed -E 's/.*\("//; s/"$//'
    grep -rhoE '"key": *"[^"]+"' crates/zeabur-modules/fixtures | sed -E 's/.*: *"//; s/"$//'
  } | grep -E '^(diagnosis|gap|trace|incident)\.' | sort -u
)"
missing=0
for catalog in crates/zeabur-gaps/messages/{en,zh-TW,zh-CN}.json; do
  while read -r key; do
    if ! jq -e --arg key "$key" '.messages | has($key)' "$catalog" >/dev/null; then
      echo "error: $catalog has no words for $key" >&2
      missing=1
    fi
  done <<<"$keys"
done
exit "$missing"
