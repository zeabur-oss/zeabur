#!/usr/bin/env bash
# Fails when a built binary would not run on a machine without Nix. A Mach-O file may link only
# libraries every Mac has (/usr/lib, /System/Library) and libraries next to it in the same bundle
# (@rpath, @executable_path, @loader_path). An ELF file may name nothing in /nix/store, needs the
# system's program interpreter, and may search for libraries only next to itself ($ORIGIN) and in
# the system's library folders. Checks every such file under the given paths.
#   scripts/check-portable.sh target/desktop/release/bundle/macos/Zeabur.app
set -euo pipefail
[[ $# -gt 0 ]] || { echo "usage: check-portable.sh <file or directory>..." >&2; exit 2; }

status=0
checked=0
fail() {
  echo "$1" >&2
  status=1
}

# A library path that starts with @ (@rpath, @executable_path, @loader_path) is in the bundle when a
# file under the given paths ends with the rest of it, past any "..".
resolves() {
  local rest="${1#*/}"
  rest="${rest##*../}"
  [[ -n "$(find "${roots[@]}" -path "*/$rest" -print -quit)" ]]
}
roots=("$@")

while IFS= read -r -d '' file; do
  case "$(file -b "$file")" in
    Mach-O*)
      checked=$((checked + 1))
      # Library lines are indented; the others name the file (once per architecture). A library's
      # own install name (otool -D) is listed too.
      id="$(otool -D "$file" | sed -n 2p)"
      while IFS= read -r library; do
        case "$library" in
          "$id" | /usr/lib/* | /System/Library/*) ;;
          @*) resolves "$library" || fail "$file links $library, which is not in the bundle" ;;
          *) fail "$file links $library, which is not part of macOS" ;;
        esac
      done < <(otool -L "$file" | sed -n 's/^\t\(.*\) (compatibility version .*/\1/p')
      while IFS= read -r rpath; do
        [[ "$rpath" == @* || "$rpath" == /usr/lib* || "$rpath" == /System/* ]] ||
          fail "$file searches $rpath for libraries, outside the bundle"
      done < <(otool -l "$file" | sed -n '/cmd LC_RPATH/{n;n;s/^ *path \(.*\) (offset [0-9]*)$/\1/p;}')
      ;;
    ELF*)
      checked=$((checked + 1))
      if readelf --program-headers --dynamic "$file" | grep -q /nix/store; then
        fail "$file names /nix/store in its interpreter or libraries: $(readelf --program-headers \
          --dynamic "$file" | grep -o '/nix/store/[^]: ]*' | sort -u | tr '\n' ' ')"
      fi
      interpreter="$(readelf --program-headers "$file" |
        sed -n 's/.*\[Requesting program interpreter: \(.*\)\]$/\1/p')"
      case "$interpreter" in
        "" | /lib/* | /lib64/* | /usr/lib/* | /usr/lib64/*) ;;
        *) fail "$file asks for the program interpreter $interpreter, not the system's" ;;
      esac
      # The GVFS modules an AppImage carries search GVFS's own folder in /usr/lib.
      while IFS= read -r path; do
        # shellcheck disable=SC2016
        case "$path" in
          '$ORIGIN'* | '${ORIGIN}'* | /lib/* | /lib64/* | /usr/lib/* | /usr/lib64/*) ;;
          *) fail "$file searches $path for libraries, outside the package and the system" ;;
        esac
      done < <(readelf --dynamic "$file" | sed -n 's/.*(R\(UN\)\{0,1\}PATH).*\[\(.*\)\]$/\2/p' |
        tr ':' '\n' | sed '/^$/d')
      ;;
  esac
done < <(find "$@" -type f -print0)

[[ "$checked" -gt 0 ]] || fail "no Mach-O or ELF file under $*"
[[ "$status" == 0 ]] && echo "portable: $*"
exit "$status"
