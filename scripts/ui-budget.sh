#!/usr/bin/env bash
# Fails if the web interface exceeds its size budget, measured as brotli (what a browser
# downloads). Run after scripts/build-ui.sh. Budgets: first measurements with headroom; raise one
# only with a reason in the commit.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
dist="${ENGAWA_UI_DIST:-$root/web/dist}"
index="$dist/index.html"

# The files the first page load needs: index.html, its entry script, every chunk it asks the
# browser to preload (<link rel="modulepreload">: fetched before the page runs), and its CSS.
entry_js=$(grep -o '<script[^>]*src="/assets/[^"]*\.js"' "$index" | grep -o '/assets/[^"]*' | sed 's/^\///')
preload_js=$(grep -o '<link rel="modulepreload"[^>]*href="/assets/[^"]*\.js"' "$index" | grep -o '/assets/[^"]*' | sed 's/^\///' || true)
entry_css=$(grep -o 'href="/assets/[^"]*\.css"' "$index" | sed 's/^href="\///; s/"$//')

if [[ -z "$entry_js" ]]; then
  echo "error: no entry script in $index" >&2
  exit 1
fi

brotli_size() { brotli --best --stdout "$dist/$1" | wc -c | tr -d ' '; }

status=0
check() { # $1 = label, $2 = bytes, $3 = budget
  printf '%-32s %8s %8s\n' "$1" "$2" "$3"
  if (($2 > $3)); then
    echo "error: $1 is $2 bytes compressed, over its budget of $3" >&2
    status=1
  fi
}

printf '%-32s %8s %8s\n' file brotli budget
check index.html "$(brotli_size index.html)" 2048
js=0
for f in $entry_js $preload_js; do
  size=$(brotli_size "$f")
  printf '  %-30s %8s\n' "$f" "$size"
  js=$((js + size))
done
check "first-load JavaScript" "$js" 204800
css=0
for f in $entry_css; do css=$((css + $(brotli_size "$f"))); done
check "first-load CSS" "$css" 24576

# The usage history's charts (shadcn/ui's Chart, Recharts) load on their own once a service page
# shows history, never with the first page; their chunk has a budget of its own.
charts=("$dist"/assets/history-chart-*.js)
if [[ ! -e "${charts[0]}" ]]; then
  echo "error: no history-chart chunk in $dist/assets" >&2
  status=1
else
  chart=0
  for f in "${charts[@]}"; do chart=$((chart + $(brotli_size "assets/${f##*/}"))); done
  check "history chart (lazy)" "$chart" 102400
fi
exit $status
