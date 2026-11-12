#!/usr/bin/env bash
# Fails if desktop/, a Cargo workspace of its own, drifts from the main workspace's rules: the
# fields of [workspace.package], the lints of [workspace.lints] (desktop/ denies unsafe_code where
# the main workspace forbids it), [profile.release], the modules that may use unsafe, and the
# workspace crates it depends on. See docs/internal/crates.md, "Apps outside the workspace".
set -euo pipefail
cd "$(dirname "$0")/.."

# The only files that may name unsafe_code, each with #![expect(unsafe_code, reason = "...")].
unsafe_modules=(desktop/src/webkit.rs desktop/src/win32.rs)
# The only workspace crates desktop/ may depend on.
crates=(zeabur-local)

toml() {
  yq -p toml -o json "$1" "$2" | jq -S .
}

failed=0
fields='{"edition": .edition, "rust-version": .["rust-version"], "license": .license, "repository": .repository, "publish": .publish}'
if ! diff -u <(toml ".workspace.package | $fields" Cargo.toml) <(toml ".package | $fields" desktop/Cargo.toml) >&2; then
  echo "error: desktop/Cargo.toml [package] differs from [workspace.package] in Cargo.toml" >&2
  failed=1
fi
if ! diff -u <(toml '.workspace.lints | .rust.unsafe_code = "deny"' Cargo.toml) <(toml '.lints' desktop/Cargo.toml) >&2; then
  echo "error: desktop/Cargo.toml [lints] differs from [workspace.lints] in Cargo.toml" >&2
  failed=1
fi
if ! diff -u <(toml ".profile.release" Cargo.toml) <(toml ".profile.release" desktop/Cargo.toml) >&2; then
  echo "error: desktop/Cargo.toml [profile.release] differs from Cargo.toml" >&2
  failed=1
fi

found=$(grep -rl --include='*.rs' unsafe_code desktop/src desktop/build.rs | sort || true)
allowed=$(printf '%s\n' "${unsafe_modules[@]}" | sort)
if [[ "$found" != "$allowed" ]]; then
  diff -u <(echo "$allowed") <(echo "$found") >&2 || true
  echo "error: only ${unsafe_modules[*]} may name unsafe_code (scripts/check-desktop.sh)" >&2
  failed=1
fi

paths=$(cargo metadata --manifest-path desktop/Cargo.toml --format-version 1 --no-deps --locked |
  jq -r '.packages[].dependencies[] | select(.path != null) | .name')
for name in $paths; do
  if [[ " ${crates[*]} " != *" $name "* ]]; then
    echo "error: desktop/ depends on the workspace crate $name; it may depend only on ${crates[*]}" >&2
    failed=1
  fi
done

if [[ $failed -ne 0 ]]; then
  exit 1
fi
echo "desktop package fields, lints, release profile, unsafe modules and workspace crates: ok"
