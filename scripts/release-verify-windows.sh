#!/usr/bin/env bash
# Runs zeaburctl from a draft release on Windows, as a user gets it, and checks its Authenticode
# signature. An unsigned file only warns, as a rehearsal builds unsigned; a signature that does not
# hold fails.
#
#   scripts/release-verify-windows.sh <vX.Y.Z> <downloaded>
#
# <downloaded> holds zeaburctl-<vX.Y.Z>-windows-amd64.zip. Runs in Git Bash, outside the Nix shell.
# scripts/release-verify.sh checks every file's checksum and attestation.
set -euo pipefail

usage="usage: release-verify-windows.sh <vX.Y.Z> <downloaded>"
version=${1:?$usage}
downloaded=$(cd "${2:?$usage}" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

fail() {
  echo "$*" >&2
  exit 1
}

[[ "$(uname -s):$(uname -m)" == MINGW64_NT*:x86_64 ]] || fail "$usage, on Windows amd64"
# Windows' own tar.exe reads a zip; Git Bash's GNU tar does not.
"$(cygpath -u "$SYSTEMROOT")/System32/tar.exe" -xf \
  "$(cygpath -w "$downloaded/zeaburctl-$version-windows-amd64.zip")" -C "$(cygpath -w "$work")"
[[ -f "$work/LICENSE" ]] || fail "the archive has no LICENSE"

said=$("$work/zeaburctl.exe" --version | tr -d '\r')
[[ "$said" == "zeaburctl ${version#v}" ]] || fail "zeaburctl says '$said'"

status=$(powershell.exe -NoProfile -Command \
  "(Get-AuthenticodeSignature -LiteralPath '$(cygpath -w "$work/zeaburctl.exe")').Status" | tr -d '\r')
case "$status" in
  Valid) ;;
  NotSigned) echo "::warning::zeaburctl.exe is not signed" ;;
  *) fail "zeaburctl.exe's signature is $status" ;;
esac
echo "zeaburctl $version verified on windows-amd64"
