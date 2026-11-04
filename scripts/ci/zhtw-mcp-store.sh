#!/usr/bin/env bash
# Keeps the zhtw-mcp build of the zh dev shell between wording runs: `import` copies a cached
# store into /nix/store, `export` writes zhtw-mcp's closure to it. Building it takes about 4 minutes.
set -euo pipefail
cd "$(dirname "$0")/../.."

store="${ZHTW_MCP_STORE:-$HOME/.cache/zhtw-mcp-store}"
case "${1:-}" in
  import)
    [[ -d "$store" ]] || exit 0
    # Root is a trusted user, so the unsigned local store is accepted.
    sudo "$(command -v nix)" copy --no-check-sigs --from "file://$store" --all
    ;;
  export)
    nix copy --to "file://$store" "$(nix build .#zhtw-mcp --no-link --print-out-paths)"
    ;;
  *)
    echo "usage: $0 import|export" >&2
    exit 2
    ;;
esac
