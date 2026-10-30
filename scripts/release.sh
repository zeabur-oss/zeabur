#!/usr/bin/env bash
# The release, one stage per call, as zeabur-oss/.github's shared release workflow runs it
# (.github/workflows/release.yml, docs/internal/releasing.md). Each stage enters the Nix dev shell
# itself: the one of the checkout it is called from, so that verify-only checks a tag's release
# with a newer commit's scripts in the tag's dev shell. zeabur-oss-release (zeabur-oss/.github,
# release/) pushes, signs, writes the checksums and notes, and verifies; this script keeps what is
# Zeabur's own.
#
#   scripts/release.sh check   <version>                                the version is the
#                                                                       manifests', with a
#                                                                       CHANGELOG.md section
#   scripts/release.sh build   <version> <os> <arch> <dist>             build one platform and smoke
#                                                                       it (release-build.sh,
#                                                                       release-smoke.sh)
#   scripts/release.sh sign    <version> <os> <arch> <dist>             macOS: zeaburctl signed
#                                                                       with the Developer ID and
#                                                                       notarized (zeabur#87)
#   scripts/release.sh publish <version> <artifacts> <published>        the images and the chart
#                                                                       by digest, signed
#   scripts/release.sh finish  <version> <os> <arch> <artifacts> <dist> the installer, pinned to
#                                                                       what publish pushed
#   scripts/release.sh assets  <version> <artifacts> <release>          the assets, SHA256SUMS
#                                                                       signed, and the notes
#   scripts/release.sh verify  <version> <os> <arch> <downloaded>       the draft as a user gets it
#
# <artifacts> holds the earlier stages' outputs: candidate-<os>-<arch> (build), published (publish)
# and finish-<os>-<arch> (finish). The platforms are linux-amd64, linux-arm64 and darwin-arm64
# (zeaburctl only); each builds natively on its own runner.
set -euo pipefail

scripts="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$scripts/.." && pwd)"

# The marker tells a re-run inside the dev shell apart from a call outside it. The dev shell is
# the checkout's this is called from; a sparse checkout of scripts alone has no flake.
if [[ -z "${ZEABUR_RELEASE_SHELL:-}" ]]; then
  export ZEABUR_RELEASE_SHELL=1
  flake="$root"
  [[ -f "$flake/flake.nix" ]] || flake="$PWD"
  exec nix develop "$flake" --command "$0" "$@"
fi

usage() {
  cat >&2 <<'EOF'
usage: scripts/release.sh check   <vVERSION>
       scripts/release.sh build   <vVERSION> <os> <arch> <dist>
       scripts/release.sh sign    <vVERSION> <os> <arch> <dist>
       scripts/release.sh publish <vVERSION> <artifacts> <published>
       scripts/release.sh finish  <vVERSION> <os> <arch> <artifacts> <dist>
       scripts/release.sh assets  <vVERSION> <artifacts> <release>
       scripts/release.sh verify  <vVERSION> <os> <arch> <downloaded>
EOF
  exit 2
}

stage="${1:-}"
version="${2:-}"
[[ -n "$version" ]] || usage
plain="$(zeabur-oss-release version "$version" | sed -n 's/^plain=//p')" || usage

# The platform of the arguments, which must be the machine's: every build is native.
platform() { # $1 = os, $2 = arch
  local here
  case "$(uname -s):$(uname -m)" in
    Linux:x86_64) here=linux-amd64 ;;
    Linux:aarch64) here=linux-arm64 ;;
    Darwin:arm64) here=darwin-arm64 ;;
    *) here="$(uname -s):$(uname -m)" ;;
  esac
  case "$1-$2" in
    linux-amd64 | linux-arm64 | darwin-arm64) ;;
    *) echo "no release for $1-$2: linux-amd64, linux-arm64 and darwin-arm64" >&2; exit 2 ;;
  esac
  [[ "$1-$2" == "$here" ]] || { echo "this machine is $here, not $1-$2" >&2; exit 2; }
}

