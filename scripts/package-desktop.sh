#!/usr/bin/env bash
# Builds Zeabur Desktop's release packages for this machine and copies them into <out> with their
# release names (scripts/desktop-release.ts), signed when the signing variables are set.
#   scripts/package-desktop.sh compile <version> <os> <arch> <out>   # v0.3.0 darwin arm64 dist
#   scripts/package-desktop.sh package <version> <os> <arch> <out>
# compile builds the app and package bundles and signs it, so no build script of a dependency runs
# with the signing variables set.
#
# Runs outside the Nix shell, with the system's C compiler and linker: a binary linked in the
# shell loads libraries from /nix/store, which a user's machine does not have. It needs on PATH
# cargo and rustc of the nightly flake.lock pins and cargo-tauri; in the environment
# ZEABUR_CATALOG_RELEASE, ZEABUR_CATALOG_COMMIT and ZEABUR_UI_DIST, as the dev shell sets them; and
# desktop/web/dist built. .github/workflows/desktop.yml prepares all of this.
#
# Signing:
#   macOS    APPLE_CERTIFICATE_P12, APPLE_CERTIFICATE_PASSWORD, APPLE_API_KEY_P8, APPLE_API_KEY_ID
#            and APPLE_API_ISSUER_ID (the apple-signing environment): Developer ID, hardened
#            runtime, notarized and stapled.
#   Windows  AZURE_SIGNING_ENDPOINT, AZURE_SIGNING_ACCOUNT and AZURE_SIGNING_PROFILE, after
#            `az login`: Azure Artifact Signing through signtool.
# Without them the packages are unsigned (ad hoc on macOS). The updater's signatures are made
# later, in one job that holds that key.
set -euo pipefail

