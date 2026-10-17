#!/usr/bin/env bash
# Builds the binaries without the web interface (`--no-default-features`) with bun removed from
# PATH, proving the API-only build needs Cargo alone. Run inside `nix develop`.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
kept=()
IFS=: read -r -a entries <<<"$PATH"
for entry in "${entries[@]}"; do
  [[ -x "$entry/bun" ]] || kept+=("$entry")
done
PATH="$(IFS=:; echo "${kept[*]}")"
export PATH
if command -v bun >/dev/null; then
  echo "error: bun is still on PATH at $(command -v bun)" >&2
  exit 1
fi
unset ENGAWA_UI_DIST
cargo build --release --locked --no-default-features -p engawa -p engawactl --manifest-path "$root/Cargo.toml"
echo "built without the web interface and without bun"
