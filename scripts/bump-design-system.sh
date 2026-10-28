#!/usr/bin/env bash
# Moves the design system (zeabur-oss/design-system) that web/ and the installer's interface use
# to a commit of its public repository. The package is checked in as vendor/zeabur-design-system-
# <commit>.tgz, packed from that commit as a registry would ship it (`bun pm pack`, its `files`),
# and both package.json name it, so `bun ci` installs it from the checkout and downloads nothing
# from GitHub: an install that fetched GitHub's tarball failed now and then on CI runners
# (UNABLE_TO_VERIFY_LEAF_SIGNATURE). git pins the archive's content; bun.lock records only its
# path. Run inside `nix develop`.
#   scripts/bump-design-system.sh <commit>
set -euo pipefail

commit="${1:?usage: scripts/bump-design-system.sh <full commit of zeabur-oss/design-system>}"
[[ "$commit" =~ ^[0-9a-f]{40}$ ]] || { echo "expected a full commit hash" >&2; exit 2; }
root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

curl --fail --silent --show-error --location --retry 3 --output "$work/source.tar.gz" \
  "https://codeload.github.com/zeabur-oss/design-system/tar.gz/$commit"
mkdir "$work/source" "$work/pack"
tar -xzf "$work/source.tar.gz" -C "$work/source" --strip-components 1
(cd "$work/source" && bun pm pack --destination "$work/pack" >/dev/null)
packed=("$work"/pack/*.tgz)
[[ ${#packed[@]} == 1 && -f "${packed[0]}" ]] || { echo "bun pm pack made no single archive" >&2; exit 1; }

archive="zeabur-design-system-${commit:0:7}.tgz"
mkdir -p "$root/vendor"
git -C "$root" rm --quiet --ignore-unmatch 'vendor/zeabur-design-system-*.tgz'
rm -f "$root"/vendor/zeabur-design-system-*.tgz
install -m 0644 "${packed[0]}" "$root/vendor/$archive"

for web in web crates/zeabur-installer/web; do
  relative="$(realpath --relative-to="$root/$web" "$root/vendor/$archive")"
  (
    cd "$root/$web"
    VALUE="file:$relative" jq --indent 2 '.dependencies["@zeabur/design-system"] = env.VALUE' \
      package.json >"$work/package.json"
    cat "$work/package.json" >package.json
    bun install
  )
done
echo "vendor/$archive: commit, with web/ and crates/zeabur-installer/web package.json and bun.lock"
