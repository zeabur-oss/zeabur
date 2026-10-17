// Icons are Phosphor's, imported one by one as `~icons/ph/<name>` (`~icons/ph/warning-fill` for a
// weight) and built in by unplugin-icons (vite.config.ts). Like Phosphor's own components they are
// 1em square and take the text colour, so a class sizes and tints them.
import type { ComponentType, SVGProps } from "react"

/** An icon component, to pass one around (a navigation item's icon). */
export type Icon = ComponentType<SVGProps<SVGSVGElement>>
