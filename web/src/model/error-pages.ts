import type { Schemas } from "@/api/client"

type Change = Schemas["Change"]

/** The ConfigMap and Deployment of the project's error-page server. */
export const ERROR_PAGE_SERVER = "zeabur-error-pages"

export const PAGE_KEYS = [
  "maintenance",
  "badGateway",
  "serviceUnavailable",
  "gatewayTimeout",
] as const

export type PageKey = (typeof PAGE_KEYS)[number]

export type Pages = Record<PageKey, string>

/** The ConfigMap key of each page. */
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

/** What the form asks for, against what the route and the ConfigMap say now. */
export function pageChanges(
  route: Schemas["RouteRef"],
  host: string,
  was: { maintenance: boolean; errorPages: boolean; pages?: Pages; defaults: Pages },
  form: PagesForm,
  /** Gets the domain back when the rules changed during maintenance. */
  workload?: Schemas["WorkloadRef"],
): Change[] {
  const starts = (form.maintenance && !was.maintenance) || (form.errorPages && !was.errorPages)
  const html = htmlChange(was.pages, was.defaults, form.edited, starts)
  return [
    ...html,
    ...(form.errorPages === was.errorPages
      ? []
      : [{ op: "errorPages", route, enabled: form.errorPages } as const]),
    ...(form.maintenance === was.maintenance
      ? []
      : [
          {
            op: "maintenance",
            route,
            hostname: host,
            enabled: form.maintenance,
            ...(form.maintenance ? {} : { workload }),
          } as const,
        ]),
  ]
}

/** All four pages the first time, else only those that changed. */
function htmlChange(
  pages: Pages | undefined,
  defaults: Pages,
  edited: Partial<Pages>,
  starts: boolean,
): Change[] {
  if (!pages) {
    const typed = PAGE_KEYS.some((key) => edited[key] !== undefined)
    return starts || typed ? [{ op: "errorPageHtml", ...defaults, ...edited }] : []
  }
  const changed = PAGE_KEYS.filter((key) => edited[key] !== undefined && edited[key] !== pages[key])
  if (changed.length === 0) return []
  return [{ op: "errorPageHtml", ...Object.fromEntries(changed.map((key) => [key, edited[key]])) }]
}
