# shellcheck shell=bash
# Sourced by the release scripts.
#
#   pack <dir> <archive> <file>...
#
# Packs the files of <dir> into <archive>, a .tar.gz or a .zip by its name, so the same files give
# the same archive: entries in the order given (give them sorted), SOURCE_DATE_EPOCH as each one's
# time, no owner names, no gzip timestamp. GNU tar in the Nix shell; macOS's bsdtar and Windows'
# tar.exe on a runner outside it.
pack() {
  local dir=$1 archive=$2
  shift 2
  : "${SOURCE_DATE_EPOCH:?set SOURCE_DATE_EPOCH to the commit time}"
  case "$archive" in
    *.tar.gz)
      if [[ "$(tar --version)" == *'GNU tar'* ]]; then
        tar --format=ustar --sort=name --mtime="@$SOURCE_DATE_EPOCH" \
          --owner=0 --group=0 --numeric-owner -C "$dir" -cf - "$@" | gzip -n -9 >"$archive"
      else
        (cd "$dir" && TZ=UTC touch -t "$(TZ=UTC date -r "$SOURCE_DATE_EPOCH" +%Y%m%d%H%M.%S)" "$@")
        tar --format ustar --uid 0 --gid 0 --uname "" --gname "" --no-xattrs --no-mac-metadata \
          -C "$dir" -cf - "$@" | gzip -n -9 >"$archive"
      fi
      ;;
    *.zip)
      # Git Bash's tar is GNU tar, which cannot write a zip.
      (cd "$dir" && touch -d "@$SOURCE_DATE_EPOCH" "$@")
      "$(cygpath -u "$SYSTEMROOT")/System32/tar.exe" --format zip \
        -C "$(cygpath -w "$dir")" -cf "$(cygpath -w "$archive")" "$@"
      ;;
    *)
      echo "pack: $archive is not a .tar.gz or a .zip" >&2
      return 2
      ;;
  esac
}
