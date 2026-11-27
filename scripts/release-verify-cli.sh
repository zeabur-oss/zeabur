#!/usr/bin/env bash
# Runs zeaburctl from a draft release on Windows or an Intel Mac, as a user gets it, and checks its
# signature: Authenticode on Windows, Developer ID and notarization on macOS. An unsigned file only
# warns, as a rehearsal builds unsigned; a signature that does not hold fails.
#
#   scripts/release-verify-cli.sh <vX.Y.Z> <windows|darwin> amd64 <downloaded>
#
# Outside the Nix shell. scripts/release-verify.sh checks every file's checksum and attestation.
set -euo pipefail

usage="usage: release-verify-cli.sh <vX.Y.Z> <windows|darwin> amd64 <downloaded>"
version=${1:?$usage} os=${2:?$usage} arch=${3:?$usage}
downloaded=$(cd "${4:?$usage}" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

fail() {
  echo "$*" >&2
  exit 1
}

case "$(uname -s):$(uname -m):$os:$arch" in
  MINGW64_NT*:x86_64:windows:amd64)
    # Windows' own tar.exe reads a zip; Git Bash's GNU tar does not.
    "$(cygpath -u "$SYSTEMROOT")/System32/tar.exe" -xf \
      "$(cygpath -w "$downloaded/zeaburctl-$version-windows-amd64.zip")" -C "$(cygpath -w "$work")"
    zeaburctl=$work/zeaburctl.exe
    ;;
  Darwin:x86_64:darwin:amd64)
    tar -xzf "$downloaded/zeaburctl-$version-darwin-amd64.tar.gz" -C "$work"
    zeaburctl=$work/zeaburctl
    ;;
  *) fail "$usage, on that platform" ;;
esac
[[ -f "$work/LICENSE" ]] || fail "the archive has no LICENSE"

said=$("$zeaburctl" --version | tr -d '\r')
[[ "$said" == "zeaburctl ${version#v}" ]] || fail "zeaburctl says '$said'"

if [[ "$os" == windows ]]; then
  status=$(powershell.exe -NoProfile -Command \
    "(Get-AuthenticodeSignature -LiteralPath '$(cygpath -w "$zeaburctl")').Status" | tr -d '\r')
  case "$status" in
    Valid) ;;
    NotSigned) echo "::warning::zeaburctl.exe is not signed" ;;
    *) fail "zeaburctl.exe's signature is $status" ;;
  esac
elif /usr/bin/codesign -dvv "$zeaburctl" 2>&1 | grep -q '^Authority=Developer ID Application: '; then
  assessment=$(/usr/sbin/spctl --assess -vvv --type open --context context:primary-signature \
    "$zeaburctl" 2>&1) || fail "zeaburctl is not notarized: $assessment"
  grep -q 'source=Notarized Developer ID' <<<"$assessment" ||
    fail "zeaburctl is not notarized: $assessment"
else
  echo "::warning::zeaburctl is not signed with a Developer ID"
fi
echo "zeaburctl $version verified on $os-$arch"
