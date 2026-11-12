#!/usr/bin/env bash
# Screenshots of every screen (macOS). Needs the demo API and the web dev server running, and a
# built app (`cargo build` in desktop/).
#   desktop/shots.sh desktop/shots [screens...]
# LANGS and THEMES narrow the set, e.g. LANGS=zh-TW THEMES=dark.
set -u
D=$(cd "$(dirname "$0")" && pwd)
out=$1; shift
screens=${*:-first list search paste drop cluster}
home=$D/fixtures/home/.kube/config
mkdir -p "$out"
for lang in ${LANGS:-en zh-TW zh-CN}; do
 for theme in ${THEMES:-light dark}; do
  for screen in $screens; do
    st=$(mktemp -d)
    kc=$home; scen="lang=$lang"; target=main; delay=6000
    case $screen in
      first) kc=$D/fixtures/none ;;
      list) printf '{"added":[{"kind":"folder","path":"%s"}],"favourites":["%s"],"recents":["%s"]}\n' "$D/fixtures/team" "$D/fixtures/team/eks.yaml#prod-tokyo" "$home#zeabur-demo" > "$st/state.json" ;;
      search) scen="$scen;q=staging" ;;
      paste) scen="$scen;paste=1" ;;
      drop) scen="$scen;drop=1" ;;
      cluster) scen="$scen;open=zeabur-demo"; target=cluster; delay=8000 ;;
    esac
    if [[ $screen != list && $screen != first ]]; then
      printf '{"added":[{"kind":"folder","path":"%s"}]}\n' "$D/fixtures/team" > "$st/state.json"
    fi
    timeout 40 env -i HOME="$HOME" USER="$USER" SHELL="$SHELL" TMPDIR="$TMPDIR" \
      PATH=/usr/bin:/bin:/usr/sbin:/sbin \
      ZEABUR_DESKTOP_STATE=$st KUBECONFIG=$kc ZEABUR_DESKTOP_THEME=$theme \
      ZEABUR_DESKTOP_SCENARIO="$scen" ZEABUR_DESKTOP_SHOT="$out/$lang-$screen-$theme.png" \
      ZEABUR_DESKTOP_SHOT_TARGET=$target ZEABUR_DESKTOP_SHOT_DELAY=$delay \
      "$D/target/debug/zeabur-desktop" >/dev/null 2>&1
    echo "$lang-$screen-$theme $?"
    rm -rf "$st"
  done
 done
done
