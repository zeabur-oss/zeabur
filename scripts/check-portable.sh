#!/usr/bin/env bash
# Fails when a built binary would not run on a machine without Nix. A Mach-O file may link only
# libraries every Mac has (/usr/lib, /System/Library) and libraries next to it in the same bundle
# (@rpath, @executable_path, @loader_path). An ELF file may name nothing in /nix/store: no program
# interpreter, library path or library there. Checks every such file under the given paths.
#   scripts/check-portable.sh target/desktop/release/bundle/macos/Zeabur.app
set -euo pipefail
[[ $# -gt 0 ]] || { echo "usage: check-portable.sh <file or directory>..." >&2; exit 2; }

status=0
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
      if readelf --program-headers --dynamic "$file" | grep -q /nix/store; then
        fail "$file names /nix/store in its interpreter or libraries: $(readelf --program-headers \
          --dynamic "$file" | grep -o '/nix/store/[^]: ]*' | sort -u | tr '\n' ' ')"
      fi
      ;;
  esac
done < <(find "$@" -type f -print0)

[[ "$status" == 0 ]] && echo "portable: $*"
exit "$status"
