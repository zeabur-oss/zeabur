// Dynamic values (positions on the Architecture canvas, sliding highlights, bar heights) reach the
// styles as CSS custom properties on the element, read by classes such as `left-(--x)`; `style`
// carries nothing else (shadcn/lint's no-inline-styles, https://github.com/shadcn-ui/lint/blob/main/docs/rules/no-inline-styles.md).
import "react"

declare module "react" {
  interface CSSProperties {
    [property: `--${string}`]: string | number | undefined
  }
}