if [[ $# -ne 5 || ( "$1" != compile && "$1" != package ) ]]; then
  echo "usage: $0 <compile|package> <version> <os> <arch> <out>" >&2
  exit 2
fi
stage=$1 version=$2 os=$3 arch=$4
mkdir -p "$5"
out=$(cd "$5" && pwd)
root=$(cd "$(dirname "$0")/.." && pwd)
name="zeabur-desktop-$version-$os-$arch"

for var in ZEABUR_CATALOG_RELEASE ZEABUR_CATALOG_COMMIT ZEABUR_UI_DIST; do
  if [[ -z "${!var:-}" ]]; then
    echo "error: $var is not set" >&2
    exit 1
  fi
done
if [[ ! -f "$root/desktop/web/dist/index.html" ]]; then
  echo "error: desktop/web/dist is not built" >&2
  exit 1
fi
if [[ -n "${NIX_CC:-}${NIX_LDFLAGS:-}" ]]; then
  echo "error: run this outside the Nix shell; it links with the system's tools" >&2
  exit 1
fi
if [[ "$os" != windows ]]; then
  for tool in cc ld; do
    if [[ "$(command -v "$tool")" == /nix/* ]]; then
      echo "error: $tool is $(command -v "$tool"); the build must use the system's" >&2
      exit 1
    fi
  done
fi

export CARGO_TARGET_DIR="$root/target/desktop-release"
bundle="$CARGO_TARGET_DIR/release/bundle"
cd "$root/desktop"
if [[ "$stage" == compile ]]; then
  cargo tauri build --ci --no-bundle
  exit 0
fi
rm -rf "$bundle"
work=$(mktemp -d)
cleanup=""
trap 'eval "$cleanup"; rm -rf "$work"' EXIT

# Fails if a package loads anything from outside the system and itself (scripts/check-portable.sh).
portable() {
  local dir
  dir="$work/unpacked-$(basename "$1")"
  mkdir "$dir"
  case "$1" in
    *.deb) dpkg-deb -x "$1" "$dir" ;;
    *.rpm) bsdtar -xf "$1" -C "$dir" ;;
    *.AppImage) (cd "$dir" && "$1" --appimage-extract >/dev/null) ;;
  esac
  "$root/scripts/check-portable.sh" "$dir"
}

# The single file a bundle step left, by pattern.
one() {
  local found
  found=$(find "$1" -maxdepth 1 -name "$2")
  if [[ $(wc -l <<<"$found") -ne 1 || -z "$found" ]]; then
    echo "error: expected one $2 in $1, found: ${found:-none}" >&2
    exit 1
  fi
  echo "$found"
}

case "$os" in
  darwin)
    signing=false
    if [[ -n "${APPLE_CERTIFICATE_P12:-}" ]]; then
      signing=true
      keychain="$work/signing.keychain-db"
      secret=$(uuidgen)
      # The user's keychains, to put back; macOS's own paths have no spaces.
      keychains=$(security list-keychains -d user | tr -d ' "' | tr '\n' ' ')
      security create-keychain -p "$secret" "$keychain"
      cleanup="security list-keychains -d user -s $keychains; security delete-keychain '$keychain' 2>/dev/null || true"
      security set-keychain-settings -lut 3600 "$keychain"
      security unlock-keychain -p "$secret" "$keychain"
      # shellcheck disable=SC2086
      security list-keychains -d user -s "$keychain" $keychains
      base64 --decode <<<"$APPLE_CERTIFICATE_P12" >"$work/certificate.p12"
      security import "$work/certificate.p12" -k "$keychain" -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
      security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$secret" "$keychain" >/dev/null
      identity=$(security find-identity -v -p codesigning "$keychain" |
        sed -n 's/.*) \([0-9A-F]\{40\}\) "Developer ID Application: .*/\1/p' | head -n 1)
      if [[ -z "$identity" ]]; then
        echo "error: the certificate is not a Developer ID Application one" >&2
        exit 1
      fi
      printf '%s\n' "$APPLE_API_KEY_P8" >"$work/key.p8"
      # The Tauri bundler signs with the hardened runtime and a timestamp, then notarizes and
      # staples the app.
      export APPLE_SIGNING_IDENTITY=$identity APPLE_API_KEY=$APPLE_API_KEY_ID \
        APPLE_API_ISSUER=$APPLE_API_ISSUER_ID APPLE_API_KEY_PATH="$work/key.p8"
    else
      # Ad hoc, so the app's signature covers its resources as a Developer ID one would.
      export APPLE_SIGNING_IDENTITY=-
    fi
    cargo tauri bundle --ci --bundles app,dmg
    app=$(one "$bundle/macos" '*.app')
    dmg=$(one "$bundle/dmg" '*.dmg')
    codesign --verify --deep --strict --verbose=2 "$app"
    if $signing; then
      codesign --force --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$dmg"
      xcrun notarytool submit "$dmg" --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" \
        --issuer "$APPLE_API_ISSUER" --wait --timeout 30m
      xcrun stapler staple "$dmg"
      spctl --assess --type execute --verbose=2 "$app"
      spctl --assess --type open --context context:primary-signature --verbose=2 "$dmg"
    else
      echo "::warning::no Developer ID certificate: the app is signed ad hoc and not notarized"
    fi
    "$root/scripts/check-portable.sh" "$app"
    # The updater replaces the whole app with the one in this archive; without the build machine's
    # extended attributes.
    tar --no-xattrs --no-mac-metadata -czf "$out/$name.app.tar.gz" -C "$(dirname "$app")" "$(basename "$app")"
    cp "$dmg" "$out/$name.dmg"
    ;;

  windows)
    sign=()
    if [[ -n "${AZURE_SIGNING_ENDPOINT:-}" ]]; then
      # Microsoft's signtool plug-in for Azure Artifact Signing, pinned by version and digest. It
      # signs with the Azure CLI login of azure/login.
      dlib_version=1.0.151
      dlib_sha256=35375a4bc4e1102da729fc3d8d9925f774c3baae6b8b7ff6398d948ca9ab0d8a
      curl --fail --silent --show-error --location --retry 3 --output "$work/dlib.nupkg" \
        "https://api.nuget.org/v3-flatcontainer/microsoft.artifactsigning.client/$dlib_version/microsoft.artifactsigning.client.$dlib_version.nupkg"
      echo "$dlib_sha256  $work/dlib.nupkg" | sha256sum --check --status
      unzip -q "$work/dlib.nupkg" -d "$work/dlib"
      jq -n --arg endpoint "$AZURE_SIGNING_ENDPOINT" --arg account "$AZURE_SIGNING_ACCOUNT" \
        --arg profile "$AZURE_SIGNING_PROFILE" \
        '{Endpoint: $endpoint, CodeSigningAccountName: $account, CertificateProfileName: $profile}' \
        >"$work/metadata.json"
      signtool=$(find "/c/Program Files (x86)/Windows Kits/10/bin" -path '*/x64/signtool.exe' | sort -V | tail -n 1)
      sign=("$(cygpath -w "$signtool")" sign /v /fd SHA256 /tr http://timestamp.acs.microsoft.com /td SHA256
        /dlib "$(cygpath -w "$work/dlib/bin/x64/Azure.CodeSigning.Dlib.dll")"
        /dmdf "$(cygpath -w "$work/metadata.json")")
    else
      echo "::warning::no Azure Artifact Signing account: the Windows files are not signed"
    fi
    # The portable .exe is the plain build. The bundler then marks the binary as the installer's,
    # so the copy is taken before it bundles.
    cp "$CARGO_TARGET_DIR/release/zeabur-desktop.exe" "$out/$name-portable.exe"
    config=()
    if [[ ${#sign[@]} -gt 0 ]]; then
      "${sign[@]}" "$(cygpath -w "$out/$name-portable.exe")"
      jq -n --arg cmd "${sign[0]}" '$ARGS.positional as $args |
        {bundle: {windows: {signCommand: {cmd: $cmd, args: ($args + ["%1"])}}}}' \
        --args "${sign[@]:1}" >"$work/sign.json"
      config=(--config "$(cygpath -w "$work/sign.json")")
    fi
    cargo tauri bundle --ci --bundles nsis "${config[@]}"
    cp "$(one "$bundle/nsis" '*-setup.exe')" "$out/$name-setup.exe"
    if [[ ${#sign[@]} -gt 0 ]]; then
      for file in "$out/$name-portable.exe" "$out/$name-setup.exe"; do
        "${sign[0]}" verify /pa "$(cygpath -w "$file")"
      done
    fi
    ;;

  linux)
    # linuxdeploy, which the bundler runs for the AppImage, is itself an AppImage; this runs it
    # without FUSE.
    export APPIMAGE_EXTRACT_AND_RUN=1
    cargo tauri bundle --ci --bundles deb,rpm,appimage
    deb=$(one "$bundle/deb" '*.deb')
    rpm=$(one "$bundle/rpm" '*.rpm')
    appimage=$(one "$bundle/appimage" '*.AppImage')
    for file in "$deb" "$rpm" "$appimage"; do
      portable "$file"
    done
    cp "$deb" "$out/$name.deb"
    cp "$rpm" "$out/$name.rpm"
    cp "$appimage" "$out/$name.AppImage"
    ;;

  *)
    echo "error: no desktop build for $os" >&2
    exit 2
    ;;
esac
ls -l "$out"
