#!/usr/bin/env bash
# Lists every shipped dependency that contains `unsafe` code or links native code, and fails if one
# is not approved in supply-chain/unsafe-allowlist.toml. Writes a Markdown report to
# target/unsafe-report.md. Policy: CONTRIBUTING.md, "Dependencies".
#
# The `unsafe` count is the number of non-comment source lines containing the `unsafe` keyword in
# the crate's .rs files. It is a review signal, not an audit. The dependency graph comes from the
# pinned Cargo itself, so the script keeps working when the manifest uses new Cargo features.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
allowlist="$root/supply-chain/unsafe-allowlist.toml"
report="$root/target/unsafe-report.md"
target=x86_64-unknown-linux-musl
mkdir -p "$root/target"

# Prints the approval date of an allowlist entry, or nothing. An entry counts only when `approved`
# is a date; "pending" marks a proposal awaiting a maintainer's review.
approval() { # $1 = crate name, $2 = kind
  awk -v n="$1" -v k="$2" '
    function flush() {
      if (name == n && kind == k && approved ~ /^[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]$/) print approved
    }
    /^\[\[crate\]\]/ { flush(); name = ""; kind = ""; approved = "" }
    /^name *=/ { v = $0; gsub(/.*= *"|"$/, "", v); name = v }
    /^kind *=/ { v = $0; gsub(/.*= *"|"$/, "", v); kind = v }
    /^approved *=/ { v = $0; gsub(/.*= *"|"$/, "", v); approved = v }
    END { flush() }' "$allowlist"
}

# Download sources first; the scan reads them from the Cargo home.
cargo fetch --locked --quiet

# The shipped graph: normal and build dependencies of the two binaries for the image target.
# Dev-dependencies and the test-only crates never reach a binary and are not listed.
shipped=$(cargo tree --locked -p zeabur -p zeaburctl --all-features -e normal,build --target "$target" \
  --prefix none --format '{p}' | sed 's/ (.*//; s/ v/@/' | sort -u)

# Every shipped non-workspace package, with its source directory and whether it declares `links`
# (native code built by a build script).
metadata=$(cargo metadata --format-version 1 --locked --all-features --filter-platform "$target")
packages=$(jq -r --arg shipped "$shipped" '
  ($shipped | split("\n")) as $keep
  | .workspace_members as $ws
  | .packages[]
  | select((.name + "@" + .version) as $p | ($keep | index($p)) != null)
  | select(.id as $id | ($ws | index($id)) == null)
  | [.name, .version, (.manifest_path | sub("/Cargo.toml$"; "")), (if .links then "yes" else "no" end)]
  | @tsv' <<<"$metadata")

status=0
{
  echo "# Dependency unsafe report"
  echo
  echo "| crate | version | lines with \`unsafe\` | native code | approved |"
  echo "|---|---|---|---|---|"
  while IFS=$'\t' read -r name version dir native; do
    [[ -z "$name" ]] && continue
    if [[ ! -d "$dir" ]]; then echo "error: sources of $name $version not found at $dir" >&2; exit 1; fi
    used=$( (grep -rhE --include='*.rs' '(^|[^[:alnum:]_])unsafe([^[:alnum:]_]|$)' "$dir" || true) |
      grep -cvE '^[[:space:]]*(//|\*)' || true)
    if [[ "$used" == 0 && "$native" == no ]]; then continue; fi
    ok=yes
    if [[ "$used" != 0 && -z "$(approval "$name" unsafe)" ]]; then ok=NO; status=1; fi
    if [[ "$native" == yes && -z "$(approval "$name" native)" ]]; then ok=NO; status=1; fi
    echo "| $name | $version | $used | $native | $ok |"
  done <<<"$packages"
} >"$report"

cat "$report"
if [[ $status -ne 0 ]]; then
  echo "error: dependencies above marked NO need a maintainer's approval in $allowlist" >&2
fi
exit $status
