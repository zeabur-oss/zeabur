import type { Schemas } from "@/api/client"

type Change = Schemas.Change

export const PAGE_KEYS = [
  "maintenance",
  "badGateway",
  "serviceUnavailable",
  "gatewayTimeout",
] as const

export type PageKey = (typeof PAGE_KEYS)[number]

export type Pages = Record<PageKey, string>

/** The key of each page in the page server's ConfigMap. */
const PAGE_FILES: Record<PageKey, string> = {
  maintenance: "maintenance.html",
  badGateway: "502.html",
  serviceUnavailable: "503.html",
  gatewayTimeout: "504.html",
}

/** The server refuses a larger page. */
const PAGE_MAX = 64 * 1024

export const tooLarge = (html: string) => new TextEncoder().encode(html).length > PAGE_MAX

/** The pages in the ConfigMap, when it has all four. */
export function pagesOf(data: Record<string, string> | undefined): Pages | undefined {
  if (!data) return undefined
  const pages = Object.fromEntries(PAGE_KEYS.map((key) => [key, data[PAGE_FILES[key]]]))
  return PAGE_KEYS.every((key) => typeof pages[key] === "string") ? (pages as Pages) : undefined
}

const escape = (text: string) => text.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`)

/** A plain page that names the project, readable in light and dark mode. */
export function defaultPage(lang: string, project: string, heading: string, line: string) {
  return `<!doctype html>
<html lang="${escape(lang)}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escape(project)}</title>
<style>
body { margin: 0; min-height: 100vh; display: grid; place-items: center; font-family: system-ui, sans-serif; color: #1f1f1f; background: #fafafa; }
main { max-width: 32rem; padding: 2rem; text-align: center; }
h1 { font-size: 1.5rem; font-weight: 600; }
p { color: #5c5c5c; }
@media (prefers-color-scheme: dark) { body { color: #ededed; background: #141414; } p { color: #a3a3a3; } }
</style>
</head>
<body>
<main>
<h1>${escape(heading)}</h1>
<p>${escape(line)}</p>
</main>
</body>
</html>
`
}

export interface PagesForm {
  maintenance: boolean
  errorPages: boolean
  /** Pages typed in the form; the others keep their HTML. */
  edited: Partial<Pages>
}

/** What the form asks for, against what the route and the page server say now. */
export function pageChanges(
  route: Schemas.RouteRef,
  host: string,
  was: {
    maintenance: boolean
    errorPages: boolean
    /** The project has a page server. */
    server: boolean
    pages?: Pages
    defaults: Pages
  },
  form: PagesForm,
): Change[] {
  const starts = (form.maintenance && !was.maintenance) || (form.errorPages && !was.errorPages)
  const html = was.server
    ? htmlChange(was.pages, form.edited)
    : deployChange(was.defaults, form.edited, starts)
  return [
    ...html,
    ...(form.errorPages === was.errorPages
      ? []
      : [{ op: "errorPages", route, enabled: form.errorPages } as const]),
    ...(form.maintenance === was.maintenance
      ? []
      : [{ op: "maintenance", route, hostname: host, enabled: form.maintenance } as const]),
  ]
}

/** Without a page server: deploy the template with the pages, when the form needs them. */
function deployChange(defaults: Pages, edited: Partial<Pages>, starts: boolean): Change[] {
  const typed = PAGE_KEYS.some((key) => edited[key] !== undefined)
  return starts || typed ? [{ op: "pageServer", ...defaults, ...edited }] : []
}

/** Only the pages that changed. */
function htmlChange(pages: Pages | undefined, edited: Partial<Pages>): Change[] {
  const changed = PAGE_KEYS.filter(
    (key) => edited[key] !== undefined && edited[key] !== pages?.[key],
  )
  if (changed.length === 0) return []
  return [{ op: "errorPageHtml", ...Object.fromEntries(changed.map((key) => [key, edited[key]])) }]
}
