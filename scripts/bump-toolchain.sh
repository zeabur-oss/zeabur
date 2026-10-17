#!/usr/bin/env bash
# Moves rust-toolchain.toml to the newest nightly of the last week that ships every listed
# component and target for every system the flake supports, and updates the fenix `sha256` in
# flake.nix in the same step (it is the hash of that day's channel manifest).
# Usage: scripts/bump-toolchain.sh   (run inside `nix develop`; leaves the files unchanged when no
# newer nightly qualifies)
set -euo pipefail
cd "$(dirname "$0")/.."

# Rust host triples of `supportedSystems` in flake.nix.
hosts='[ "x86_64-unknown-linux-gnu" "aarch64-unknown-linux-gnu" "aarch64-apple-darwin" ]'
systems=(x86_64-linux aarch64-linux aarch64-darwin)

current="$(sed -n 's/^channel = "nightly-\([0-9-]*\)"$/\1/p' rust-toolchain.toml)"
if [[ -z $current ]]; then
  echo "error: rust-toolchain.toml has no 'channel = \"nightly-YYYY-MM-DD\"' line" >&2
  exit 1
fi

# Prints "<package> <target>" for everything rust-toolchain.toml needs that the manifest lacks.
# A nightly can be published with a component missing on some hosts (rustup skips such days); fenix
# only notices when building the toolchain.
missing() {
  nix eval --impure --raw --expr "
    let
      m = builtins.fromTOML (builtins.readFile $1);
      t = (builtins.fromTOML (builtins.readFile ./rust-toolchain.toml)).toolchain;
      pkg = c: m.renames.\${c}.to or c;
      available = p: target:
        (m.pkg.\${p}.target.\${target}.available or false) || (m.pkg.\${p}.target.\"*\".available or false);
      needs = builtins.concatMap (host:
        map (c: { p = pkg c; target = host; }) ([ \"rustc\" \"cargo\" \"rust-std\" ] ++ t.components)
        ++ map (target: { p = \"rust-std\"; target = target; }) t.targets) $hosts;
    in
    builtins.concatStringsSep \"\" (map (n: \"\${n.p} \${n.target}\n\")
      (builtins.filter (n: !(available n.p n.target)) needs))"
}

toolchain_orig="$(cat rust-toolchain.toml)"
flake_orig="$(cat flake.nix)"
restore() {
  printf '%s\n' "$toolchain_orig" > rust-toolchain.toml
  printf '%s\n' "$flake_orig" > flake.nix
}
# Only a nightly that passed every check stays written; any other exit puts the files back.
keep=false
trap '$keep || restore' EXIT

for days_ago in 0 1 2 3 4 5 6; do
  date="$(date -u -d "$days_ago days ago" +%F)"
  if [[ ! $date > $current ]]; then
    break
  fi
  # A day without a nightly (or one not yet published) has no manifest.
  if ! manifest="$(nix store prefetch-file --json \
    "https://static.rust-lang.org/dist/$date/channel-rust-nightly.toml" 2>/dev/null)"; then
    echo "nightly-$date: no manifest" >&2
    continue
  fi
  hash="$(jq -r .hash <<< "$manifest")"
  printf '%s\n' "$toolchain_orig" |
    sed "s/^channel = \"nightly-$current\"$/channel = \"nightly-$date\"/" > rust-toolchain.toml
  printf '%s\n' "$flake_orig" |
    sed "s|^\( *sha256 = \"\)sha256-[^\"]*\";$|\1$hash\";|" > flake.nix

  if ! lacking="$(missing "$(jq -r .storePath <<< "$manifest")")"; then
    echo "nightly-$date: could not read the manifest" >&2
    restore
    continue
  fi
  if [[ -n $lacking ]]; then
    echo "nightly-$date: missing" "$lacking" >&2
    restore
    continue
  fi
  # Every dev shell must still evaluate with the current fenix and nixpkgs (this also checks the
  # hash fenix fetches with); an older nightly may still work when this one does not.
  for system in "${systems[@]}"; do
    if ! nix eval --raw ".#devShells.$system.default.drvPath" > /dev/null; then
      echo "nightly-$date: the $system dev shell does not evaluate" >&2
      restore
      continue 2
    fi
  done
  keep=true
  echo "nightly-$current -> nightly-$date"
  exit 0
done

echo "nightly-$current is the newest usable nightly"
