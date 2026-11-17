# shellcheck shell=bash
# Sourced by the scripts: `cargo` goes through kache when the shell has it.
cargo() {
  if command -v kache >/dev/null; then
    kache cargo -- "$@"
  else
    command cargo "$@"
  fi
}
