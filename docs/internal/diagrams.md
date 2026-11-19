# Docs diagrams

A docs diagram is one SVG for each locale at `docs/[<locale>/]images/diagrams/<id>.svg`. The page
shows it like a screenshot. The SVG uses only the values in this file. A restyle changes this
table and then the same values in each SVG.

## Palette

| Use | Class | Light | Dark |
| --- | --- | --- | --- |
| Figure background | `.bg` fill | `#FAFAFB` | `#1D1C20` |
| Figure border, group border | `.bg`, `.grp` stroke | `#E8E7EB` | `#34313A` |
| Group box | `.grp` fill | `rgba(255,255,255,.55)` | `rgba(31,30,34,.55)` |
| Card | `.card` fill | `#FFFFFF` | `#1F1E22` |
| Card border, tag border, arrow | `.card`, `.tag` stroke, `.line`, `.head` | `#D0CED4` | `#4C4851` |
| Title text | `.t` | `#18171B` | `#F0EFF2` |
| Body text, labels, tags | `.n`, `.h`, `.tagt` | `#5C5962` | `#B0ADB6` |
| The one thing that matters | `.accent` stroke, `.hitt` | `#6300FF` | `#A469FF` |
| Accent tag background | `.hit` | `rgba(99,0,255,.07)` | `rgba(164,105,255,.14)` |

## Shape and type

| Value | Use |
| --- | --- |
| Width 440 | Every figure, so the text stays readable on a 390 px screen. |
| Stroke 1 | Borders. Stroke 1.5: the accent card and arrows. |
| Corner 6 | Figure, groups and cards. Corner 4: tags. |
| Font | `-apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif` |
| 14 px, weight 600 | Card titles (`.t`). |
| 13 px | Card text (`.n`). |
| 12.5 px | Group labels (`.h`). |
| 12 px | Tags (`.tagt`, `.hitt`). |

The dark values go in `@media (prefers-color-scheme: dark)` inside the SVG's `<style>`. Keep a
figure to about seven boxes. No shadows, glows, gradients or window dots.
