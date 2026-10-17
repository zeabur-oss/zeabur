#!/usr/bin/env bash
# Fails if the web interface uses a style it has given up (see web/DESIGN.md): all-caps
# labels (`uppercase`), wide letter-spacing (`tracking-wider`, `tracking-widest`), gradients
# (`bg-gradient-*`), glass (`backdrop-blur*`) and Phosphor's `duotone` weight. A use that is really
# needed goes in the allowlist below as `path:token`, with the reason above it.
#
# It also fails on copy patterns that make a page read as generated (web/DESIGN.md, "Copy"):
# - a Kubernetes kind after a heading (`kube=` on Panel or Section, "Public · Ingress"): headings
#   are plain words; the kind goes in the rows, a tooltip or the YAML tab;
# - a middle dot joining facts ("A · B · C"), in code or in the message catalogues: give each value
#   its label, join with the locale's separator (`t("common.sep")`) or set them apart by space;
# - a bordered pill in small type for a fact ("Deployment", "HTTPS configured"): status is a
#   StatusPill, anything else plain text. Exceptions go in `chip_allow` as `path:line text`.
# Run from anywhere; needs only grep.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

allow=(
  # Masked Secret values: the bullets are spaced so their count reads at a glance.
  "web/src/routes/service/variables.tsx:tracking-widest"
)

# A line worth a closer look, then each class token on it with any variant prefix (`md:`) and
# important marker (`!`) taken off, compared whole.
pattern='uppercase|tracking-wider|tracking-widest|bg-gradient-|backdrop-blur|duotone'
banned='^(uppercase|tracking-wider|tracking-widest|bg-gradient-[[:alnum:]-]+|backdrop-blur(-[[:alnum:]]+)?|duotone)$'

status=0
# shadcn/ui's own components (components/ui) and the files vendored from Genkan are left as their
# upstream wrote them.
while IFS= read -r hit; do
  file=${hit%%:*}
  rest=${hit#*:}
  line=${rest%%:*}
  text=${rest#*:}
  while IFS= read -r token; do
    token=${token##*:}
    token=${token//!/}
    [[ "$token" =~ $banned ]] || continue
    allowed=0
    for entry in "${allow[@]}"; do
      if [[ "$entry" == "$file:$token" ]]; then allowed=1; fi
    done
    if [[ $allowed -eq 0 ]]; then
      echo "error: $file:$line uses \`$token\`, a banned style (see scripts/check-ui-styles.sh)" >&2
      status=1
    fi
  done < <(grep -oE '[[:alnum:]_:/.!-]+' <<<"$text")
done < <(grep -rnE --include='*.ts' --include='*.tsx' --include='*.css' "$pattern" web/src \
  --exclude-dir=ui --exclude-dir=genkan || true)

# App code and catalogues: everything under web/src but shadcn/ui's components, Genkan's files and
# the generated API types.
sources() {
  grep -rnE --include='*.ts' --include='*.tsx' "$1" web/src \
    --exclude-dir=ui --exclude-dir=genkan --exclude=schema.d.ts || true
}

while IFS= read -r hit; do
  [[ -n "$hit" ]] || continue
  echo "error: ${hit%%:*}:$(cut -d: -f2 <<<"$hit") puts a Kubernetes kind after a heading (\`kube=\`); headings are plain words, the kind goes in the rows, a tooltip or the YAML tab" >&2
  status=1
done < <(sources '(^|[[:space:]])kube=')

while IFS= read -r hit; do
  [[ -n "$hit" ]] || continue
  text=${hit#*:*:}
  # Comments may use the dot; what a person reads may not.
  [[ "$text" =~ ^[[:space:]]*(//|\*|/\*) ]] && continue
  echo "error: ${hit%%:*}:$(cut -d: -f2 <<<"$hit") joins facts with a middle dot (\"A · B\"); label each value, use t(\"common.sep\") or set them apart by space" >&2
  status=1
done < <(sources '·')

chip_allow=(
  # A key hint in the palette's footer is a key cap, not a fact.
  'web/src/components/shell/palette-body.tsx:<kbd className="rounded border bg-muted px-1.5 type-nano'
)
chip='rounded(-full|-md)? ([^"]* )?border( [^"]*)? type-(micro|nano)|type-(micro|nano) ([^"]* )?rounded(-full|-md)? ([^"]* )?border( |")'
while IFS= read -r hit; do
  [[ -n "$hit" ]] || continue
  file=${hit%%:*}
  text=${hit#*:*:}
  allowed=0
  for entry in "${chip_allow[@]}"; do
    if [[ "$file" == "${entry%%:*}" && "$text" == *"${entry#*:}"* ]]; then allowed=1; fi
  done
  [[ $allowed -eq 1 ]] && continue
  echo "error: $file:$(cut -d: -f2 <<<"$hit") draws a bordered pill for a fact; status is a StatusPill, anything else plain text" >&2
  status=1
done < <(sources "$chip")

if [[ $status -eq 0 ]]; then echo "web/src uses no banned styles or copy patterns"; fi
exit $status
