#!/usr/bin/env bash
# Fails if a workspace crate depends on a workspace crate of the same or a higher level, if a
# crate other than a composer depends on a module crate, or if a pure crate depends on a workspace
# crate that is not pure. Levels are declared in Cargo.toml [workspace.metadata.levels], modules
# and composers in [workspace.metadata.modules], pure crates in [workspace.metadata.pure]; normal
# and build dependencies count, dev-dependencies do not (tests may use test helpers). See
# docs/architecture.md.
set -euo pipefail

errors=$(cargo metadata --format-version 1 --no-deps --locked | jq -r '
  (.metadata.levels // error("Cargo.toml has no [workspace.metadata.levels]")) as $levels
  | (.metadata.modules // error("Cargo.toml has no [workspace.metadata.modules]")) as $modules
  | (.metadata.pure // error("Cargo.toml has no [workspace.metadata.pure]")) as $pure
  | [.packages[].name] as $members
  | (($members - ($levels | keys)) | map("error: \(.) has no level")),
    (.packages[] as $p
     | $p.dependencies[]
     | select(.kind != "dev" and (.name as $n | $members | index($n)))
     | select($levels[.name] >= $levels[$p.name])
     | "error: \($p.name) (level \($levels[$p.name])) depends on \(.name) (level \($levels[.name]))"),
    (.packages[] as $p
     | select(($modules.composers | index($p.name)) == null)
     | select(($modules.crates | index($p.name)) == null)
     | $p.dependencies[]
     | select(.kind != "dev" and (.name as $n | $modules.crates | index($n)))
     | "error: \($p.name) depends on the module crate \(.name); only \($modules.composers | join(", ")) may"),
    (.packages[] as $p
     | select($pure.crates | index($p.name))
     | $p.dependencies[]
     | select(.kind != "dev" and (.name as $n | $members | index($n)))
     | select(.name as $n | ($pure.crates | index($n)) == null)
     | "error: the pure crate \($p.name) depends on \(.name), which is not pure")
  | .[]?, strings' | sort -u)
if [[ -n "$errors" ]]; then
  echo "$errors" >&2
  exit 1
fi
echo "crate levels, module boundaries and pure crates: ok"
