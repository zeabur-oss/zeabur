#!/usr/bin/env bash
# Fails if a Zeabur Desktop package would load anything from outside the system and the package
# itself: a Nix store path, or a library the build machine had that a user's machine may not.
#   scripts/check-desktop-bundle.sh Zeabur.app | <file>.dmg | <file>.deb | <file>.AppImage | <file>.rpm
# On macOS every Mach-O may link only /usr/lib, /System and @rpath, @executable_path or
# @loader_path inside the app, and no LC_RPATH or load path may name /nix. On Linux every ELF
# needs the system's program interpreter, no RPATH or RUNPATH outside the package and the system's
# library folders, and every library it needs found outside /nix.
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <Zeabur.app|.dmg|.deb|.rpm|.AppImage>" >&2
  exit 2
fi
if [[ ! -e "$1" ]]; then
  echo "error: $1 does not exist" >&2
  exit 2
fi
package=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$(mktemp -d)
mounted=""
cleanup() {
  if [[ -n "$mounted" ]]; then hdiutil detach -quiet "$mounted" || true; fi
  rm -rf "$work"
}
trap cleanup EXIT

problems=()
checked=0

check_macho() {
  local file=$1 line lib
  while IFS= read -r line; do
    lib=${line%% (*}
    lib=${lib#"${lib%%[![:space:]]*}"}
    case "$lib" in
      /usr/lib/* | /System/Library/* | @rpath/* | @executable_path/* | @loader_path/*) ;;
      *) problems+=("$file links $lib") ;;
    esac
  done < <(otool -L "$file" | tail -n +2)
  if otool -l "$file" | grep -A2 LC_RPATH | grep -q '/nix/'; then
    problems+=("$file has an LC_RPATH in /nix")
  fi
}

check_elf() {
  local file=$1 root=$2 interp dynamic missing
  interp=$(readelf -lW "$file" | sed -n 's/.*\[Requesting program interpreter: \(.*\)\]/\1/p')
  case "$interp" in
    "" | /lib/* | /lib64/* | /usr/lib/* | /usr/lib64/*) ;;
    *) problems+=("$file asks for the interpreter $interp") ;;
  esac
  dynamic=$(readelf -dW "$file" 2>/dev/null || true)
  while IFS= read -r path; do
    # A path relative to the file itself, which the package carries, or a system library folder
    # (the GVFS modules an AppImage carries look for GVFS's own libraries there).
    # shellcheck disable=SC2016
    case "$path" in
      '$ORIGIN'* | '${ORIGIN}'* | /lib/* | /lib64/* | /usr/lib/* | /usr/lib64/*) ;;
      *) problems+=("$file has the library path $path") ;;
    esac
  done < <(sed -n 's/.*(R\(UN\)\{0,1\}PATH).*\[\(.*\)\]/\2/p' <<<"$dynamic" | tr ':' '\n' | sed '/^$/d')
  if grep -q '/nix/' <<<"$dynamic"; then
    problems+=("$file names /nix in its dynamic section")
  fi
  # The libraries the system resolves for it (the system's ldd, never a Nix shell's); an AppImage's
  # own come first, as its AppRun sets.
  if [[ -n "$interp" ]]; then
    missing=$(LD_LIBRARY_PATH="$root/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" /usr/bin/ldd "$file" 2>&1 |
      grep -E 'not found|/nix/' || true)
    if [[ -n "$missing" ]]; then
      problems+=("$file: $(tr '\n' ' ' <<<"$missing")")
    fi
  fi
}

walk() {
  local root=$1 kind=$2 file
  while IFS= read -r -d '' file; do
    if [[ "$kind" == macho ]] && file -b "$file" | grep -q '^Mach-O'; then
      check_macho "$file"
      checked=$((checked + 1))
    elif [[ "$kind" == elf ]] && file -b "$file" | grep -q '^ELF'; then
      check_elf "$file" "$root"
      checked=$((checked + 1))
    fi
  done < <(find "$root" -type f -print0)
}

case "$package" in
  *.app) walk "$package" macho ;;
  *.dmg)
    mounted="$work/mount"
    mkdir "$mounted"
    hdiutil attach -quiet -nobrowse -readonly -mountpoint "$mounted" "$package"
    walk "$mounted" macho
    ;;
  *.deb)
    dpkg-deb -x "$package" "$work/root"
    walk "$work/root" elf
    ;;
  *.rpm)
    mkdir "$work/root"
    (cd "$work/root" && rpm2cpio "$package" | cpio -idm --quiet)
    walk "$work/root" elf
    ;;
  *.AppImage)
    (cd "$work" && APPIMAGE_EXTRACT_AND_RUN=1 "$package" --appimage-extract >/dev/null)
    walk "$work/squashfs-root" elf
    ;;
  *)
    echo "error: $package is not an .app, .dmg, .deb, .rpm or .AppImage" >&2
    exit 2
    ;;
esac

if [[ $checked -eq 0 ]]; then
  problems+=("no executable or library in $(basename "$package")")
fi
if [[ ${#problems[@]} -gt 0 ]]; then
  printf 'error: %s\n' "${problems[@]}" >&2
  exit 1
fi
echo "$(basename "$package"): $checked files load only system libraries and the package's own"