# Every release asset; SHA256SUMS lists exactly these.
assets=(digests.txt "zeabur-$plain.tgz")
for build in linux-amd64 linux-arm64 darwin-arm64; do
  assets+=("zeaburctl-$version-$build.tar.gz")
done
for arch in amd64 arm64; do
  assets+=("zeabur-$version-linux-$arch.tar.gz" "zeabur-installer-$version-linux-$arch.tar.gz")
done

# The images the release publishes, ghcr.io/<owner>/<name>, and their label in digests.txt: none
# for Zeabur's, the name for the others. The chart may run no other image of the owner.
images=(zeabur proxy)
label() { if [[ "$1" != zeabur ]]; then echo "$1"; fi; }

# The one reference digests.txt records for a kind: [<label> ]linux/amd64, linux/arm64, index, or
# chart.
recorded() { # $1 = digests.txt, $2 = kind
  local ref
  ref="$(awk -v kind="$2" '{ ref = $NF; $NF = ""; sub(/ +$/, "") } $0 == kind { print ref }' "$1")"
  [[ "$ref" =~ ^[^[:space:]@]+@sha256:[0-9a-f]{64}$ ]] || { echo "$1: no single $2 digest" >&2; return 1; }
  echo "$ref"
}

# Each image's index that digests.txt records, sorted as chart_images prints them.
indexes() { # $1 = digests.txt
  local name label
  for name in "${images[@]}"; do
    label="$(label "$name")"
    recorded "$1" "${label:+$label }index" || return 1
  done | sort -u
}

# Every image the chart runs, with each optional workload on.
chart_images() { # $1 = chart directory, packaged chart or oci:// reference
  helm template zeabur "$1" --set publicUrl=https://zeabur.example.com \
    --set issuer.url=https://identity.example.com --set issuer.clientSecret.existingSecret=zeabur-oidc \
    --set proxy.enabled=true |
    yq eval-all --no-doc '.. | select(tag == "!!map" and has("image")) | .image' | sort -u
}

# KEY=value lines (release-pins.sh, published/outputs.env) into the environment, never evaluated.
export_lines() {
  local key value
  while IFS='=' read -r key value; do
    [[ "$key" =~ ^ZEABUR_PIN_[A-Z0-9_]+$ ]] || { echo "unexpected line: $key" >&2; return 1; }
    export "$key=$value"
  done
}

case "$stage" in
  check)
    [[ $# == 2 ]] || usage
    cd "$root"
    status=0
    # Every crate of the workspace (Cargo.lock included, through --locked), the chart, the web
    # packages and the API document carry the version.
    while read -r name crate; do
      [[ "$crate" == "$plain" ]] || { echo "crate $name is $crate, not $plain" >&2; status=1; }
    done < <(cargo metadata --locked --no-deps --format-version 1 |
      jq -r '.packages[] | "\(.name) \(.version)"')
    chart="$(yq '.version + " " + .appVersion' deploy/chart/zeabur/Chart.yaml)"
    [[ "$chart" == "$plain $plain" ]] ||
      { echo "deploy/chart/zeabur/Chart.yaml has version and appVersion $chart, not $plain" >&2; status=1; }
    for manifest in web/package.json crates/zeabur-installer/web/package.json; do
      [[ "$(jq -r .version "$manifest")" == "$plain" ]] ||
        { echo "$manifest has version $(jq -r .version "$manifest"), not $plain" >&2; status=1; }
    done
    [[ "$(yq .info.version crates/zeabur-api/openapi.yaml)" == "$plain" ]] ||
      { echo "crates/zeabur-api/openapi.yaml is not at $plain" >&2; status=1; }
    zeabur-oss-release notes CHANGELOG.md "$version" >/dev/null || status=1
    rendered="$(chart_images deploy/chart/zeabur)"
    while read -r ref; do
      repository="${ref%@*}"
      repository="${repository%:*}"
      name="${repository#ghcr.io/zeabur-oss/}"
      if [[ "$repository" != "$name" && " ${images[*]} " != *" $name "* ]]; then
        echo "the chart runs $ref, which the release does not publish" >&2
        status=1
      fi
    done <<<"$rendered"
    exit "$status"
    ;;

  build)
    [[ $# == 5 ]] || usage
    os="$3" arch="$4"
    platform "$os" "$arch"
    mkdir -p "$5"
    dist="$(cd "$5" && pwd)"
    cd "$root"
    # A tag's binaries embed the catalog bundle, which must be the one the catalog's release
    # signed, and its installer installs the pinned Zeabur Identity release: both pins must
    # resolve before anything is built. A rehearsal builds whatever the pins name.
    if [[ "${GITHUB_EVENT_NAME:-}" == push ]]; then
      "$scripts/release-pins.sh" "$version" >/dev/null
    fi
    "$scripts/release-build.sh" "$version" "$arch" "$dist"
    if [[ "$os" == linux ]]; then
      # The archive, the chart and the image as built, on the development cluster with Zeabur
      # Identity at ZEABUR_IDENTITY_PINNED (scripts/dev-cluster.sh fetches it).
      "$scripts/release-smoke.sh" "$dist/zeaburctl-$version-linux-$arch.tar.gz" deploy/chart/zeabur \
        "zeabur:release-$arch"
      # A rehearsal publishes nothing, so finish never runs: a development installer, without
      # pins, shows that it builds. It is not kept.
      if [[ "${GITHUB_EVENT_NAME:-}" != push ]]; then
        installer="$(mktemp -d)"
        "$scripts/release-build.sh" "$version" "$arch" "$installer" installer
        rm -rf "$installer"
      fi
    fi
    ls -l "$dist"
    ;;

  sign)
    [[ $# == 5 ]] || usage
    os="$3" arch="$4"
    platform "$os" "$arch"
    [[ "$os" == darwin ]] || { echo "only the macOS build is signed" >&2; exit 2; }
    dist="$(cd "$5" && pwd)"
    # The shared workflow passes the secrets only on main and tags. Without them zeaburctl keeps
    # the linker's ad hoc signature, and the release goes on.
    if [[ -z "${APPLE_CERTIFICATE_P12:-}" ]]; then
      echo "::warning::no Developer ID certificate: zeaburctl is not signed or notarized"
      exit 0
    fi
    : "${APPLE_CERTIFICATE_PASSWORD:?}" "${APPLE_API_KEY_P8:?}" "${APPLE_API_KEY_ID:?}" "${APPLE_API_ISSUER_ID:?}"
    archive="$dist/zeaburctl-$version-$os-$arch.tar.gz"
    work="$(mktemp -d)"
    keychain="$work/signing.keychain-db"
    # Apple's tools, not the Nix shell's: its codesign is sigtool's, and its DEVELOPER_DIR and
    # SDKROOT point xcrun at a bare SDK without notarytool.
    apple() { env -u DEVELOPER_DIR -u SDKROOT "$@"; }
    mapfile -t keychains < <(apple /usr/bin/security list-keychains -d user | sed -e 's/^ *"//' -e 's/"$//')
    restore() {
      apple /usr/bin/security list-keychains -d user -s "${keychains[@]}"
      apple /usr/bin/security delete-keychain "$keychain" 2>/dev/null || true
      rm -rf "$work"
    }
    trap restore EXIT
    mkdir "$work/zeaburctl"
    tar -xzf "$archive" -C "$work/zeaburctl"
    file="$work/zeaburctl/zeaburctl"

    password="$(/usr/bin/uuidgen)"
    apple /usr/bin/security create-keychain -p "$password" "$keychain"
    apple /usr/bin/security set-keychain-settings -lut 3600 "$keychain"
    apple /usr/bin/security unlock-keychain -p "$password" "$keychain"
    apple /usr/bin/security list-keychains -d user -s "$keychain" "${keychains[@]}"
    base64 --decode <<<"$APPLE_CERTIFICATE_P12" >"$work/certificate.p12"
    apple /usr/bin/security import "$work/certificate.p12" -k "$keychain" \
      -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
    apple /usr/bin/security set-key-partition-list -S apple-tool:,apple:,codesign: -s \
      -k "$password" "$keychain" >/dev/null
    identity="$(apple /usr/bin/security find-identity -v -p codesigning "$keychain" |
      awk '/"Developer ID Application: / { print $2; exit }')"
    [[ -n "$identity" ]] || { echo "the certificate is not a Developer ID Application one" >&2; exit 1; }
    # The hardened runtime and a secure timestamp, which notarization requires.
    apple /usr/bin/codesign --force --options runtime --timestamp --keychain "$keychain" \
      --sign "$identity" "$file"
    apple /usr/bin/codesign --verify --strict --verbose=2 "$file"

    # A bare executable cannot carry the ticket; Gatekeeper looks it up online.
    printf '%s\n' "$APPLE_API_KEY_P8" >"$work/key.p8"
    apple /usr/bin/ditto -c -k --keepParent "$file" "$work/zeaburctl.zip"
    result="$(apple /usr/bin/xcrun notarytool submit "$work/zeaburctl.zip" --key "$work/key.p8" \
      --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER_ID" --wait --timeout 30m \
      --output-format json)" || true
    if [[ "$(jq -r '.status // empty' <<<"$result" 2>/dev/null || true)" != Accepted ]]; then
      echo "notarization: $result" >&2
      id="$(jq -r '.id // empty' <<<"$result" 2>/dev/null || true)"
      if [[ -n "$id" ]]; then
        apple /usr/bin/xcrun notarytool log "$id" --key "$work/key.p8" \
          --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER_ID" >&2 || true
      fi
      exit 1
    fi
    "$file" --version

    # The archive again, as release-build.sh makes it.
    SOURCE_DATE_EPOCH="$(git -C "$root" show -s --format=%ct HEAD)"
    tar --format=ustar --sort=name --mtime="@$SOURCE_DATE_EPOCH" \
      --owner=0 --group=0 --numeric-owner -C "$work/zeaburctl" -cf - LICENSE zeaburctl |
      gzip -n -9 >"$archive"
    ;;

  publish)
    [[ $# == 4 ]] || usage
    candidates="$(cd "$3" && pwd)"
    mkdir -p "$4"
    out="$(cd "$4" && pwd)"
    : "${GITHUB_REPOSITORY:?}" "${GITHUB_ACTOR:?}" "${GH_TOKEN:?}"
    owner="${GITHUB_REPOSITORY%%/*}"
    owner="${owner,,}"
    charts="ghcr.io/$owner/charts"
    work="$(mktemp -d)"
    trap 'rm -rf "$work"' EXIT
    # Registry logins live in this run's own files, gone when it ends. Helm reads the same file.
    export DOCKER_CONFIG="$work/docker" HELM_REGISTRY_CONFIG="$work/docker/config.json"
    mkdir -p "$DOCKER_CONFIG"

    # Everything that can refuse does so before the first write. zeabur-oss-release refuses an
    # existing tag for each push; this refuses both before the first, so a release never stops
    # halfway on a tag that was there from the start. Recovering from a failed run is a
    # maintainer's decision (docs/internal/releasing.md).
    if gh release view "$version" --repo "$GITHUB_REPOSITORY" >/dev/null 2>&1; then
      echo "release $version already exists; inspect it before trying again" >&2
      exit 1
    fi
    printf '%s' "$GH_TOKEN" | docker login ghcr.io --username "$GITHUB_ACTOR" --password-stdin
    printf '%s' "$GH_TOKEN" | helm registry login ghcr.io --username "$GITHUB_ACTOR" --password-stdin
    refs=("$charts/zeabur:$plain")
    for name in "${images[@]}"; do refs+=("ghcr.io/$owner/$name:$plain"); done
    for ref in "${refs[@]}"; do
      if skopeo inspect --raw --authfile "$DOCKER_CONFIG/config.json" "docker://$ref" >/dev/null 2>&1; then
        echo "$ref already exists; inspect it before trying again" >&2
        exit 1
      fi
    done
    for build in linux-amd64 linux-arm64 darwin-arm64; do
      [[ -f "$candidates/candidate-$build/zeaburctl-$version-$build.tar.gz" ]] ||
        { echo "missing the $build build" >&2; exit 1; }
    done

    rm -f "$out/digests.txt" "$out/subjects.json"
    for name in "${images[@]}"; do
      archive="image.tar"
      [[ "$name" == zeabur ]] || archive="$name-image.tar"
      zeabur-oss-release push-image --repository "ghcr.io/$owner/$name" --tag "$plain" \
        --label "$(label "$name")" --digests "$out/digests.txt" --subjects "$out/subjects.json" \
        "linux/amd64=$candidates/candidate-linux-amd64/$archive" \
        "linux/arm64=$candidates/candidate-linux-arm64/$archive"
    done
    image="$(recorded "$out/digests.txt" index)"
    index="${image#*@}"
    proxy="$(recorded "$out/digests.txt" "proxy index")"
    proxy="${proxy#*@}"

    # The chart: each image pinned to its index, at the version of Chart.yaml (checked equal to
    # the release's by `check`).
    cp -R "$root/deploy/chart/zeabur" "$work/chart"
    # sed rather than `yq -i`, which would drop the blank lines between the documented values.
    sed -i -e "s|^  digest: \"\"\$|  digest: \"$index\"|" \
      -e "s|^    digest: \"\"\$|    digest: \"$proxy\"|" "$work/chart/values.yaml"
    pinned="$(chart_images "$work/chart")"
    [[ "$pinned" == "$(indexes "$out/digests.txt")" ]] ||
      { echo "the chart runs $pinned, not the images just pushed" >&2; exit 1; }
    helm lint --strict "$work/chart" --set publicUrl=https://zeabur.example.com \
      --set issuer.url=https://identity.example.com --set issuer.clientSecret.existingSecret=zeabur-oidc
    helm package "$work/chart" --destination "$out"
    zeabur-oss-release push-chart --repository "$charts" \
      --digests "$out/digests.txt" --subjects "$out/subjects.json" "$out/zeabur-$plain.tgz"

    # What the installer pins (`finish`).
    {
      echo "ZEABUR_PIN_ZEABUR_IMAGE=$image"
      echo "ZEABUR_PIN_ZEABUR_CHART_SHA256=$(sha256sum "$out/zeabur-$plain.tgz" | cut -d ' ' -f 1)"
    } >"$out/outputs.env"
    cat "$out/subjects.json" "$out/digests.txt"
    ;;

  finish)
    [[ $# == 6 ]] || usage
    os="$3" arch="$4"
    platform "$os" "$arch"
    artifacts="$(cd "$5" && pwd)"
    mkdir -p "$6"
    dist="$(cd "$6" && pwd)"
    cd "$root"
    # The installer runs on Linux servers only; release.yml finishes on Linux only.
    [[ "$os" == linux ]] || { echo "no installer for $os-$arch" >&2; exit 2; }
    # The catalog and Zeabur Identity releases, and the image and the chart that publish pushed.
    export_lines < <("$scripts/release-pins.sh" "$version")
    export_lines <"$artifacts/published/outputs.env"
    "$scripts/release-build.sh" "$version" "$arch" "$dist" installer
    ;;

  assets)
    [[ $# == 4 ]] || usage
    candidates="$(cd "$3" && pwd)"
    mkdir -p "$4/assets"
    out="$(cd "$4" && pwd)"
    files=(
      "$candidates/published/digests.txt"
      "$candidates/published/zeabur-$plain.tgz"
      "$candidates/candidate-darwin-arm64/zeaburctl-$version-darwin-arm64.tar.gz"
    )
    for arch in amd64 arm64; do
      files+=(
        "$candidates/candidate-linux-$arch/zeaburctl-$version-linux-$arch.tar.gz"
        "$candidates/candidate-linux-$arch/zeabur-$version-linux-$arch.tar.gz"
        "$candidates/finish-linux-$arch/zeabur-installer-$version-linux-$arch.tar.gz"
      )
    done
    for file in "${files[@]}"; do
      [[ -f "$file" ]] || { echo "missing $file" >&2; exit 1; }
    done
    cp "${files[@]}" "$out/assets/"
    zeabur-oss-release checksums "$out/assets"
    zeabur-oss-release notes "$root/CHANGELOG.md" "$version" --docs https://oss.zeabur.com/docs/zeabur/ \
      >"$out/notes.md"
    ;;

  verify)
    [[ $# == 5 ]] || usage
    platform "$3" "$4"
    directory="$(cd "$5" && pwd)"
    source_ref="${RELEASE_SOURCE_REF:-refs/tags/$version}"
    [[ "$source_ref" == "refs/tags/$version" ]] ||
      { echo "RELEASE_SOURCE_REF is $source_ref, not the tag refs/tags/$version" >&2; exit 2; }
    work="$(mktemp -d)"
    trap 'rm -rf "$work"' EXIT
    # A fresh, empty registry configuration: a publisher's login must not hide a private package.
    export DOCKER_CONFIG="$work/docker" HELM_REGISTRY_CONFIG="$work/docker/config.json"
    mkdir -p "$DOCKER_CONFIG"
    release=(--repository zeabur-oss/zeabur --version "$version")

    zeabur-oss-release verify-assets "${release[@]}" "$directory" "${assets[@]}"
    for name in "${images[@]}"; do
      zeabur-oss-release verify-image "${release[@]}" --digests "$directory/digests.txt" \
        --label "$(label "$name")" linux/amd64 linux/arm64
    done
    zeabur-oss-release verify-chart "${release[@]}" --digests "$directory/digests.txt" "$directory/zeabur-$plain.tgz"
    # The chart runs those images and no other.
    pinned="$(chart_images "$directory/zeabur-$plain.tgz")"
    [[ "$pinned" == "$(indexes "$directory/digests.txt")" ]] ||
      { echo "the chart runs $pinned, not the images in digests.txt" >&2; exit 1; }

    # The artifacts themselves: on Linux, the published chart on a disposable k3s cluster whose
    # node pulls the published image; on macOS, zeaburctl.
    if [[ "$3" == linux ]]; then
      "$scripts/release-smoke.sh" "$directory/zeaburctl-$version-$3-$4.tar.gz" "oci://$(recorded "$directory/digests.txt" chart)"
    else
      tar -xOzf "$directory/zeaburctl-$version-$3-$4.tar.gz" zeaburctl >"$work/zeaburctl"
      chmod 0755 "$work/zeaburctl"
      said="$("$work/zeaburctl" --version)"
      # Signed with a Developer ID only when the release had the certificate (`sign`); then
      # Gatekeeper must accept it as notarized.
      if /usr/bin/codesign -dvv "$work/zeaburctl" 2>&1 | grep -q '^Authority=Developer ID Application: '; then
        assessment="$(/usr/sbin/spctl --assess -vvv --type open --context context:primary-signature \
          "$work/zeaburctl" 2>&1)" || { echo "$assessment" >&2; exit 1; }
        [[ "$assessment" == *'source=Notarized Developer ID'* ]] ||
          { echo "zeaburctl is not notarized: $assessment" >&2; exit 1; }
      else
        echo "zeaburctl is not signed with a Developer ID"
      fi
      [[ "$said" == "zeaburctl $plain" ]] || { echo "zeaburctl says '$said'" >&2; exit 1; }
    fi
    echo "release $version verified on $3-$4"
    ;;

  *) usage ;;
esac
